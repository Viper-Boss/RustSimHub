#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded offline B regression entry. Evidence only, never build/install authority."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import queue
import re
import shutil
import stat
import subprocess
import sys
import threading
import tempfile
import time

from checks import CHECKS, EXCLUDED

ROOT = Path(__file__).resolve().parents[2]
LIMIT = 1024 * 1024


def git(root, *args):
    return subprocess.check_output(['git', '--no-replace-objects', '-c',
        'safe.directory=' + root.as_posix(), '-C', str(root), *args], timeout=20).strip()


def source(root):
    """Hash tracked and nonignored source, never enumerate ignored/private trees."""
    names = git(root, 'ls-files', '-z').split(b'\0')
    names += git(root, 'ls-files', '--others', '--exclude-standard', '-z').split(b'\0')
    digest = hashlib.sha256()
    total = 0
    for raw in sorted(set(names) - {b''}):
        name = raw.decode('utf-8')
        parts = Path(name).parts
        if any(p.casefold() in ('.local', '.private', '.git') for p in parts) or '..' in parts:
            raise ValueError('private/invalid source path refused before opening')
        path = root / name
        digest.update(raw + b'\0')
        if not path.exists() and not path.is_symlink():
            digest.update(b'deleted\0')
            continue
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode):
            data = os.readlink(path).encode()
        else:
            if not stat.S_ISREG(info.st_mode) or info.st_size > 4 * LIMIT:
                raise ValueError('source file type/size refused')
            data = path.read_bytes()
        total += len(data)
        if total > 64 * LIMIT:
            raise ValueError('source byte budget exceeded')
        digest.update(data + b'\0')
    return {'head': git(root, 'rev-parse', 'HEAD').decode(),
            'tree': git(root, 'rev-parse', 'HEAD^{tree}').decode(),
            'worktree_sha256': digest.hexdigest(),
            'dirty': bool(git(root, 'status', '--porcelain=v1', '--untracked-files=all'))}


def fresh_output(path, root=ROOT):
    path = Path(os.path.abspath(path))
    if path == root or root in path.parents:
        raise ValueError('output must be outside the source checkout')
    current = Path(path.anchor)
    for component in path.parts[1:]:
        if component.casefold() in ('.private', '.local', '.git'):
            raise ValueError('private output component refused')
        current /= component
        if current.is_symlink():
            raise ValueError('linked output refused')
        if os.name == 'nt' and current.exists() and current.stat().st_file_attributes & 0x400:
            raise ValueError('reparse output refused')
    if not path.parent.is_dir():
        raise ValueError('output parent must already exist')
    path.mkdir(mode=0o700)  # exclusive reservation, never reuse an old run
    return path


def capability(name):
    if name == 'linux':
        return sys.platform == 'linux'
    if name == 'browser':
        return importlib.util.find_spec('playwright') is not None
    if name == 'edge':
        return os.name == 'nt' and any((Path(os.environ.get(key, '')) /
            'Microsoft/Edge/Application/msedge.exe').is_file()
            for key in ('ProgramFiles', 'ProgramFiles(x86)'))
    if name == 'node-playwright':
        try:
            return subprocess.run(['node', '-e', "require('playwright').chromium || process.exit(1)"],
                capture_output=True, timeout=5).returncode == 0
        except (OSError, subprocess.SubprocessError):
            return False
    if name == 'symlink':
        try:
            with tempfile.TemporaryDirectory(prefix='offline-b-link-probe-') as temp:
                root = Path(temp)
                (root/'input').write_bytes(b'owned synthetic probe')
                (root/'link').symlink_to(root/'input')
                return (root/'link').is_symlink()
        except OSError:
            return False
    if name == 'node':
        try:
            result = subprocess.run(['node', '--version'], capture_output=True, timeout=5)
            match = re.fullmatch(rb'v(\d+)\.\d+\.\d+\s*', result.stdout)
            return result.returncode == 0 and match is not None and int(match[1]) >= 22
        except (OSError, subprocess.SubprocessError):
            return False
    return shutil.which(name) is not None


