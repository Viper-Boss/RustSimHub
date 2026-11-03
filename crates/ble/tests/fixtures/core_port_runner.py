# SPDX-License-Identifier: AGPL-3.0-only
"""Root controller only inside ephemeral isolated container, no host NSS."""
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time

CASES = ('roundtrip', 'cancel', 'fence', 'oversize', 'wrong_tag', 'wrong_version', 'timeout',
         'wrong_kernel', 'bad_mode', 'symlink', 'extra_group', 'wrong_local')

def role(uid, groups=()):
    def drop():
        os.setgroups(list(groups))
        os.setgid(uid)
        os.setuid(uid)
    return drop

def exact(s, n):
    data = b''
    while len(data) < n:
        part = s.recv(n-len(data))
        if not part:
            raise EOFError('early close')
        data += part
    return data

def server(case, base, uid):
    directory = Path(base)/case
    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(9)
    sock.bind(str(directory/'recovery.sock'))
    os.chmod(directory/'recovery.sock', 0o660)
    sock.listen(2)
    (Path(base)/(case+'.ready')).write_text('ready')
    if case in ('wrong_kernel', 'bad_mode', 'symlink', 'extra_group', 'wrong_local'):
        if case == 'wrong_kernel':
            conn, _ = sock.accept()
            conn.settimeout(2)
            assert conn.recv(1) == b'', 'no input before role validation'
            conn.close()
        else:
            # Controller verifies no connection was queued, then writes done.
            while not (Path(base)/(case+'.done')).exists():
                time.sleep(.01)
            sock.settimeout(.05)
            try:
                sock.accept()
            except socket.timeout:
                pass
            else:
                raise AssertionError('metadata/local role rejection connected')
        return
    probe, _ = sock.accept()
    probe.settimeout(2)
    assert probe.recv(1) == b'', 'probe must send no bytes'
    probe.close()
    conn, _ = sock.accept()
    conn.settimeout(7)
    count = 2 if case == 'roundtrip' else 1
    for index in range(count):
        n, = struct.unpack('!I', exact(conn, 4))
        frame = exact(conn, n)
        assert frame == bytes((1, index+1))+b'opaque-input'
        if case in ('cancel', 'fence', 'timeout'):
            assert conn.recv(1) == b'', 'cancel/deadline/fence must shut down actual FD'
        elif case == 'oversize':
            conn.sendall(struct.pack('!I',2051))
        else:
            prefix = bytes((2 if case == 'wrong_version' else 1,
                            2 if case == 'wrong_tag' else index+1))
            payload = prefix+b'opaque-reply'
            conn.sendall(struct.pack('!I',len(payload))+payload)
    conn.close()
    sock.settimeout(.15)
    try:
        sock.accept()
    except socket.timeout:
        pass
    else:
        raise AssertionError('dead session silently reconnected')

if len(sys.argv)==4:
    server(sys.argv[1], sys.argv[2], int(sys.argv[3]))
    sys.exit(0)
assert os.getuid() == 0, 'container controller only'
os.setgroups([1002])  # Traverse the private fixture; each child clears groups.
base = Path('/tmp/core-port-fixture')
base.mkdir(mode=0o1777)
base.chmod(0o1777)
for case in CASES:
    directory = base/case
    directory.mkdir()
    uid = 1003 if case == 'wrong_kernel' else 1001
    os.chown(directory, uid, 1002)
    directory.chmod(0o2750)
    child = subprocess.Popen([sys.executable, __file__, case, str(base), str(uid)], preexec_fn=role(uid))
    try:
        limit = time.monotonic()+3
        while not (base/(case+'.ready')).exists():
            assert child.poll() is None and time.monotonic()<limit, 'server startup'
            time.sleep(.01)
        path = directory/'recovery.sock'
        if case == 'wrong_kernel':
            os.chown(directory,1001,1002)
            directory.chmod(0o2750)
            os.chown(path,1001,1002)
        if case == 'bad_mode':
            path.chmod(0o666)
        if case == 'symlink':
            subprocess.run([sys.executable,'-c','import os,sys; os.symlink("recovery.sock",sys.argv[1])',str(directory/'alias.sock')],preexec_fn=role(1001),check=True)
            path = directory/'alias.sock'
        env = dict(os.environ, PORT_CASE=case, PORT_SOCKET=str(path))
        run_uid = 1004 if case == 'wrong_local' else 1002
        result = subprocess.run(['/input/core_port_uid','--exact','installed_roles_and_opaque_io','--nocapture'],
                                env=env, preexec_fn=role(run_uid,(1005,) if case=='extra_group' else ()), timeout=10)
        assert result.returncode==0, case
        (base/(case+'.done')).write_text('done')
        assert child.wait(timeout=10)==0, case+' server evidence'
    finally:
        (base/(case+'.done')).touch(exist_ok=True)
        if child.poll() is None:
            # Only signal a registered child after changing to its own UID.
            subprocess.run([sys.executable,'-c','import os,signal,sys; os.kill(int(sys.argv[1]),signal.SIGTERM)',str(child.pid)],preexec_fn=role(uid),check=True)
            child.wait(timeout=2)
print('CORE_PORT_COMPLETE 12 actual-UID scenarios')
