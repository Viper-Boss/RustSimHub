#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run only inside the named disposable network-none container, never on host."""
import json,os,pathlib,subprocess,time
ROOT=pathlib.Path('/fixture')
assert os.getuid()==0 and os.environ.get('RUSTSIMHUB_PRIVATE_UID_FIXTURE')=='1'
assert ROOT.is_dir() and pathlib.Path('/input/worker-test').is_file()
roles={'core':986,'media':984,'edge':985}
for role,uid in roles.items():
    directory=ROOT/role;directory.mkdir(mode=0o750);os.chown(directory,uid,983);directory.chmod(0o2750)
children=[]
def launch(role,uid=None):
    uid=roles[role] if uid is None else uid
    env=dict(os.environ,RUSTSIMHUB_PRIVATE_WORKER_ROLE=role,TMPDIR=str(ROOT/(role if role in roles else 'edge')))
    log=(ROOT/f'{role}-{uid}.log').open('w')
    process=subprocess.Popen(['/input/worker-test','--ignored','--exact','worker_actual_uid_role','--nocapture','--test-threads=1'],
        env=env,user=uid,group=uid,extra_groups=[983],stdout=log,stderr=subprocess.STDOUT)
    children.append((process,uid,log));return process
def wait_file(name):
    end=time.monotonic()+12
    while not (ROOT/name).exists():
        for process,uid,_ in children:
            assert process.poll() in (None,0),f'fixture role {uid} failed'
        assert time.monotonic()<end,f'timed out waiting for {name}'
        time.sleep(.01)
def production_entry():
    # Actual non-cfg(test) binary, fixed /run endpoint and Root-owned NSS image.
    # The valid registration's public address is never contacted: unknown handle
    # fails before allocation. No physical PCM factory can be authorized here.
    import stat
    material=json.loads((ROOT/'edge/offer.json').read_text())
    config={'protocol':2,'relay_handle':'approved','address':'8.8.8.8:5349',
        'hostname':'turn.fixture.invalid','ca_der':material['ca_der'],
        'username':'fixture','password':'synthetic-only',
        'until_ms':int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)+30000}
    directory=ROOT/'production-media';directory.mkdir(mode=0o700)
    for name,address in [('valid','8.8.8.8:5349'),('loopback','127.0.0.1:5349')]:
        config['address']=address;path=directory/(name+'.json');path.write_text(json.dumps(config));path.chmod(0o600);os.chown(path,984,984)
    os.chown(directory,984,984)
    path=pathlib.Path('/run/rustsimhub/media');path.mkdir(parents=True,mode=0o750);os.chown(path,984,983);path.chmod(0o2750)
    def start(name):
        log=(ROOT/f'production-{name}.log').open('w')
        process=subprocess.Popen(['/input/native-worker','--serve-remote-worker',str(directory/(name+'.json'))],
            user=984,group=984,extra_groups=[983],stdout=log,stderr=subprocess.STDOUT)
        children.append((process,984,log));return process
    bad=start('loopback');assert bad.wait(timeout=2)==1
    socket=path/'remote.sock';assert not socket.exists()
    process=start('valid');end=time.monotonic()+2
    while not socket.exists():
        assert process.poll() is None and time.monotonic()<end;time.sleep(.01)
    metadata=socket.lstat();assert metadata.st_uid==984 and stat.S_IMODE(metadata.st_mode)==0o660 and metadata.st_nlink==1
    probe = r"""import json,socket,struct,os
s=socket.socket(socket.AF_UNIX);s.settimeout(1);s.connect('/run/rustsimhub/media/remote.sock')
b=json.dumps({'protocol':2,'id':1,'command':{'op':'prepare','session':'a'*32,'relay_handle':'unknown','expected_peer':[7]*32,'offer':'not SDP'}}).encode()
try:
 s.sendall(struct.pack('!I',len(b))+b);header=s.recv(4)
 if os.getuid()!=985: assert header==b''
 else:
  assert len(header)==4
  length=struct.unpack('!I',header)[0];body=b''
  while len(body)<length: body+=s.recv(length-len(body))
  assert json.loads(body)['result']=={'Err':'denied'}
except (ConnectionResetError,BrokenPipeError): assert os.getuid()!=985
"""
    for uid in (984,986,987,985):
        subprocess.run(['python3','-c',probe],user=uid,group=uid,extra_groups=[983],check=True,timeout=2)
    subprocess.run(['python3','-c','import os,signal,sys;os.kill(int(sys.argv[1]),signal.SIGTERM)',str(process.pid)],
        user=984,group=984,extra_groups=[],check=True)
    assert process.wait(timeout=2)==0 and not socket.exists()
    return {'non_test_binary':True,'uid':984,'actual_root_nss':True,'loopback_config_rejected':True,
        'denied_uids':[984,986,987],'edge_uid':985,'unknown_handle_rejected_before_network':True,'sigterm_socket_removed':True,'pcm_opened':False}
try:
    launch('core');launch('media');launch('edge');wait_file('media/ready.json')
    # Test before the real owner connects, so Busy cannot hide a UID error.
    for uid in (984,986,987):
        process=launch('denied',uid);assert process.wait(timeout=2)==0
    (ROOT/'start-edge.json').write_text('{}\n');(ROOT/'start-edge.json').chmod(0o644)
    for name in ('core/done.json','media/done.json','edge/done.json'):wait_file(name)
    for process,uid,log in children:assert process.wait(timeout=4)==0,f'role {uid} did not finish'
    proofs={role:json.loads((ROOT/role/'done.json').read_text()) for role in roles}
    assert proofs['core']['uid']==986 and proofs['core']['actual_media_peer_uid']==984
    assert proofs['media']['uid']==984 and proofs['media']['mirror_revoked'] and proofs['media']['closed']==1
    assert proofs['edge']['uid']==985 and proofs['edge']['forced_relay_opus_both_directions']
    entry=production_entry()
    print(json.dumps({'production_cli':entry,'fixture':'actual-different-uids-remote-worker','proofs':proofs,'denied_uids':[984,986,987],
        'actors_capabilities':[],'synthetic_nss_only':True,'full_core_business_daemon':False},sort_keys=True))
except BaseException:
    for path in sorted(ROOT.glob('*.log')):
        print(path.name, path.read_text()[-6000:],flush=True)
    raise
finally:
    for process,uid,log in children:
        if process.poll() is None:
            # No CAP_KILL. A short helper first drops to that child's same UID.
            subprocess.run(['python3','-c','import os,signal;os.kill(int(__import__("sys").argv[1]),signal.SIGTERM)',str(process.pid)],
                user=uid,group=uid,extra_groups=[],check=False,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        log.close()