def environment():
    # Offline toolchain/module resolution. No downloads or production endpoints.
    env = os.environ.copy()
    # Only our verified clean Git archive may provide this compile-time stamp.
    env.pop('RUSTSIMHUB_BUILD_SOURCE_SHA256', None)
    env.update(CARGO_NET_OFFLINE='true', GOTOOLCHAIN='local', GOPROXY='off',
               GOSUMDB='off', TZ='UTC', PYTHONUTF8='1', WEB3_WEB_BROWSER_PYTHON=sys.executable)
    # Git fixture commits must not launch detached automatic maintenance while
    # their owned temporary repository is being removed (Windows holds cwd).
    # These finite process-local overrides never write user/repository config.
    env.update(GIT_CONFIG_COUNT='2', GIT_CONFIG_KEY_0='gc.auto', GIT_CONFIG_VALUE_0='0',
               GIT_CONFIG_KEY_1='maintenance.auto', GIT_CONFIG_VALUE_1='false')
    return env


def windows_execute(check, root, log, seconds, *, measure=False):
    from windows_job import Job
    result = {'execution': 'not_run', 'exit_code': None, 'reason': '', 'cleanup_complete': True,
              'bytes_seen': 0, 'summary': '', 'command': list(check.argv)}
    try:
        job = Job()
    except OSError:
        result['reason'] = 'Windows owned job isolation unavailable'
        return result
    process = None
    reader = None
    chunks = queue.Queue(maxsize=16)
    stop_reader = threading.Event()
    began = time.monotonic()
    tail = b''
    saved = 0
    resources = {'samples': 0, 'rss_samples': 0, 'rss_gaps': 0, 'measurement_errors': 0,
                 'peak_observed_rss_bytes': None}
    next_sample = 0
    try:
        with log.open('xb') as output:
            process = subprocess.Popen([sys.executable, '-I', '-S', str(Path(__file__).with_name('worker.py')),
                job.name, check.name], cwd=root, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, env=environment())
            result['execution'] = 'executed'

            def read():
                try:
                    while not stop_reader.is_set():
                        data = process.stdout.read1(65536)
                        if not data:
                            break
                        while not stop_reader.is_set():
                            try:
                                chunks.put(data, timeout=.05)
                                break
                            except queue.Full:
                                pass
                finally:
                    while not stop_reader.is_set():
                        try:
                            chunks.put(None, timeout=.05)
                            break
                        except queue.Full:
                            pass

            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            eof = False
            while not eof or process.poll() is None:
                if measure and time.monotonic() >= next_sample:
                    try:
                        sample = job.resources()
                        resources['samples'] += 1
                        resources['rss_gaps'] += sample['rss_gaps']
                        if sample['rss_bytes'] is not None:
                            resources['rss_samples'] += 1
                            resources['peak_observed_rss_bytes'] = max(
                                resources['peak_observed_rss_bytes'] or 0, sample['rss_bytes'])
                    except OSError:
                        resources['measurement_errors'] += 1
                    next_sample = time.monotonic() + .02
                if time.monotonic() - began >= seconds:
                    result.update(execution='timeout', reason='check deadline exceeded')
                    break
                try:
                    data = chunks.get(timeout=.05)
                except queue.Empty:
                    continue
                if data is None:
                    eof = True
                    continue
                result['bytes_seen'] += len(data)
                output.write(data[:max(0, LIMIT - saved)])
                saved = min(LIMIT, saved + len(data))
                tail = (tail + data)[-LIMIT:]
                if result['bytes_seen'] > LIMIT:
                    result.update(execution='output_limit', reason='bounded output exceeded')
                    break
            code = process.poll()
            # Parent success with live descendants is not a clean success.
            if code is not None and job.active() and result['execution'] == 'executed':
                result.update(execution='orphaned_children', reason='test left live descendants')
            job.stop()
            process.wait(timeout=3)
            until = time.monotonic() + 2
            while job.active() and time.monotonic() < until:
                time.sleep(.02)
            result.update(exit_code=process.returncode, cleanup_complete=job.active() == 0)
            if measure:
                try:
                    final = job.resources()
                    resources.update(cpu_seconds=final['cpu_seconds'],
                                     peak_job_committed_bytes=final['peak_job_committed_bytes'])
                except OSError:
                    resources['measurement_errors'] += 1
                result['resources'] = resources
    finally:
        # Closing the sole parent handle is a final kill-on-close guard, even on
        # Ctrl-C, log write error or an unexpected exception above.
        try:
            job.stop()
        finally:
            job.close()
        if process is not None:
            process.wait(timeout=3)
        stop_reader.set()
        if reader is not None:
            reader.join(timeout=3)
            if reader.is_alive():
                result['cleanup_complete'] = False
        if process is not None:
            process.stdout.close()
    result['summary'] = tail.decode('utf-8', errors='replace')
    result['duration_seconds'] = round(time.monotonic() - began, 3)
    if 'OFFLINE_B_JOB_UNAVAILABLE' in result['summary'] and 'OFFLINE_B_JOB_READY' not in result['summary']:
        result.update(execution='not_run', reason='child could not join owned job; no check was started')
    return result


