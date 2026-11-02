#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Network-disabled real-UID Core and native cryptographic peer fixture."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

ROOT = Path('/tmp/recovery-secure-uid')


def drop_to(uid):
    os.setgroups([])
    os.setgid(1002 if uid != 1001 else 1001)
    os.setuid(uid)
    os.umask(0o007)


def line(child, expected):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if not select.select([child.stdout], [], [], max(0, deadline-time.monotonic()))[0]:
            break
        data = child.stdout.readline()
        if not data:
            break
        print(data.decode('utf-8', 'replace'), end='', flush=True)
        if data.strip().endswith(expected.encode()):
            return
    raise RuntimeError('Core did not report ' + expected)


def run(binary):
    assert os.geteuid() == 0, 'explicit isolated container setup only'
    ROOT.mkdir(mode=0o700)
    os.chown(ROOT, 1001, 1002)
    os.setgroups([1002])
    os.chmod(ROOT, 0o2750)
    os.setgroups([])
    peer_dir = Path('/tmp/recovery-secure-peer')
    peer_dir.mkdir(mode=0o700)
    os.chown(peer_dir, 1002, 1002)
    environment = dict(os.environ, RUSTSIMHUB_UID_FIXTURE_DIR=str(ROOT))
    core = subprocess.Popen([binary, '--exact', 'isolated_secure_core', '--nocapture', '--test-threads=1'],
                            preexec_fn=lambda: drop_to(1001), env=environment,
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0)
    children = [(core, 1001)]

    def wait(child):
        output = child.communicate(timeout=15)[0]
        print(output.decode('utf-8', 'replace'), end='', flush=True)
        if child.returncode:
            raise RuntimeError('actual native cryptographic peer failed')

    try:
        line(core, 'SECURE_READY')
        for uid in [0, 1001, 1003, 1004, 1005, 1006]:
            code = 'import socket,sys; s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);s.connect(sys.argv[1]);s.close()'
            p = subprocess.Popen([sys.executable, '-c', code, str(ROOT/'core.sock')],
                                 preexec_fn=lambda: drop_to(uid), stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            children.append((p, uid))
            wait(p)
            line(core, f'UID_CASE denied_{uid}')
        for case in ['query', 'wrong_pin', 'tamper', 'old_cipher', 'disconnect', 'revoked']:
            p = subprocess.Popen([binary, '--exact', 'isolated_secure_client', '--nocapture', '--test-threads=1'],
                                 env=dict(environment, RUSTSIMHUB_SECURE_CASE=case), preexec_fn=lambda: drop_to(1002),
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            children.append((p, 1002))
            line(core, 'UID_CASE ' + case)
            wait(p)
        wait(core)
        print(json.dumps({'fixture': 'isolated-secure-core-v1', 'core_uid': 1001, 'ble_uid': 1002,
                          'denied_uids': [0,1001,1003,1004,1005,1006], 'tests_passed': 7,
                          'application_dispatch': 'original-Core-QueryOperation',
                          'cryptography': 'actual-Noise-Identity-Ed25519', 'production_roles_selected': False}))
    except Exception:
        for p, _ in children:
            if p is not core and p.poll() is not None:
                print(p.communicate()[0].decode('utf-8', 'replace'), end='', flush=True)
        raise
    finally:
        for p, uid in children:
            if p.poll() is None:
                code = 'import os,signal,sys\ntry: os.kill(int(sys.argv[1]),signal.SIGKILL)\nexcept ProcessLookupError: pass\n'
                subprocess.run([sys.executable, '-c', code, str(p.pid)], preexec_fn=lambda: drop_to(uid),
                               check=True, timeout=2, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            p.wait(timeout=3)


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('requires explicit compiled test binary')
    run(sys.argv[1])
