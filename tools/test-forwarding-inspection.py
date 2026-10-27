#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual production CLI against disposable SQLite; no network or device.

Rust tests additionally create the outbox through the actual Queue. These
process tests do not establish Owner validity or signed updater admission.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile

APP = 0x52534657


def snapshot(directory):
    result = {}
    for path in directory.iterdir():
        stat = path.lstat()
        if path.is_symlink():
            value = ('symlink', os.readlink(path))
        elif path.is_fifo():
            value = ('fifo',)
        else:
            value = ('file', hashlib.sha256(path.read_bytes()).hexdigest())
        result[path.name] = (stat.st_mode, stat.st_uid, stat.st_gid, stat.st_size,
                             stat.st_ino, stat.st_nlink, stat.st_mtime_ns, value)
    return result


def fixture(directory, version=3, app=APP):
    (directory/'.writer.lock').touch(mode=0o600)
    path = directory/'outbox.sqlite3'
    with sqlite3.connect(path) as db:
        db.executescript('PRAGMA journal_mode=DELETE;'
                         'CREATE TABLE probe(id INTEGER PRIMARY KEY, state TEXT, payload TEXT);'
                         "INSERT INTO probe VALUES(1,'in_flight','synthetic secret, never output');"
                         "INSERT INTO probe VALUES(2,'unknown','synthetic secret, never output');")
        db.execute('PRAGMA application_id='+str(app))
        db.execute('PRAGMA user_version='+str(version))
    path.chmod(0o600)


def invoke(binary, directory, expected=None):
    before = snapshot(directory)
    result = subprocess.run([str(binary), '--inspect-outbox', str(directory)],
                            stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
    assert snapshot(directory) == before, 'read-only inspection changed files or metadata'
    assert len(result.stdout) <= 4096 and len(result.stderr) <= 4096
    assert b'synthetic secret' not in result.stdout+result.stderr
    if expected is None:
        assert result.returncode != 0 and not result.stdout, 'unsafe probe accepted'
        assert result.stderr == b'rustsimhub-forwarder: operation refused or unconfirmed\n'
    else:
        assert result.returncode == 0 and not result.stderr
        assert json.loads(result.stdout) == {
            'format': 1, 'read_only': True, 'authorization_verified': False,
            'outbox': {'application_id': expected[0], 'format': expected[1]},
            'compatibility': expected[2]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--worker', type=Path, required=True)
    binary = parser.parse_args().worker.resolve(strict=True)
    info = subprocess.run([str(binary), '--build-info'], check=True,
                          capture_output=True, timeout=3)
    assert not info.stderr and len(info.stdout) <= 16384
    build = json.loads(info.stdout)
    assert build['worker'] is True and build['development_fixtures'] is False
    cases = []
    with tempfile.TemporaryDirectory(prefix='rsh-readonly-cli-') as temporary:
        root = Path(temporary)
        versions = [(APP, 1, 'migration_required'), (APP, 2, 'migration_required'),
                    (APP, 3, 'read_write'), (APP, 4, 'unsupported_format'),
                    (APP, 0, 'unsupported_format'), (0, 3, 'foreign_application')]
        for index, expected in enumerate(versions):
            directory = root/str(index)
            directory.mkdir(mode=0o700)
            fixture(directory, expected[1], expected[0])
            invoke(binary, directory, expected)
            cases.append('version-'+str(index))
        for name in ['missing-lock', 'missing-database', 'busy', 'journal', 'wal', 'shm',
                     'dangling-journal', 'symlink', 'hardlink', 'fifo', 'corrupt',
                     'permissions', 'hot-journal']:
            directory = root/name
            directory.mkdir(mode=0o700)
            fixture(directory)
            path = directory/'outbox.sqlite3'
            lock = None
            if name == 'missing-lock':
                (directory/'.writer.lock').unlink()
            elif name == 'missing-database':
                path.unlink()
            elif name == 'busy':
                lock = (directory/'.writer.lock').open('rb')
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            elif name in ['journal', 'wal', 'shm']:
                (directory/('outbox.sqlite3-'+name)).touch(mode=0o600)
            elif name == 'dangling-journal':
                (directory/'outbox.sqlite3-journal').symlink_to(directory/'absent')
            elif name in ['symlink', 'hardlink', 'fifo']:
                saved = directory/'saved'
                path.rename(saved)
                if name == 'symlink':
                    path.symlink_to(saved)
                elif name == 'hardlink':
                    os.link(saved, path)
                else:
                    os.mkfifo(path, 0o600)
            elif name == 'corrupt':
                path.write_bytes(b'X'*4096)
            elif name == 'permissions':
                path.chmod(0o640)
            elif name == 'hot-journal':
                code = ('import sqlite3,os,sys; d=sqlite3.connect(sys.argv[1]);'
                        'd.executescript("PRAGMA cache_size=1; BEGIN IMMEDIATE;");'
                        'd.executemany("INSERT INTO probe(state,payload) VALUES(?,?)",'
                        '[("in_flight", "x"*8192)]*32); os._exit(0)')
                subprocess.run([sys.executable, '-c', code, str(path)], check=True, timeout=5)
                assert (directory/'outbox.sqlite3-journal').stat().st_size > 512
            try:
                invoke(binary, directory)
                cases.append(name)
            finally:
                if lock:
                    lock.close()
    print(json.dumps({'passed': True, 'cases': cases, 'case_count': len(cases),
                      'production_cli': True, 'network_used': False,
                      'owner_authorization_verified': False, 'device_access': False}))


if __name__ == '__main__':
    main()
