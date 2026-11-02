#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed, local development regression checks; never release authorization."""
import argparse
import ctypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import selectors
import shutil
import signal
import subprocess
import sys
import time

from checklist import CHECKS, GROUPS

ROOT = Path(__file__).resolve().parents[2]
LOG_LIMIT = 2 * 1024 * 1024
TAIL_LIMIT = 16384
REPORT_PATH = 'docs/testing/WEB-CODEX-MAINLINE-REGRESSION-20261103.md'


def now():
    return datetime.now(timezone.utc).isoformat()


def small(argv, cwd=ROOT):
    return subprocess.run(argv, cwd=cwd, capture_output=True, timeout=15, check=True).stdout.decode().strip()


def source(root):
    sha = small(['git', 'rev-parse', 'HEAD'], root)
    status = small(['git', 'status', '--porcelain=v1', '--untracked-files=all'], root)
    paths = small(['git', 'diff', '--name-only', 'HEAD'], root).splitlines()
    paths += small(['git', 'ls-files', '--others', '--exclude-standard'], root).splitlines()
    digest = hashlib.sha256()
    for name in sorted(set(paths)):
        p = root / name
        digest.update(name.encode() + b'\0')
        if p.is_file() and not p.is_symlink():
            with p.open('rb') as stream:
                for chunk in iter(lambda: stream.read(65536), b''):
                    digest.update(chunk)
        elif p.is_symlink():
            digest.update(os.readlink(p).encode())
    return {'sha': sha, 'dirty': bool(status), 'status': status,
            'changed_paths': sorted(set(paths)), 'changes_sha256': digest.hexdigest(),
            'tested_code_changed': any(not (p.startswith('tools/mainline-checks/') or p == REPORT_PATH) for p in paths)}


def reserve_output(root, value):
    """Only a fresh directory below real .local; no source/report overwrite."""
    requested = Path(value)
    if requested.is_absolute() or '..' in requested.parts or requested.suffix != '.json':
        raise ValueError('output must be a relative .local/.../report.json path')
    if len(requested.parts) < 3 or requested.parts[0] != '.local':
        raise ValueError('output needs a fresh directory below .local')
    current = root
    for part in requested.parts[:-1]:
        current = current / part
        if current.is_symlink():
            raise ValueError('symlink output component refused')
    destination = root / requested
    if destination.parent.exists():
        raise ValueError('run directory already exists; choose a new directory')
    # Refuse tracked paths even if someone tracks a path under .local.
    tracked = small(['git', 'ls-files', '--', requested.parts[0]], root)
    if tracked:
        raise ValueError('.local contains tracked files')
    destination.parent.mkdir(parents=True, mode=0o700)
    destination.parent.chmod(0o700)
    return destination


def subreaper():
    if sys.platform != 'linux':
        raise ValueError('Linux is required for process group cleanup')
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), 'cannot enable child subreaper')


def group_alive(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False


def reap_group(pgid):
    while True:
        try:
            pid, _ = os.waitpid(-pgid, os.WNOHANG)
            if pid == 0:
                break
        except ChildProcessError:
            break


def stop_group(proc):
    """TERM, bounded grace, KILL, wait parent and reap adopted descendants."""
    for sig, grace in ((signal.SIGTERM, .5), (signal.SIGKILL, 2)):
        try:
            os.killpg(proc.pid, sig)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            proc.poll()
            if proc.returncode is not None:
                reap_group(proc.pid)
            if not group_alive(proc.pid):
                break
            time.sleep(.02)
        if not group_alive(proc.pid):
            break
    proc.wait(timeout=2)
    reap_group(proc.pid)
    return not group_alive(proc.pid)


def execute(argv, cwd, log, timeout, *, limit=LOG_LIMIT, artifact=None, env=None):
    """Internal primitive; CLI exposes neither argv nor environment overrides."""
    subreaper()
    start = now()
    began = time.monotonic()
    result = {'command': argv, 'cwd': str(cwd), 'started': start, 'ended': None,
              'exit_code': None, 'execution': 'not_run', 'reason': '',
              'log': log.name, 'bytes_seen': 0, 'bytes_saved': 0, 'truncated': False,
              'cleanup_complete': True, 'pid': None}
    tail = b''
    proc = None
    artifact_stream = None
    try:
        with log.open('xb') as output:
            if artifact:
                artifact_stream = artifact.open('xb')
            try:
                proc = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.DEVNULL,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        start_new_session=True, env=env)
            except FileNotFoundError:
                result['reason'] = 'missing executable: ' + argv[0]
                return result
            result.update(execution='executed', pid=proc.pid)
            with selectors.DefaultSelector() as selector:
                for stream in (proc.stdout, proc.stderr):
                    os.set_blocking(stream.fileno(), False)
                    selector.register(stream, selectors.EVENT_READ)
                while selector.get_map() or proc.poll() is None:
                    if time.monotonic() - began >= timeout:
                        result['execution'] = 'timeout'
                        result['reason'] = 'deadline exceeded; terminated the original process group'
                        break
                    for key, _ in selector.select(.05):
                        data = os.read(key.fd, 65536)
                        if not data:
                            selector.unregister(key.fileobj)
                            continue
                        result['bytes_seen'] += len(data)
                        remaining = max(0, limit - result['bytes_saved'])
                        saved = data[:remaining]
                        output.write(saved)
                        result['bytes_saved'] += len(saved)
                        tail = (tail + data)[-TAIL_LIMIT:]
                        if artifact_stream and key.fileobj is proc.stdout:
                            artifact_stream.write(data[:max(0, limit - artifact_stream.tell())])
                        if result['bytes_seen'] > limit:
                            result['truncated'] = True
                            result['reason'] = 'log limit exceeded; terminated the original process group'
                            result['execution'] = 'output_limit'
                            break
                    if result['execution'] != 'executed':
                        break
            # Even a successful parent must not leave background children behind.
            parent_code = proc.poll()
            if parent_code is not None:
                reap_group(proc.pid)
            if parent_code is not None and group_alive(proc.pid) and result['execution'] == 'executed':
                result['reason'] = 'parent exited with live descendants'
                result['execution'] = 'orphaned_children'
            result['cleanup_complete'] = stop_group(proc)
            result['exit_code'] = proc.returncode
    except BaseException:
        if proc is not None:
            stop_group(proc)
        raise
    finally:
        if artifact_stream:
            artifact_stream.close()
        if proc:
            proc.stdout.close()
            proc.stderr.close()
        result['ended'] = now()
        result['duration_seconds'] = round(time.monotonic() - began, 3)
        result['summary'] = tail.decode('utf-8', errors='replace')
    return result


