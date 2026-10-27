#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual fixed-path Root CLI in disposable chroots, real UID 986 data fences.

Run as Root in a fresh network namespace. No host service, production data,
device files, release keys or installed trust are exposed to the chroot.
"""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import socket
import sqlite3
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tools/system'))
sys.path.insert(0, str(ROOT/'tools'))
import forwarding_install as forwarding
import setup_ap_package as ap

STATE = Path('var/lib/rustsimhub/rustsimhub-v1/core/state')
CORE_UNIT = 'etc/systemd/system/rustsimhub-core.service'


def put(root, relative, data, mode=0o644):
    path = root/str(relative).lstrip('/')
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)
    return path


def executable(source, root):
    target = put(root, 'updater', source.read_bytes(), 0o755)
    dependencies = subprocess.run(['ldd', str(source)], capture_output=True,
                                  text=True, check=True, timeout=5).stdout
    for line in dependencies.splitlines():
        match = re.search(r'(?:=>\s+)?(/[^\s]+)\s+\(', line)
        if match:
            path = Path(match.group(1))
            put(root, str(path), path.resolve(strict=True).read_bytes(), 0o755)
    return target


def data_ownership(root):
    state = root/STATE
    for path in [state, *state.rglob('*')]:
        if not path.is_symlink():
            os.chown(path, 986, 986)
            path.chmod(0o700 if path.is_dir() else 0o600)


def create_root(root, binary, mode='setup_ap', outbox=True):
    root.chmod(0o755)
    executable(binary, root)
    put(root, 'dev/null', b'', 0o666)  # Inert regular stand-in; no host devices.
    put(root, CORE_UNIT, (ROOT/'firmware/system/overlay'/CORE_UNIT).read_bytes().replace(b'\r\n', b'\n'))
    if mode != 'device':
        for name, target, permissions in forwarding.FILES:
            value = forwarding.MANAGED_DROPIN if name is None else (forwarding.MATERIAL/name).read_bytes().replace(b'\r\n', b'\n')
            put(root, target, value, permissions)
    if mode == 'setup_ap':
        for name, target, permissions in ap.FILES:
            value = (b'inert synthetic executable; no provenance assertion' if name in ap.BINARIES
                     else (ap.MATERIAL/name).read_bytes().replace(b'\r\n', b'\n'))
            put(root, target, value, permissions)
    directory = root/STATE
    directory.mkdir(parents=True, exist_ok=True)
    (directory/'.writer.lock').touch(mode=0o600)
    with contextlib.closing(sqlite3.connect(directory/'store.sqlite3')) as db:
        db.executescript('PRAGMA journal_mode=WAL; PRAGMA application_id=1381189698; PRAGMA user_version=6;'
                         'CREATE TABLE synthetic_secret(body TEXT);'
                         "INSERT INTO synthetic_secret VALUES('synthetic sensitive payload must not appear');")
    assert (directory/'store.sqlite3').read_bytes()[18:20] == b'\x02\x02'
    if outbox:
        forward = directory/'forwarding'
        forward.mkdir()
        (forward/'.writer.lock').touch(mode=0o600)
        with contextlib.closing(sqlite3.connect(forward/'outbox.sqlite3')) as db:
            db.executescript('PRAGMA journal_mode=DELETE; PRAGMA application_id=1381189207; PRAGMA user_version=3;'
                             'CREATE TABLE synthetic_jobs(state TEXT, body TEXT);'
                             "INSERT INTO synthetic_jobs VALUES('in_flight','synthetic sensitive payload must not appear');")
    data_ownership(root)


def tree(root):
    result = {}
    for path in root.rglob('*'):
        meta = path.lstat()
        if stat.S_ISLNK(meta.st_mode):
            value = ('link', os.readlink(path))
        elif stat.S_ISREG(meta.st_mode):
            value = ('file', hashlib.sha256(path.read_bytes()).hexdigest())
        else:
            value = ('kind', stat.S_IFMT(meta.st_mode))
        result[str(path.relative_to(root))] = (meta.st_mode, meta.st_uid, meta.st_gid,
            meta.st_ino, meta.st_nlink, meta.st_size, meta.st_mtime_ns, value)
    return result


def invoke(root, success, mode='setup_ap', outbox=True, uid=0):
    before = tree(root)
    command = ['chroot', '--userspec='+str(uid)+':'+str(uid), '--groups='+str(uid),
               str(root), '/updater', 'inspect-maintenance']
    result = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True,
                            timeout=8, env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
    assert tree(root) == before, 'observation changed data/config/metadata or created sidecars'
    assert (result.returncode == 0) == success, result.stderr.decode()
    assert len(result.stdout) < 16384 and len(result.stderr) < 4096
    assert b'synthetic sensitive' not in result.stdout+result.stderr
    if success:
        report = json.loads(result.stdout)
        assert not result.stderr and report['format'] == 1
        for field in ['authorization_verified', 'effective_systemd_verified', 'signed_programs_verified']:
            assert report[field] is False
        assert report['root_mode'] == mode and report['business_schema'] == 6
        assert len(report['root_material']) == {'device': 1, 'forwarding': 10, 'setup_ap': 18}[mode]
        assert (report['outbox'] is not None) == outbox
        if outbox:
            assert report['outbox'] == {'application_id': 0x52534657, 'format': 3}
    else:
        assert not result.stdout and result.stderr.startswith(b'application update denied: ')


@contextlib.contextmanager
def writer(path):
    code = ("import sys,fcntl,os;assert os.geteuid()==os.getegid()==986 and os.getgroups()==[];"
            "f=open(sys.argv[1],'rb');fcntl.flock(f,fcntl.LOCK_EX);print('ready',flush=True);sys.stdin.read()")
    process = subprocess.Popen([sys.executable, '-u', '-c', code, str(path)], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               user=986, group=986, extra_groups=[])
    try:
        assert select.select([process.stdout], [], [], 5)[0], 'writer did not become ready'
        assert process.stdout.readline() == b'ready\n'
        yield
    finally:
        process.communicate(timeout=3)
        assert process.returncode == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--updater', type=Path, required=True)
    binary = parser.parse_args().updater.resolve(strict=True)
    assert os.geteuid() == 0 and socket.if_nameindex() == [(1, 'lo')], 'fresh unshare --net required'
    info = json.loads(subprocess.check_output([str(binary), '--build-info'], timeout=5))
    assert info['maintenance_observation_protocol'] == 1
    results = []
    with tempfile.TemporaryDirectory(prefix='rsh-maintenance-') as temporary:
        base = Path(temporary)
        base.chmod(0o755)
        cases = ['device', 'forwarding', 'setup_ap', 'absent', 'empty', 'main-busy', 'outbox-busy',
                 'wal', 'shm', 'hot-journal', 'partial-root', 'override', 'corrupt',
                 'root-owner', 'nonroot', 'symlink', 'missing-lock', 'fifo']
        for case in cases:
            with tempfile.TemporaryDirectory(prefix=case+'-', dir=base) as directory:
                root = Path(directory)
                mode = case if case in ['device', 'forwarding', 'setup_ap'] else 'setup_ap'
                create_root(root, binary, mode, outbox=case not in ['absent', 'empty'])
                state = root/STATE
                if case in ['device', 'forwarding', 'setup_ap', 'absent']:
                    invoke(root, True, mode, outbox=case != 'absent')
                elif case == 'empty':
                    (state/'forwarding').mkdir(mode=0o700)
                    data_ownership(root)
                    invoke(root, True, outbox=False)
                elif case in ['main-busy', 'outbox-busy']:
                    path = state/('.writer.lock' if case == 'main-busy' else 'forwarding/.writer.lock')
                    with writer(path):
                        invoke(root, False)
                else:
                    path = state/'store.sqlite3'
                    if case == 'wal':
                        (state/'store.sqlite3-wal').touch(mode=0o600)
                    elif case == 'shm':
                        (state/'forwarding/outbox.sqlite3-shm').touch(mode=0o600)
                    elif case == 'hot-journal':
                        target = state/'forwarding/outbox.sqlite3'
                        code = ('import sqlite3,sys,os;d=sqlite3.connect(sys.argv[1]);'
                                'd.executescript("PRAGMA cache_size=1;BEGIN IMMEDIATE;");'
                                'd.executemany("INSERT INTO synthetic_jobs VALUES(?,?)",[("in_flight","x"*8192)]*32);os._exit(0)')
                        subprocess.run([sys.executable, '-c', code, str(target)], check=True, timeout=5)
                        assert (state/'forwarding/outbox.sqlite3-journal').stat().st_size > 512
                    elif case == 'partial-root':
                        (root/'etc/rustsimhub/setup-ap-contract.json').unlink()
                    elif case == 'override':
                        put(root, 'etc/systemd/system/rustsimhub-core.service.d/99-override.conf', b'unknown override')
                    elif case == 'corrupt':
                        path.write_bytes(b'X'*4096)
                    elif case == 'root-owner':
                        os.chown(root/CORE_UNIT, 986, 986)
                    elif case == 'nonroot':
                        invoke(root, False, uid=65534)
                        results.append(case)
                        continue
                    elif case == 'symlink':
                        saved = path.with_suffix('.saved')
                        path.rename(saved)
                        path.symlink_to(saved.name)
                    elif case == 'missing-lock':
                        (state/'forwarding/.writer.lock').unlink()
                    elif case == 'fifo':
                        path.unlink()
                        os.mkfifo(path, 0o600)
                    invoke(root, False)
                results.append(case)
    print(json.dumps({'passed': True, 'case_count': len(results), 'cases': results,
                      'build_source_sha256': info.get('source_archive_sha256'),
                      'actual_root_cli': True, 'actual_data_uid': 986,
                      'clean_wal_without_sidecar_creation': True, 'device_access': False,
                      'effective_systemd_verified': False, 'signed_programs_verified': False,
                      'updater_mutation_admission_verified': False}))


if __name__ == '__main__':
    main()
