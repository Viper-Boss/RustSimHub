#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded synthetic functional workloads; never a device benchmark or arbitrary runner."""
import argparse
import json
import hashlib
import os
import pathlib
import platform
import resource
import signal
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROFILES = {
    'sms': 'tools/test-web-sms-management.mjs',
    'call': 'tools/test-web-call-test.mjs',
    'forwarding': 'tools/test-web-forwarding.mjs',
    'media': 'tools/test-web-media.mjs',
}
SCOPE = 'synthetic-node-functional-fixtures-and-harness; not device idle or real network'
MAX_BYTES = 1024 * 1024


def plan():
    return {'schema': 1, 'mode': 'plan', 'runs_processes': False,
            'profiles': PROFILES,
            'unavailable': {'idle': 'requires actual approved device idle window',
                            'remote': 'requires approved same-function remote workload'},
            'scope': SCOPE, 'budgets': 'proposals only; no approved device thresholds',
            'limits': {'seconds': 20, 'warmup_seconds': 2, 'children': 32,
                       'output_bytes': MAX_BYTES, 'worker_iterations': 100},
            'units': {'cpu': 'process-tree CPU seconds / monotonic elapsed seconds * 100',
                      'rss': 'sum of observed resident pages * host page size; bytes'}}


def parse_stat(raw):
    # comm may contain spaces and parentheses. Fields follow the final ')'.
    fields = raw[raw.rindex(')') + 2:].split()
    return {'ppid': int(fields[1]), 'pgrp': int(fields[2]),
            'ticks': sum(int(fields[i]) for i in (11, 12, 13, 14)),
            'start': int(fields[19]), 'rss': int(fields[21]) * os.sysconf('SC_PAGE_SIZE')}


def read_stat(pid):
    try:
        return parse_stat(pathlib.Path(f'/proc/{pid}/stat').read_text())
    except (FileNotFoundError, ProcessLookupError):
        return None


def sample_tree(pid, birth):
    root = read_stat(pid)
    if root is None or root['start'] != birth:
        return None
    todo, seen, rows = [pid], set(), []
    while todo:
        child = todo.pop()
        if child in seen:
            continue
        seen.add(child)
        if len(seen) > 32:
            raise RuntimeError('child budget exceeded')
        row = read_stat(child)
        if row is None:
            continue
        if row['pgrp'] != pid:
            raise RuntimeError('unexpected process group')
        rows.append((child, row))
        # Only inspect descendants of our own child, never enumerate host processes.
        try:
            for task in pathlib.Path(f'/proc/{child}/task').iterdir():
                try:
                    todo.extend(map(int, (task / 'children').read_text().split()))
                except (FileNotFoundError, ProcessLookupError):
                    pass
        except (FileNotFoundError, ProcessLookupError):
            pass
    return {'ticks': sum(r['ticks'] for _, r in rows),
            'rss_bytes': sum(max(0, r['rss']) for _, r in rows),
            'processes': len(rows), 'identities': [(p, r['start']) for p, r in rows]}


def delta(previous, current, elapsed):
    if previous is None or current is None or elapsed <= 0:
        return None
    ticks = current['ticks'] - previous['ticks']
    if ticks < 0:
        return None  # Exit/reap races or counter replacement are gaps, not negative CPU.
    return ticks / os.sysconf('SC_CLK_TCK') / elapsed * 100


def worker(profile, deadline):
    deadline = min(deadline, time.monotonic() + 22)
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_BYTES, MAX_BYTES))
    source = ROOT / PROFILES[profile]
    script = source.resolve(strict=True)
    if not script.is_relative_to(ROOT) or source.is_symlink():
        return 2
    progress = pathlib.Path(os.environ['TMPDIR']) / 'progress.json'
    completed = 0
    for _ in range(100):
        if time.monotonic() >= deadline:
            return 0
        try:
            result = subprocess.run(['node', str(script)], cwd=ROOT, check=False, timeout=max(.1, deadline-time.monotonic()))
        except subprocess.TimeoutExpired:
            return 0
        if result.returncode:
            return 3
        completed += 1
        progress.write_text(json.dumps({'completed_iterations': completed}))
    return 0


def provenance(profile):
    """Capture both boundaries; metadata collection is outside the sample window."""
    return {
        'source_sha': subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True, timeout=3).strip(),
        'source_dirty': bool(subprocess.check_output(['git', '-C', str(ROOT), 'status', '--porcelain'], timeout=3)),
        'runner_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        'workload_sha256': hashlib.sha256((ROOT / PROFILES[profile]).read_bytes()).hexdigest(),
        'environment': {'os': platform.system(), 'architecture': platform.machine(),
                        'python': platform.python_version(),
                        'node': subprocess.check_output(['node', '--version'], text=True, timeout=3).strip(),
                        'clock_ticks': os.sysconf('SC_CLK_TCK')},
    }