def linux_backend():
    sys.path.insert(0, str(ROOT / 'tools/mainline-checks'))
    spec = importlib.util.spec_from_file_location('offline_b_process_backend', ROOT/'tools/mainline-checks/run.py')
    backend = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(backend)
    return backend


def source_stamp(backend, root, log, seconds):
    """Same git-archive contract as mainline; bounded, clean source only."""
    before = source(root)
    failure = {'execution': 'not_run', 'exit_code': None, 'cleanup_complete': True, 'summary': '',
               'reason': 'committed source stamp requires a clean checkout'}
    if before['dirty']:
        return None, failure
    argv = ['git', '--no-replace-objects', '-c', 'safe.directory='+root.as_posix(),
            '-C', str(root), 'archive', '--format=tar', 'HEAD']
    with tempfile.TemporaryDirectory(prefix='offline-b-source-') as directory:
        archive = Path(directory)/'source.tar'
        row = backend.execute(argv, root, archive, min(seconds, 30), limit=64*LIMIT, env=environment())
        if row['execution'] == 'not_run':
            failure['reason'] = 'committed archive process isolation unavailable'
            return None, failure
        if row['execution'] != 'executed' or row['exit_code'] != 0 or not row['cleanup_complete']:
            failure.update(execution='source_archive_failed', exit_code=row['exit_code'],
                           cleanup_complete=row['cleanup_complete'],
                           reason='committed archive failed: '+row['execution'])
        elif source(root) != before:
            failure.update(execution='source_changed', reason='source changed during committed archive')
        else:
            digest = hashlib.sha256()
            with archive.open('rb') as stream:
                for block in iter(lambda: stream.read(65536), b''):
                    digest.update(block)
            return digest.hexdigest(), {'source_archive_sha256': digest.hexdigest(),
                'source_archive_head': before['head'], 'source_archive_tree': before['tree'],
                'source_archive_bytes': archive.stat().st_size, 'source_archive_cleanup_complete': True,
                'source_archive_command': ['git', 'archive', '--format=tar', 'HEAD']}
    with log.open('x', encoding='utf-8') as stream:
        stream.write(failure['reason']+'\n')
    return None, failure


def execute(check, root, log, seconds):
    if os.name == 'nt':
        return windows_execute(check, root, log, seconds)
    if sys.platform == 'linux':
        # Reuse the existing pidfd/subreaper cleanup instead of a second Linux
        # process tracker. Its original cleanup regressions remain authoritative.
        backend = linux_backend()
        env, proof = environment(), {}
        if check.committed_source:
            began = time.monotonic()
            stamp, proof = source_stamp(backend, root, log, seconds)
            if stamp is None:
                return proof
            seconds -= time.monotonic()-began
            if seconds <= 0:
                with log.open('x', encoding='utf-8') as stream:
                    stream.write('check budget exhausted during committed archive\n')
                return {'execution': 'timeout', 'exit_code': None, 'cleanup_complete': True,
                        'summary': '', 'reason': 'check budget exhausted during committed archive', **proof}
            env['RUSTSIMHUB_BUILD_SOURCE_SHA256'] = stamp
        return {**backend.execute(list(check.argv), root, log, seconds, limit=LIMIT, env=env), **proof}
    return {'execution': 'not_run', 'reason': 'owned process isolation unsupported on this OS',
            'exit_code': None, 'cleanup_complete': True, 'summary': ''}


