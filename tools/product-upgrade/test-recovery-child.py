#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run only inside an owned network=none disposable Root container.
Original Store fixture -> SAME production child module at fixed path/UID986.
No production keys, host accounts/services, or real message samples.
"""
import argparse
import ctypes
import fcntl
import hashlib
import json
import os
import pathlib
import shutil
import socket
import sqlite3
import struct
import subprocess
import time

DATA = pathlib.Path('/var/lib/rustsimhub/rustsimhub-v1/core/state')

def connection():
    return sqlite3.connect('file:' + str(DATA / 'store.sqlite3') + '?immutable=1', uri=True)

def observation():
    with connection() as db:
        schema = db.execute('PRAGMA user_version').fetchone()[0]
        pin = db.execute('SELECT public_key FROM device_identity').fetchone()[0]
        realm = db.execute('SELECT db_epoch FROM meta').fetchone()[0]
    return schema, hashlib.sha256((DATA / 'store.sqlite3').read_bytes()).hexdigest(), hashlib.sha256(pin + realm.encode()).hexdigest()

def protected(tables=None):
    with connection() as db:
        if tables is None:
            tables = [r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]
        rows = {name: db.execute('SELECT * FROM "' + name.replace('"', '""') + '"').fetchall() for name in tables}
    encoded = json.dumps(rows, sort_keys=True, default=lambda x: x.hex()).encode()
    return tables, hashlib.sha256(encoded).hexdigest()

def run_fixture(option):
    subprocess.run(['/input/store6-fixture', option], user=986, group=986, extra_groups=[], check=True, timeout=8, stdout=subprocess.DEVNULL)

def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    assert os.getuid() == 0 and pathlib.Path('/.dockerenv').exists() and os.getpid() > 1
    assert ctypes.CDLL(None, use_errno=True).prctl(38, 1, 0, 0, 0) == 0
    results = []
    cases = ['commit6', 'sealed6', 'commit7_revoked', 'deny_commit', 'root_eof', 'wrong_source', 'wrong_binding', 'changed_key', 'changed_realm', 'busy_writer', 'wrong_peer', 'expired', 'late_commit', 'unknown_field', 'duplicate_key', 'symlink']
    for case in cases:
        if DATA.exists(): shutil.rmtree(DATA)
        backup = pathlib.Path('/fixture/schema7-backup')
        if backup.exists(): shutil.rmtree(backup)
        DATA.mkdir(parents=True, mode=0o700)
        os.chown(DATA, 986, 986)
        run_fixture('--make-revoked-schema7' if case == 'commit7_revoked' else '--make-schema6')
        schema, source, binding = observation()
        permit = dict(format=1, source_schema=schema, target_schema=9, source_sha256=source, binding_sha256=binding, request_id='b'*64, deadline_ms=int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)+30000)
        if case == 'wrong_source': permit['source_sha256'] = 'c'*64
        if case == 'wrong_binding': permit['binding_sha256'] = 'c'*64
        if case == 'expired': permit['deadline_ms'] -= 31000
        if case == 'late_commit': permit['deadline_ms'] -= 28500
        if case == 'unknown_field': permit['authorization_verified'] = True
        if case in ['changed_key', 'changed_realm']:
            # This is synthetic test setup only. Permit was bound before the change.
            with sqlite3.connect(DATA / 'store.sqlite3') as db:
                if case == 'changed_key': db.execute('UPDATE device_identity SET public_key=?', (b'\x08'*32,))
                else: db.execute("UPDATE meta SET db_epoch='00000000-0000-4000-8000-000000000099'")
        tables, before = protected()
        original = (DATA / 'store.sqlite3').read_bytes()
        lock = None
        if case == 'busy_writer':
            lock = open(DATA / '.writer.lock', 'rb')
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if case == 'symlink':
            (DATA / 'store.sqlite3').rename(DATA / 'saved.sqlite3')
            (DATA / 'store.sqlite3').symlink_to('saved.sqlite3')
        parent, child = socket.socketpair()
        parent.settimeout(6)
        if case == 'wrong_peer':
            script = "import socket,subprocess;p,c=socket.socketpair();r=subprocess.run(['/input/recovery-child','--migrate-product-recovery-child'],stdin=c,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5);assert r.returncode!=0"
            subprocess.run(['/usr/local/bin/python3', '-c', script], user=986, group=986, extra_groups=[], check=True, timeout=8)
            status = 4
            parent.close(); child.close()
        else:
            executable = '/input/recovery-child'
            output = subprocess.DEVNULL
            sealed = None
            if case == 'sealed6':
                sealed = os.memfd_create('recovery-fixture', os.MFD_ALLOW_SEALING | os.MFD_CLOEXEC)
                os.write(sealed, pathlib.Path(executable).read_bytes())
                os.fchmod(sealed, 0o555)
                fcntl.fcntl(sealed, fcntl.F_ADD_SEALS, fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_WRITE | fcntl.F_SEAL_SEAL)
                executable = '/proc/self/fd/1'; output = sealed
            proc = subprocess.Popen([executable, '--migrate-product-recovery-child'], stdin=child, stdout=output, stderr=subprocess.PIPE, user=986, group=986, extra_groups=[])
            child.close()
            if sealed is not None: os.close(sealed)
            try:
                raw = json.dumps(permit, separators=(',', ':')).encode()
                if case == 'duplicate_key': raw = b'{"format":1,' + raw[1:]
                parent.sendall(struct.pack('>I', len(raw)) + raw)
                ready = parent.recv(1)
                if case in ['commit6', 'sealed6', 'commit7_revoked', 'deny_commit', 'root_eof', 'late_commit']:
                    assert ready == b'R', case
                if case in ['commit6', 'sealed6', 'commit7_revoked']:
                    parent.sendall(b'G'); assert parent.recv(1) == b'D', case
                elif case == 'deny_commit': parent.sendall(b'N')
                elif case == 'root_eof': parent.close()
                elif case == 'late_commit':
                    time.sleep(1.6); parent.sendall(b'G'); assert parent.recv(1) == b''
                else: assert ready == b'', case
                status = proc.wait(timeout=5)
                assert len(proc.stderr.read(4096)) < 256
            finally:
                parent.close()
                if proc.poll() is None: proc.kill(); proc.wait(timeout=3)
        if lock: lock.close()
        assert protected(tables)[1] == before, case
        if case in ['commit6', 'sealed6', 'commit7_revoked']:
            assert status == 0 and observation()[0] == 9, case
            report = subprocess.check_output(['/input/recovery-child', '--inspect-product-recovery-upgrade'], user=986, group=986, extra_groups=[], timeout=3)
            assert json.loads(report)['state'] == 'committed'
            migrated = (DATA / 'store.sqlite3').read_bytes()
            run_fixture('--refuse-new-schema')
            assert (DATA / 'store.sqlite3').read_bytes() == migrated
            assert (DATA / 'product-upgrade/before-execution/backup.sqlite3').is_file()
            if schema == 6: assert (DATA / 'product-upgrade/before-grants/backup.sqlite3').is_file()
            # Reusing the same permit does not retry a committed operation.
            again = subprocess.run(['/input/recovery-child', '--migrate-product-recovery-child'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, user=986, group=986, extra_groups=[], timeout=3)
            assert again.returncode != 0 and (DATA / 'store.sqlite3').read_bytes() == migrated
        else:
            assert status != 0 and (DATA / 'store.sqlite3').read_bytes() == original, case
            assert not (DATA / 'product-upgrade').exists(), case
        results.append(dict(case=case, passed=True, source_schema=schema, result_schema=observation()[0], data_uid=986, child_reaped=True, protected_rows_unchanged=True))
    # Explicit ignored test uses the ORIGINAL fixed-path Root snapshot. Only
    # its signature metadata transport is synthetic; no product authority is claimed.
    shutil.rmtree(DATA)
    DATA.mkdir(mode=0o700)
    os.chown(DATA, 986, 986)
    run_fixture('--make-schema6')
    base = pathlib.Path('/etc/systemd/system/rustsimhub-core.service')
    base.parent.mkdir(parents=True, exist_ok=True)
    base.write_bytes(pathlib.Path('/input/core-base.service').read_bytes())
    base.chmod(0o644)
    subprocess.run(['/input/product-admission-tests', '--ignored', '--exact', 'product_admission::tests::actual_original_guard_borrows_real_writer_and_rejects_root_generation_change'], check=True, timeout=10)
    print(json.dumps(dict(format=1, network='none', formal_core_cli_tested=False, production_handoff_tested=False, actual_original_guard_test=True, guard_metadata_transport='synthetic', cases=results), sort_keys=True))

if __name__ == '__main__': main()