def measure(profile, seconds=5, warmup=.5, interval=.1, cancel=None):
    if profile not in PROFILES or not (.5 <= seconds <= 20 and 0 <= warmup <= 2 and .05 <= interval <= 1):
        raise ValueError('bounded profile/window required')
    if platform.system() != 'Linux':
        raise ValueError('Linux /proc required')
    source = provenance(profile)
    started = time.monotonic()
    samples, identities, previous, previous_at = [], [], None, None
    outcome = 'completed'
    with tempfile.TemporaryDirectory(prefix='rsh-performance-') as temporary:
        log_path = pathlib.Path(temporary) / 'worker.log'
        with log_path.open('wb') as log:
            proc = subprocess.Popen([sys.executable, str(pathlib.Path(__file__).resolve()),
                                     '--_worker', profile, str(started + seconds + warmup)],
                                    cwd=temporary, stdout=log, stderr=subprocess.STDOUT,
                                    start_new_session=True, env={**os.environ, 'TMPDIR': temporary})
            initial = read_stat(proc.pid)
            if initial is None:
                proc.wait(timeout=5)
                raise RuntimeError('worker exited before identity capture')
            try:
                while proc.poll() is None:
                    now = time.monotonic()
                    if cancel and cancel():
                        outcome = 'cancelled'
                        break
                    if now - started >= seconds + warmup:
                        break
                    if log_path.stat().st_size > MAX_BYTES:
                        outcome = 'output_limit'
                        break
                    current = sample_tree(proc.pid, initial['start'])
                    if current is None:
                        outcome = 'identity_lost'
                        break
                    identities = current.pop('identities')
                    if now - started >= warmup:
                        cpu = delta(previous, current, now - previous_at) if previous_at is not None else None
                        samples.append({'elapsed_s': now - started - warmup,
                                        'cpu_percent': cpu, 'rss_bytes': current['rss_bytes'],
                                        'processes': current['processes']})
                        previous, previous_at = current, now
                    time.sleep(interval)
                if proc.poll() not in (None, 0):
                    outcome = 'workload_failed'
                measured_seconds = max(0, time.monotonic() - started - warmup)
                if outcome == 'completed' and measured_seconds < seconds:
                    outcome = 'window_incomplete'
            finally:
                # Our session leader is still alive, or a sampled descendant proves ownership.
                owned = proc.poll() is None and read_stat(proc.pid) is not None
                owned = owned or any((r := read_stat(pid)) is not None and r['start'] == birth
                                     and r['pgrp'] == proc.pid for pid, birth in identities)
                if owned:
                    try:
                        os.killpg(proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                proc.wait(timeout=5)
        if log_path.stat().st_size >= MAX_BYTES:
            outcome = 'output_limit'
        try:
            completed = json.loads((pathlib.Path(temporary) / 'progress.json').read_text())['completed_iterations']
        except (FileNotFoundError, ValueError, KeyError):
            completed = 0
        if outcome == 'completed' and completed == 0:
            outcome = 'no_completed_iteration'
        cleanup = not any((r := read_stat(pid)) is not None and r['start'] == birth
                          and pathlib.Path(f'/proc/{pid}/stat').exists()
                          and r['pgrp'] == proc.pid for pid, birth in identities)
        # Killed descendants can briefly remain zombies; they hold no running workload.
        if not cleanup:
            for _ in range(20):
                alive = []
                for pid, birth in identities:
                    r = read_stat(pid)
                    if r is not None and r['start'] == birth:
                        try:
                            state = pathlib.Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                            if state != 'Z':
                                alive.append(pid)
                        except FileNotFoundError:
                            pass
                if not alive:
                    cleanup = True
                    break
                time.sleep(.05)
    cpu = [s['cpu_percent'] for s in samples if s['cpu_percent'] is not None]
    after = provenance(profile)
    source_changed = source != after
    if source_changed and outcome == 'completed':
        outcome = 'source_changed'
    return {'schema': 3, 'kind': 'synthetic-functional', 'scope': SCOPE,
            **source, 'profile': profile, 'source_changed': source_changed,
            'source_dirty': source['source_dirty'] or after['source_dirty'],
            'completed_iterations': completed, 'outcome': outcome,
            'cleanup_verified': cleanup, 'requested_seconds': seconds, 'warmup_seconds': warmup,
            'sample_interval_seconds': interval, 'measured_seconds': measured_seconds,
            'samples': samples, 'sample_count': len(samples),
            'mean_cpu_percent': statistics.mean(cpu) if cpu else None,
            'peak_rss_bytes': max((s['rss_bytes'] for s in samples), default=None),
            'budget_approved': False, 'device_performance_verified': False,
            'limitations': ['includes test/runtime launch overhead',
                           'short-lived child exit/reap can produce CPU gaps',
                           'RSS is observed tree sum, not peak RSS of exited children',
                           'no carrier traffic, real calls, real forwarding or remote connection']}


def write_report(path, value):
    data = (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + '\n').encode()
    if len(data) > MAX_BYTES:
        raise ValueError('report limit')
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', choices=sorted(PROFILES))
    parser.add_argument('--seconds', type=float, default=5)
    parser.add_argument('--warmup', type=float, default=.5)
    parser.add_argument('--interval', type=float, default=.1)
    parser.add_argument('--output', type=pathlib.Path)
    args = parser.parse_args()
    if args.run is None:
        print(json.dumps(plan(), ensure_ascii=False, indent=2))
        return 0
    if args.output is None:
        parser.error('--run requires an explicit new --output file')
    if os.path.lexists(args.output):
        parser.error('output already exists; no workload started')
    cancelled = False
    def stop(_sig, _frame):
        nonlocal cancelled
        cancelled = True
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    result = measure(args.run, args.seconds, args.warmup, args.interval, lambda: cancelled)
    write_report(args.output, result)
    print(json.dumps({k: result[k] for k in ['outcome', 'sample_count', 'cleanup_verified', 'device_performance_verified']}))
    return 0 if result['outcome'] == 'completed' and result['cleanup_verified'] else 2


if __name__ == '__main__':
    if len(sys.argv) == 4 and sys.argv[1] == '--_worker' and sys.argv[2] in PROFILES:
        raise SystemExit(worker(sys.argv[2], float(sys.argv[3])))
    raise SystemExit(main())