def classify(result, kind):
    if result['execution'] == 'not_run':
        return 'not_run'
    if result['execution'] != 'executed' or result['exit_code'] != 0 or not result['cleanup_complete']:
        return 'failed'
    text = result['summary']
    if kind == 'unittest':
        rows = re.findall(r'Ran (\d+) tests? in ', text)
        if not rows or sum(map(int, rows)) == 0 or not re.search(r'^OK(?:\s|$)', text, re.M):
            result['reason'] = 'successful process did not prove nonempty successful unittest execution'
            return 'failed'
        skipped = sum(map(int, re.findall(r'skipped=(\d+)', text)))
        result['counts'] = {'total': sum(map(int, rows)), 'skipped': skipped}
        if skipped:
            result['reason'] = 'required cases skipped; partial execution is not full coverage'
            return 'not_run'
    elif kind == 'rust':
        rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
        if not rows or not sum(int(r[0]) for r in rows):
            result['reason'] = 'zero executed Rust tests'
            return 'failed'
        ignored = sum(int(r[2]) for r in rows)
        result['counts'] = {'passed': sum(int(r[0]) for r in rows), 'ignored': ignored}
        if ignored:
            result['reason'] = 'required Rust cases ignored'
            return 'not_run'
    elif kind == 'node' and not re.search(r'^(?:PASS:|ok\b)|\bpassed\b', text, re.M):
        result['reason'] = 'missing existing fixture completion marker'
        return 'failed'
    return 'passed'


def run(checks, output, *, root=ROOT, budget=600):
    if not checks:
        raise ValueError('empty check selection')
    started = datetime.now(timezone.utc).isoformat()
    before = source(root)
    began = time.monotonic()
    results = []
    for check in checks:
        missing = [name for name in check.requires if not capability(name)]
        remaining = budget - (time.monotonic() - began)
        if missing or remaining <= 0:
            result = {'execution': 'not_run', 'exit_code': None, 'cleanup_complete': True,
                      'reason': 'missing prerequisites: ' + ', '.join(missing) if missing else 'total budget exhausted'}
            status = 'not_run'
        else:
            result = execute(check, root, output/(check.name+'.log'), min(check.seconds, remaining))
            status = classify(result, check.kind)
        results.append({'id': check.name, 'actions': check.actions, 'layer': check.layer,
                        'log': check.name+'.log' if result['execution'] != 'not_run' else None,
                        'status': status, **{k: v for k, v in result.items() if k != 'summary'}})
        print(check.name + ': ' + status, flush=True)
    after = source(root)
    changed = before != after
    code = 2 if changed or any(r['status'] == 'failed' for r in results) else (
        3 if any(r['status'] == 'not_run' for r in results) else 0)
    report = {'format': 1, 'kind': 'offline-b-test-evidence',
              'started_utc': started, 'ended_utc': datetime.now(timezone.utc).isoformat(),
              'source_before': before, 'source_after': after, 'source_changed': changed,
              'environment': {'os': platform.system(), 'architecture': platform.machine(),
                              'python': platform.python_version()},
              'exit_code': code, 'checks': results, 'excluded_or_unverified': EXCLUDED,
              'signed': False, 'epoch_assigned': False, 'installable': False,
              'health_authorized': False, 'hardware_tested': False, 'public_network_tested': False}
    with (output/'report.json').open('x', encoding='utf-8') as stream:
        json.dump(report, stream, ensure_ascii=True, indent=2)
        stream.write('\n')
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--group', choices=('all', 'materials', 'business', 'process', 'browser'), default='all')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--budget-seconds', type=int, default=600)
    args = parser.parse_args()
    selected = [c for c in CHECKS if args.group == 'all' or c.group == args.group]
    if args.list:
        print(json.dumps([c.__dict__ for c in selected], indent=2))
        return 0
    if args.output is None or not 1 <= args.budget_seconds <= 1800:
        parser.error('fresh --output and total budget 1..1800 seconds required')
    try:
        return run(selected, fresh_output(args.output), budget=args.budget_seconds)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print('Offline B refused: ' + str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
