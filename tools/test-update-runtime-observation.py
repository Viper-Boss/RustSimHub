#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the actual fixed-path CLI only in a newly created private Linux container.

Requires the managed local Docker daemon, an already present Python 3 Linux
image (no automatic pull), and a compatible native updater. The host entry
never creates/reads a real installation, starts services or accesses devices.
Root, private PID/filesystem namespaces, network=none, read-only image and
explicit tmpfs fixture roots are mandatory. The systemctl fixture is synthetic,
NOT PID1. Actual ARM/systemd success must be validated separately. Exit 77 means
an unavailable prerequisite; an executed test failure is never a skip.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
import uuid

APP = Path('/var/lib/rustsimhub/rustsimhub-v1/system/app')
TRUST = Path('/usr/share/rustsimhub/release.public')
VERSIONS = Path('/opt/rustsimhub/versions')
SELECTED = Path('/run/rustsimhub-app/selected')
MARKER = Path('/run/runtime-observation-fixture')
BINARY = Path('/fixture/updater')
CURRENT_CASE = 'startup'
READ_CHECKS = 0
FIXTURE_MOUNTS = ['/var/lib/rustsimhub', '/usr/share/rustsimhub',
                  '/opt/rustsimhub', '/run/rustsimhub-app', '/usr/bin',
                  '/run/runtime-observation-fixture', '/tmp']
DOCKER_SELECTORS = ['DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS',
                    'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH']

SYSTEMCTL = r'''#!/usr/local/bin/python3
import json,os,sys,time
from pathlib import Path
# All caller bus/remote/search settings must have been removed by production.
for name in ['SYSTEMD_HOST','SYSTEMD_MACHINE','SYSTEMD_BUS_ADDRESS',
             'DBUS_SYSTEM_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS','LD_PRELOAD']:
    if name in os.environ: raise SystemExit(71)
mode = Path('/run/runtime-observation-fixture/mode').read_text()
if mode == 'hang': time.sleep(20)
if mode == 'overflow': print('x'*5000); raise SystemExit(0)
if mode == 'exit': raise SystemExit(7)
if mode == 'bad-utf8': sys.stdout.buffer.write(b'\xff'); raise SystemExit(0)
unit = sys.argv[-1]
roles = ['rustsimhub-core','rustsimhub-edge','rustsimhub-media','rustsimhub-forwarder']
if unit not in [r+'.service' for r in roles]: raise SystemExit(72)
i = roles.index(unit[:-8])
pid = 0 if i == 3 else os.getppid()
fields = dict(Id=unit,LoadState='loaded',ActiveState='inactive' if i==3 else 'active',
              SubState='dead' if i==3 else 'running',MainPID=str(pid),User=roles[i],
              ControlGroup='' if i==3 else '/system.slice/'+unit,
              NeedDaemonReload='no',InvocationID='' if i==3 else 'a'*32)
if mode == 'wrong-user': fields['User'] = 'root'
if mode == 'foreign-cgroup': fields['ControlGroup'] = '/foreign/'+unit
if mode == 'wrong-unit': fields['Id'] = 'other.service'
if mode == 'bad-pid': fields['MainPID'] = '01'
if mode == 'missing': del fields['User']
if mode == 'not-found': fields['LoadState'] = 'not-found'
if mode == 'pending-reload': fields['NeedDaemonReload'] = 'yes'
if mode == 'malformed': print('not-properties'); raise SystemExit(0)
for k,v in fields.items(): print(k+'='+v)
if mode == 'duplicate': print('MainPID='+str(pid))
'''


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(arguments, *, uid=0, env=None):
    options = {}
    if uid:
        options.update(user=uid, group=uid, extra_groups=[])
    return subprocess.run([str(BINARY), *map(str, arguments)],
                          stdin=subprocess.DEVNULL, capture_output=True,
                          timeout=8, env=env, **options)


