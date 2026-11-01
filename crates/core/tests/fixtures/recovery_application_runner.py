#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit network-disabled container only; synthetic A bytes, actual kernel UIDs."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

ROOT=Path('/tmp/recovery-app-uid')
def drop_to(uid):
    os.setgroups([]);os.setgid(1002 if uid!=1001 else 1001);os.setuid(uid);os.umask(0o007)
def frame(marker):
    body=b'\x01\x03' if marker==8 else b'\x01\x02'+bytes([marker])
    return len(body).to_bytes(4,'big')+body
def peer(marker):
    s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);s.settimeout(12);s.connect(str(ROOT/'core.sock'))
    data=frame(0x21)+frame(0x22) if marker==4 else frame(marker)
    received=bytearray()
    try:
        s.sendall(data)
        while True:
            data=s.recv(2048)
            if not data:break
            received.extend(data)
    except (BrokenPipeError,ConnectionResetError):pass
    finally:s.close()
    expected=(4).to_bytes(4,'big')+b'\x01\x02'+bytes([0xa5,marker]) if marker in (2,3) else b''
    assert received==expected,'exact synthetic-sealed reply or EOF required, no plaintext DTO'
def line(child,expected):
    deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        ready,_,_=select.select([child.stdout],[],[],max(0,deadline-time.monotonic()))
        if not ready:break
        data=child.stdout.readline()
        if not data:break
        print(data.decode('utf-8','replace'),end='',flush=True)
        if data.strip().endswith(expected.encode()):return
    raise RuntimeError('Core did not report '+expected)
def run(binary):
    assert os.geteuid()==0,'isolated container setup requires root'
    ROOT.mkdir(mode=0o700);os.chown(ROOT,1001,1002);os.setgroups([1002]);os.chmod(ROOT,0o2750);os.setgroups([])
    core=subprocess.Popen([binary,'--exact','isolated_real_uid_application_dispatch_and_reply_fences','--nocapture','--test-threads=1'],preexec_fn=lambda:drop_to(1001),env=dict(os.environ,RUSTSIMHUB_UID_FIXTURE_DIR=str(ROOT)),stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0)
    children=[]
    child_uids={core.pid:1001}
    def launch(uid,marker):
        p=subprocess.Popen([sys.executable,__file__,'peer',str(marker)],preexec_fn=lambda:drop_to(uid),stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);children.append(p);child_uids[p.pid]=uid;return p
    def wait(p):
        output=p.communicate(timeout=15)[0]
        if p.returncode:raise RuntimeError(output.decode('utf-8','replace'))
    try:
        line(core,'APP_READY')
        for uid in [0,1001,1003,1004,1005,1006]:wait(launch(uid,0x99))
        for marker,name in [(1,'no_sealer'),(2,'sealed_lookup'),(3,'sealed_invalid_input'),(4,'queued_cancel'),(5,'sealer_cancel'),(6,'sealer_timeout'),(8,'close_only'),(7,'revoke_before_reply')]:
            p=launch(1002,marker)
            if marker==4:line(core,'APP_QUEUED');wait(launch(1002,0x99))
            if marker in (5,6):line(core,f'APP_SEAL_HELD_{marker}');wait(launch(1002,0x99))
            line(core,'UID_CASE '+name);wait(p)
        output=core.communicate(timeout=5)[0];print(output.decode('utf-8','replace'),end='',flush=True)
        assert core.returncode==0,'actual Core test failed'
        print(json.dumps({'fixture':'isolated-core-application-v1','core_uid':1001,'ble_uid':1002,'denied_uids':[0,1001,1003,1004,1005,1006],'tests_passed':1,'application_dispatch':True,'verifier_and_sealer':'cfg(test)-synthetic','production_roles_selected':False}))
    finally:
        for p in children+[core]:
            if p.poll() is None:
                # The controller intentionally has no CAP_KILL. A helper drops
                # to this exact owned child's UID before signalling it, without
                # adding capabilities or changing accounts/NSS.
                code='import os,signal,sys\ntry: os.kill(int(sys.argv[1]),signal.SIGKILL)\nexcept ProcessLookupError: pass\n'
                subprocess.run([sys.executable,'-c',code,str(p.pid)],preexec_fn=lambda:drop_to(child_uids[p.pid]),check=True,timeout=2,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            p.wait(timeout=3)
if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='peer':peer(int(sys.argv[2]))
    elif len(sys.argv)==2:run(sys.argv[1])
    else:raise SystemExit('requires compiled explicit test binary')