def classify(result, kind):
    if result['execution'] == 'not_run':
        return 'not_run'
    if (result['execution'] != 'executed' or result['exit_code'] != 0
            or not result['cleanup_complete']):
        return 'failed'
    text = result['summary']
    if kind == 'rust':
        rows = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
        # Rust output may exceed the tail. Counters are also parsed from bounded log.
        count = sum(int(r[0]) for r in rows)
        result['test_counts'] = {'passed': count, 'failed': sum(int(r[1]) for r in rows), 'ignored': sum(int(r[2]) for r in rows)}
        if count == 0:
            result['reason'] = 'no executed Rust tests in successful output'
            return 'failed'
    elif kind == 'unittest':
        rows = re.findall(r'Ran (\d+) tests? in ', text)
        if not rows or sum(map(int, rows)) == 0:
            result['reason'] = 'unittest selected zero tests or did not report a count'
            return 'failed'
        skipped = sum(map(int, re.findall(r'skipped=(\d+)', text)))
        result['test_counts'] = {'total': sum(map(int, rows)), 'skipped': skipped}
        if skipped:
            result['reason'] = 'unittest skipped required cases; see log'
            return 'not_run'
    elif kind == 'go':
        count = len(re.findall(r'^--- PASS: ', text, re.MULTILINE))
        result['test_counts'] = {'passed': count}
        if not count:
            result['reason'] = 'Go selected zero tests or did not report a count'
            return 'failed'
    return 'passed'


def probe(argv):
    try:
        r = subprocess.run(argv, capture_output=True, timeout=10)
        return {'command': argv, 'exit_code': r.returncode,
                'summary': (r.stdout + r.stderr)[:4096].decode(errors='replace')}
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {'command': argv, 'exit_code': None, 'summary': type(exc).__name__}


def environment():
    versions = {name: probe(argv) for name, argv in {
        'rustc': ['rustc', '--version'], 'cargo': ['cargo', '--version'],
        'node': ['node', '--version'], 'python': ['python3', '--version'],
        'go': ['go', 'version'], 'audio': ['pkg-config', '--modversion', 'alsa', 'opus'],
    }.items()}
    capabilities = {'go': versions['go']['exit_code'] == 0, 'audio': versions['audio']['exit_code'] == 0}
    details = {}
    root = os.geteuid() == 0
    if not root:
        details['root'] = probe(['sudo', '-n', 'true'])
        root = details['root']['exit_code'] == 0
    capabilities['root'] = root
    prefix = [] if os.geteuid() == 0 else ['sudo', '-n']
    for name, args in [('netns', ['unshare', '--net', 'true']),
                       ('mount', ['unshare', '--mount', '--propagation', 'private', 'true'])]:
        details[name] = probe(prefix + args) if root else {'summary': 'Root unavailable', 'exit_code': None}
        capabilities[name] = details[name]['exit_code'] == 0
    capabilities['systemd'] = Path('/proc/1/comm').read_text().strip() == 'systemd' and Path('/sys/fs/cgroup/cgroup.controllers').exists()
    capabilities['polkit'] = shutil.which('pkcheck') is not None
    disk = shutil.disk_usage(ROOT)
    return {'os': dict(zip(('sysname', 'nodename', 'release', 'version', 'machine'), os.uname())), 'uid': os.getuid(), 'euid': os.geteuid(),
            'disk_free_bytes': disk.free, 'versions': versions, 'capabilities': capabilities,
            'probes': details,
            # Explicit finite build settings only; never serialize os.environ.
            'build_settings': {k: os.environ[k] for k in ('CARGO_BUILD_JOBS', 'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG') if k in os.environ}}