def put(path, data, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
    path.write_bytes(data)
    path.chmod(mode)


def inventory():
    """No atime comparison: ordinary reads can update filesystem access time."""
    result = {}
    for base in map(Path, FIXTURE_MOUNTS):
        paths = [base, *base.rglob('*')] if base.exists() else []
        for path in paths:
            m = path.lstat()
            content = os.readlink(path).encode() if path.is_symlink() else (
                path.read_bytes() if stat.S_ISREG(m.st_mode) else b'')
            result[str(path)] = [m.st_mode, m.st_uid, m.st_gid, m.st_ino,
                                 m.st_nlink, m.st_mtime_ns, m.st_ctime_ns,
                                 hashlib.sha256(content).hexdigest()]
    return result


def reset():
    # This is reachable only inside the runner-created mount namespace. All
    # destructive preparation is restricted to its seven fresh tmpfs roots.
    for root in FIXTURE_MOUNTS:
        directory = Path(root)
        for entry in directory.iterdir():
            if entry.is_dir() and not entry.is_symlink():
                shutil.rmtree(entry)
            else:
                entry.unlink()
    keys = Path('/tmp/keys'); keys.mkdir(mode=0o700)
    incoming = Path('/tmp/incoming'); incoming.mkdir(mode=0o700)
    package = Path('/tmp/package'); package.mkdir(mode=0o700)
    elf = bytearray(64)
    elf[:7] = b'\x7fELF\x02\x01\x01'
    elf[16] = 2; elf[18] = 183; elf[52] = 64
    for name in ['rustsimhub-core', 'simhubctl', 'rustsimhub-edge',
                 'rustsimhub-audio-alsa', 'rustsimhub-forwarder']:
        put(incoming/name, elf)
    put(incoming/'Opus-COPYING.txt', b'synthetic license fixture')
    for args in [['keygen', keys], ['make-forwarding', keys/'release.seed', 'a'*64,
                                  '1', incoming, package]]:
        result = run(args)
        assert result.returncode == 0, 'synthetic package preparation failed'
    put(TRUST, (keys/'release.public').read_bytes(), 0o444)
    APP.mkdir(parents=True, mode=0o700)
    result = run(['install-factory', APP, TRUST, package])
    assert result.returncode == 0, 'synthetic ledger preparation failed'
    VERSIONS.mkdir(parents=True, mode=0o755)
    state = json.loads((APP/'state.json').read_bytes())
    digest = state['confirmed']['hash']
    shutil.copytree(APP/'versions'/digest, VERSIONS/digest)
    SELECTED.symlink_to(VERSIONS/digest)
    # Synthetic sentinels are checked but never read by the observer.
    put(APP.parent/'synthetic-business-data', b'fixture only, never real business data')
    put(MARKER/'mode', b'good')
    return state, VERSIONS/digest


def check(arguments, *, success=False, uid=0, error=None, env=None):
    global READ_CHECKS
    READ_CHECKS += 1
    before = inventory(); start = time.monotonic()
    result = run(arguments, uid=uid, env=env)
    elapsed = time.monotonic()-start
    assert inventory() == before, 'observer modified protected fixture sources'
    assert elapsed < 3, 'bounded CLI observation did not return in three seconds'
    assert len(result.stdout) <= 4096 and len(result.stderr) <= 256
    assert (result.returncode == 0) == success, 'unexpected CLI exit'
    if success:
        assert not result.stderr
        return json.loads(result.stdout)
    assert not result.stdout, 'rejection emitted a partial success report'
    assert re.fullmatch(rb'application update denied: [A-Za-z]+\n', result.stderr)
    if error:
        assert result.stderr == ('application update denied: '+error+'\n').encode(), (
            CURRENT_CASE+': unexpected rejection category: '+result.stderr.decode())
    return None


def isolated_child():
    # Do not accept just a flag or /.dockerenv as permission to touch fixed paths.
    # These exact mounts and a private network must exist before any fixture
    # preparation, and the host runner is the only public entry.
    assert os.geteuid() == 0
    assert socket.if_nameindex() == [(1, 'lo')]
    mounts = [line.split() for line in Path('/proc/self/mountinfo').read_text().splitlines()]
    for root in FIXTURE_MOUNTS:
        assert any(parts[4] == root and parts[parts.index('-')+1] == 'tmpfs'
                   for parts in mounts), 'missing mandatory private tmpfs'
    assert any(parts[4] == '/' and 'ro' in parts[5].split(',') for parts in mounts)
    assert any(parts[4] == '/fixture' and 'ro' in parts[5].split(',') for parts in mounts)
    info = json.loads(run(['--build-info']).stdout)
    assert info['read_only_runtime_inspection_protocol'] == 1
    assert info['read_only_state_inspection_protocol'] == 1
    results = []
    cases = ['state-control', 'systemd-unavailable', 'runtime-unavailable', 'nonroot',
             'missing-state', 'missing-lock', 'missing-trust', 'missing-app',
             'pending', 'bad-pending', 'linked-pending', 'writer-busy', 'wrong-trust',
             'bad-signature', 'bad-payload', 'bad-manifest', 'oversize-manifest',
             'missing-slot', 'missing-selected', 'escape-selected', 'relative-selected',
             'pending-selected', 'staged-not-running', 'attempted-candidate',
             'attempted-wrong-slot', 'fake-health-json', 'extra-arguments',
             'invalid-before-writable-store', 'bad-systemd', 'environment-cleared',
             'repeated-calls']
    global CURRENT_CASE
    for case in cases:
        CURRENT_CASE = case
        state, slot = reset()
        if case == 'state-control':
            value = check(['inspect-state'], success=True)
            assert value['scope'] == 'application-ledger' and value['read_only']
            assert value['health_verified'] is False
        elif case == 'nonroot':
            control = run(['--build-info'], uid=65534)
            assert control.returncode == 0
            check(['inspect-runtime'], uid=65534, error='Denied')
        elif case in ['extra-arguments', 'invalid-before-writable-store', 'fake-health-json']:
            put(APP/'state.next', json.dumps(dict(state, floor=2)).encode())
            shutil.rmtree(APP/'versions')
            if case == 'fake-health-json':
                arguments = [['inspect-runtime', '{"health_verified":true,"may_confirm":true}']]
            elif case == 'extra-arguments':
                arguments = [['inspect-runtime', '--pid', '1'], ['inspect-runtime', '--unit', 'x'],
                             ['inspect-runtime', '--root', APP], ['inspect-runtime', '--now-ms', '0']]
            else:
                arguments = [['inspect-runtime', APP, TRUST], ['inspect-runtime', APP],
                             ['inspect-runtime', APP, TRUST, 'extra']]
            for args in arguments: check(args, error='Invalid')
            assert not (APP/'versions').exists() and (APP/'state.next').exists()
        else:
            if case not in ['systemd-unavailable']:
                put(Path('/usr/bin/systemctl'), SYSTEMCTL.encode(), 0o555)
            error = None
            if case.startswith('missing-'):
                target = {'missing-state': APP/'state.json', 'missing-lock': APP/'.update.lock',
                          'missing-trust': TRUST, 'missing-app': APP, 'missing-slot': slot,
                          'missing-selected': SELECTED}[case]
                if target.is_dir() and not target.is_symlink(): shutil.rmtree(target)
                else: target.unlink()
            elif case in ['pending', 'bad-pending', 'linked-pending']:
                if case == 'linked-pending': (APP/'state.next').symlink_to('absent')
                else: put(APP/'state.next', json.dumps(dict(state, floor=2)).encode() if case=='pending' else b'invalid')
                error = 'Busy'
            elif case == 'wrong-trust':
                TRUST.chmod(0o644); TRUST.write_bytes(b'x'*32); error = 'Denied'
            elif case == 'bad-signature': put(slot/'manifest.sig', bytes(64)); error = 'Signature'
            elif case == 'bad-payload': put(slot/'rustsimhub-core', bytes(64)); error = 'Integrity'
            elif case == 'bad-manifest':
                value = (slot/'manifest.json').read_bytes().replace(b'a'*64,b'b'*64)
                put(slot/'manifest.json',value); error = 'Integrity'
            elif case == 'oversize-manifest': put(slot/'manifest.json', bytes(8193)); error = 'Invalid'
            elif case in ['escape-selected', 'relative-selected']:
                SELECTED.unlink(); SELECTED.symlink_to('/tmp' if case=='escape-selected' else '../versions')
                error = 'Denied'
            elif case == 'pending-selected': SELECTED.with_name('selected.next').symlink_to(slot); error = 'Busy'
            elif case in ['staged-not-running', 'attempted-candidate', 'attempted-wrong-slot']:
                original = state['confirmed'].copy()
                if case == 'attempted-wrong-slot': state['candidate'] = dict(original, hash='b'*64)
                else: state['candidate'] = original; state['confirmed'] = dict(original, hash='b'*64)
                state['attempted'] = case != 'staged-not-running'
                put(APP/'state.json', json.dumps(state).encode())
                error = 'Io' if case == 'attempted-candidate' else 'Denied'
            if case == 'writer-busy':
                with (APP/'.update.lock').open('rb') as lock:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    check(['inspect-runtime'], error='Busy')
            elif case == 'bad-systemd':
                failures = {'hang':'Busy', 'overflow':'Invalid', 'exit':'Integrity', 'bad-utf8':'Invalid',
                            'wrong-user':'Denied', 'foreign-cgroup':'Integrity', 'wrong-unit':'Denied',
                            'bad-pid':'Invalid', 'missing':'Denied', 'not-found':'Denied',
                            'pending-reload':'Denied', 'malformed':'Invalid', 'duplicate':'Invalid'}
                for mode, category in failures.items():
                    put(MARKER/'mode',mode.encode())
                    check(['inspect-runtime'], error=category)
            elif case == 'environment-cleared':
                env = os.environ.copy()
                for key in ['SYSTEMD_HOST','SYSTEMD_MACHINE','SYSTEMD_BUS_ADDRESS',
                            'DBUS_SYSTEM_BUS_ADDRESS','DBUS_SESSION_BUS_ADDRESS']:
                    env[key] = 'synthetic-invalid-target'
                # Good synthetic unit data reaches the unavailable actual cgroup
                # check (Io); leaked environment would make the helper exit (Integrity).
                check(['inspect-runtime'], env=env, error='Io')
            else:
                if case == 'runtime-unavailable': error = 'Io'
                for _ in range(2 if case=='repeated-calls' else 1):
                    check(['inspect-runtime'], error=error)
        results.append({'case':case,'passed':True})
    print(json.dumps({'status':'passed','groups':len(results),'read_checks':READ_CHECKS,'results':results,
                      'actual_root_cli':True,'actual_linux_boot_read':True,
                      'protected_tree_unchanged':True,'systemd_transport':'synthetic-systemctl, not PID1',
                      'not_run':['real PID1 and service cgroups','native ARM signed-slot running success',
                                 'business readiness/data generation/authorization/confirm','device deployment'],
                      'health_verified':False,'may_confirm':False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--updater',type=Path,required=True,help='native compiled updater; mounted read-only')
    parser.add_argument('--image',default='python:3.13-slim-trixie',help='already-local compatible Python3 image; never pulled')
    parser.add_argument('--output',type=Path,help='write bounded synthetic results to this caller-owned file')
    args = parser.parse_args()
    binary = args.updater.resolve(strict=True)
    environment = os.environ.copy()
    for name in DOCKER_SELECTORS: environment.pop(name,None)
    docker = ['docker','--host=unix:///var/run/docker.sock']
    try:
        inspected = subprocess.run([*docker,'image','inspect',args.image,'--format','{{.Id}}'],
                                   env=environment,stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
    except (OSError,subprocess.TimeoutExpired):
        print(json.dumps({'status':'not_run','reason':'local Docker/image prerequisite unavailable'})); return 77
    if inspected.returncode:
        print(json.dumps({'status':'not_run','reason':'local Docker/image prerequisite unavailable'})); return 77
    with tempfile.TemporaryDirectory(prefix='rsh-runtime-cli-input-') as temporary:
        stage = Path(temporary); stage.chmod(0o755)
        shutil.copyfile(binary,stage/'updater'); (stage/'updater').chmod(0o555)
        shutil.copyfile(__file__,stage/'driver.py'); (stage/'driver.py').chmod(0o444)
        container_name = 'rsh-runtime-observation-'+uuid.uuid4().hex
        command = [*docker,'run','--rm','--name',container_name,'--pull=never','--network=none','--read-only',
                   '--cap-drop=ALL','--cap-add=SETUID','--cap-add=SETGID',
                   '--pids-limit=128','--memory=256m','--ulimit=nofile=512:512',
                   '--mount','type=bind,src='+str(stage)+',dst=/fixture,readonly']
        for root in FIXTURE_MOUNTS:
            command += ['--tmpfs',root+':rw,nosuid,nodev,size=32m,mode=755'+(',exec' if root=='/usr/bin' else ',noexec')]
        image_id = inspected.stdout.decode().strip()
        used_binary_sha = sha(stage/'updater')
        used_fixture_sha = sha(stage/'driver.py')
        command += [image_id,'python3','-c',
                    "import runpy; runpy.run_path('/fixture/driver.py',run_name='runtime_fixture')['isolated_child']()"]
        try:
            completed = subprocess.run(command,env=environment,stdin=subprocess.DEVNULL,
                                       capture_output=True,timeout=60)
        except subprocess.TimeoutExpired:
            subprocess.run([*docker,'rm','--force',container_name],env=environment,
                           stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
            print('private CLI fixture timed out; own container removed',file=sys.stderr)
            return 1
        if completed.returncode:
            # The child only uses synthetic materials. Keep the useful traceback,
            # but never print the host environment or Docker configuration.
            sys.stderr.buffer.write(completed.stderr[:8192]); return 1
        report = json.loads(completed.stdout)
    report['updater_sha256'] = used_binary_sha
    report['fixture_sha256'] = used_fixture_sha
    report['image_id'] = inspected.stdout.decode().strip()
    repository = Path(__file__).resolve().parents[1]
    head = subprocess.run(['git','rev-parse','HEAD'],cwd=repository,capture_output=True,text=True)
    dirty = subprocess.run(['git','status','--porcelain'],cwd=repository,capture_output=True,text=True)
    report['tested_source_head'] = head.stdout.strip() if head.returncode==0 else None
    report['source_worktree_dirty'] = bool(dirty.stdout) if dirty.returncode==0 else None
    output = json.dumps(report,ensure_ascii=False,sort_keys=True)
    assert len(output.encode()) < 16384
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(output+'\n')
    print(output)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
