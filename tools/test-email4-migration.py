#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Owned container fixture: real default3 writer -> real low-UID candidate4.
Run INSIDE a disposable --network=none container as Root. No host paths/DBs.
This exercises the authenticated child, not a substitute systemd producer.
"""
import ctypes
import argparse, hashlib, json, os, pathlib, shutil, socket, sqlite3, struct, subprocess, time

DATA=pathlib.Path('/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding')
def snapshot():
    with sqlite3.connect(DATA/'outbox.sqlite3') as db:
        return {'meta':db.execute('SELECT * FROM meta').fetchall(),'requests':[(a,b,c.hex(),d) for a,b,c,d in db.execute('SELECT * FROM requests')], 'jobs':db.execute('SELECT * FROM jobs ORDER BY seq').fetchall(),'version':db.execute('PRAGMA user_version').fetchone()[0]}
def main():
    p=argparse.ArgumentParser();p.add_argument('--legacy',required=True);p.add_argument('--candidate',required=True);a=p.parse_args()
    assert os.getuid()==0 and pathlib.Path('/.dockerenv').exists() and os.getpid()>1
    assert ctypes.CDLL(None,use_errno=True).prctl(38,1,0,0,0)==0
    # Copy into this disposable filesystem; input workspace mounts stay readonly.
    pathlib.Path('/owned').mkdir();
    for name,src in [('legacy',a.legacy),('candidate',a.candidate)]:shutil.copyfile(src,'/owned/'+name);os.chmod('/owned/'+name,0o755)
    results=[]
    for mode in ['commit','deny_commit','root_eof','wrong_source','busy_writer','wrong_peer']:
        if DATA.exists():shutil.rmtree(DATA)
        DATA.mkdir(parents=True,mode=0o700);os.chown(DATA,986,986)
        subprocess.run(['/owned/legacy',str(DATA)],user=986,group=986,extra_groups=[],check=True,timeout=10)
        original=(DATA/'outbox.sqlite3').read_bytes();before=snapshot();assert before['version']==3
        digest=list(hashlib.sha256(original).digest())
        permit={'format':1,'source_sha256':digest,'backup_sha256':digest,'deadline_ms':int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)+30000,'data_uid':986}
        if mode=='wrong_source':permit['source_sha256']=[9]*32;permit['backup_sha256']=[9]*32
        parent,child=socket.socketpair();parent.settimeout(6)
        lock=None
        if mode=='busy_writer':
            import fcntl
            lock=open(DATA/'.writer.lock','rb');fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if mode=='wrong_peer':
            # A non-root parent creates the socketpair itself. Its SO_PEERCRED
            # differs from the privileged driver's; the same public JSON fails.
            script="import socket,subprocess; p,c=socket.socketpair(); s=subprocess.run(['/owned/candidate','--migrate-email4-child'],stdin=c,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5); assert s.returncode!=0"
            subprocess.run(['/usr/local/bin/python3','-c',script],user=986,group=986,extra_groups=[],check=True,timeout=8)
            status=1;parent.close();child.close()
        else:
            proc=subprocess.Popen(['/owned/candidate','--migrate-email4-child'],stdin=child,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,user=986,group=986,extra_groups=[]);child.close()
            try:
                raw=json.dumps(permit,separators=(',',':')).encode();parent.sendall(struct.pack('>I',len(raw))+raw)
                ready=parent.recv(1)
                if mode in ['commit','deny_commit','root_eof']:assert ready==b'R'
                if mode=='root_eof':parent.close()
                elif mode=='deny_commit':parent.sendall(b'N')
                elif mode=='commit':parent.sendall(b'G');assert parent.recv(1)==b'D'
                else:assert ready==b''
                status=proc.wait(timeout=5);assert len(proc.stderr.read(4096))<256
            finally:
                parent.close()
                if proc.poll() is None:proc.kill();proc.wait(timeout=3)
        if lock:lock.close()
        after=snapshot()
        if mode=='commit':
            assert status==0 and after['version']==4 and after['meta']==before['meta'] and after['requests']==before['requests']
            for old,new in zip(before['jobs'],after['jobs']):
                old=list(old);new=list(new)
                if old[9] is not None:assert new[9]=='{"transport":"https","request":'+old[9]+'}'
                assert new[7]==('unknown' if old[7]=='in_flight' else old[7]);new[7]=old[7];new[9]=old[9];assert new==old
            assert (DATA/'outbox.sqlite3').stat().st_uid==986
            migrated=(DATA/'outbox.sqlite3').read_bytes()
            refused=subprocess.run(['/owned/legacy',str(DATA)],user=986,group=986,extra_groups=[],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=5)
            assert refused.returncode!=0 and (DATA/'outbox.sqlite3').read_bytes()==migrated
        else:assert status!=0 and after==before and (DATA/'outbox.sqlite3').read_bytes()==original
        assert sorted(x.name for x in DATA.iterdir())==['.writer.lock','outbox.sqlite3']
        results.append({'case':mode,'pass':True,'data_uid':986,'child_reaped':True,'source_writer':3,'result_version':after['version']})
    print(json.dumps({'format':1,'network':'none','systemd_producer_tested':False,'cases':results},sort_keys=True))
if __name__=='__main__':main()