def select_checks(groups):
    if not groups or any(g not in GROUPS for g in groups):
        raise ValueError('unknown or empty group selection')
    selected = [c for c in CHECKS if c.group in groups]
    if not selected:
        raise ValueError('selection contains zero checks')
    return selected


def totals(rows, dirty=False):
    counts = {state: sum(r['status'] == state for r in rows) for state in ('passed', 'failed', 'not_run')}
    status = 'failed' if counts['failed'] else ('not_run' if counts['not_run'] or dirty or not rows else 'passed')
    return {'status': status, **counts}


def run(groups, output, root=ROOT):
    checks = select_checks(groups)
    original = source(root)
    path = reserve_output(root, output)
    env = environment()
    report = {'schema': 1, 'purpose': 'development checks, not release approval',
              'groups': groups, 'started': now(), 'source': original, 'environment': env,
              'checklist_sha256': hashlib.sha256(Path(__file__).with_name('checklist.py').read_bytes()).hexdigest(),
              'checks': [], 'ended': None}

    def save():
        report['totals'] = totals(report['checks'], original['dirty'])
        temporary = path.with_suffix('.tmp')
        temporary.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
        os.replace(temporary, path)

    save()
    for c in checks:
        before = source(root)
        argv = [arg.replace('{run}', str(path.parent)).replace('{user}', pwd.getpwuid(os.getuid()).pw_name) for arg in c.argv]
        if os.geteuid() == 0 and argv[:2] == ['sudo', '-n']:
            argv = argv[2:]
        missing = [name for name in c.requires if not env['capabilities'].get(name)]
        missing += [name for name in (argv[0],) if shutil.which(name) is None]
        missing += [name for name in c.files if not (root / name).exists()]
        previous = {r['name']: r for r in report['checks']}
        missing += ['dependency: ' + name for name in c.depends if previous.get(name, {}).get('status') != 'passed']
        if before != original:
            missing.append('source changed since run start')
        base = {'name': c.name, 'group': c.group, 'source': before, 'command': argv,
                'cwd': str(root / c.cwd), 'timeout_seconds': c.timeout, 'kind': c.kind}
        if missing:
            base.update(status='not_run', execution='not_run', reason='unavailable: ' + ', '.join(missing),
                        started=now(), ended=now(), exit_code=None, summary='')
        else:
            command_env = None
            if c.committed_source:
                archive = subprocess.check_output(['git', 'archive', '--format=tar', 'HEAD'], cwd=root, timeout=30)
                archive_sha = hashlib.sha256(archive).hexdigest()
                command_env = dict(os.environ, RUSTSIMHUB_BUILD_SOURCE_SHA256=archive_sha)
                base['fixed_environment'] = {'RUSTSIMHUB_BUILD_SOURCE_SHA256': archive_sha}
                base['source_archive_command'] = ['git', 'archive', '--format=tar', 'HEAD']
            print('START ' + c.name, flush=True)
            result = execute(argv, root / c.cwd, path.parent / (c.name + '.log'), c.timeout, env=command_env,
                             artifact=path.parent / c.artifact if c.artifact else None)
            # Classification sees the complete bounded output, including early Rust suites.
            if result['execution'] != 'not_run':
                result['summary'] = (path.parent / (c.name + '.log')).read_text(errors='replace')
            base.update(result)
            base['status'] = classify(base, c.kind)
            base['summary'] = base['summary'][-TAIL_LIMIT:]
            after = source(root)
            base['source_after'] = after
            if after != before:
                base['status'] = 'failed'
                base['reason'] = 'source changed during check'
        report['checks'].append(base)
        save()
        print(c.name + ': ' + base['status'] + (' (' + base['reason'] + ')' if base.get('reason') else ''), flush=True)
        if base.get('cleanup_complete') is False:
            # Do not start another command if the original tree was not reclaimed.
            break
    report['ended'] = now()
    if len(report['checks']) != len(checks):
        for c in checks[len(report['checks']):]:
            report['checks'].append({'name': c.name, 'group': c.group, 'status': 'not_run', 'execution': 'not_run', 'command': list(c.argv), 'source': source(root), 'started': now(), 'ended': now(), 'exit_code': None, 'reason': 'previous process cleanup incomplete'})
    save()
    print(json.dumps(report['totals']), flush=True)
    return 0 if report['totals']['status'] == 'passed' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--group', action='append', choices=GROUPS)
    parser.add_argument('--output', default='.local/mainline-checks/report.json')
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    if args.list:
        for c in CHECKS:
            print(json.dumps({'name': c.name, 'group': c.group, 'command': c.argv, 'requires': c.requires, 'depends': c.depends, 'cwd': c.cwd, 'timeout_seconds': c.timeout}))
        return 0
    try:
        return run(args.group or ['ordinary'], args.output)
    except (ValueError, OSError, subprocess.SubprocessError) as exc:
        print('mainline-checks: ' + str(exc), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
