#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Disposable actual Core Auth/calls controller -> normal host -> TURN worker."""
import json,os,pathlib,subprocess,time
ROOT=pathlib.Path('/fixture')
assert os.getuid()==0 and os.environ.get('RUSTSIMHUB_PRIVATE_UID_FIXTURE')=='1'
for name,uid in [('core',986),('media',984),('edge',985)]:
    path=ROOT/name;path.mkdir(mode=0o750);os.chown(path,uid,983);path.chmod(0o2750)
for name,uid in [('core',986),('media',984)]:
    path=pathlib.Path('/run/rustsimhub')/name;path.mkdir(parents=True,mode=0o750);os.chown(path,uid,983);path.chmod(0o2750)
children=[]
def launch(name,uid,binary,test):
    env=dict(os.environ,RUSTSIMHUB_NORMAL_CORE_FIXTURE='1',RUSTSIMHUB_PRIVATE_WORKER_ROLE=name,TMPDIR=str(ROOT/name))
    log=(ROOT/(name+'.log')).open('w')
    child=subprocess.Popen([binary,'--ignored','--exact',test,'--nocapture','--test-threads=1'],env=env,
        user=uid,group=uid,extra_groups=[983],stdout=log,stderr=subprocess.STDOUT)
    children.append((child,uid,log));return child
def wait(name):
    end=time.monotonic()+15
    while not (ROOT/name).exists():
        assert all(child.poll() in (None,0) for child,_,_ in children),f'normal actor failed before {name}'
        assert time.monotonic()<end,f'timeout {name}'
        time.sleep(.01)
try:
    launch('core',986,'/input/core-test','calls::checks::remote_media::normal_core_actual_uid_role')
    launch('media',984,'/input/worker-test','worker_actual_uid_role')
    launch('edge',985,'/input/worker-test','normal_host::actual_uid_edge_normal_host')
    for role in ('core','media','edge'):wait(role+'/done.json')
    for child,uid,_ in children:assert child.wait(timeout=4)==0,f'normal actor {uid} failed'
    core=json.loads((ROOT/'core/done.json').read_text());edge=json.loads((ROOT/'edge/done.json').read_text());media=json.loads((ROOT/'media/done.json').read_text())
    assert core['normal_auth_and_calls_controller'] and core['original_owner_revocation']
    assert core['fake_modem_creates']==1 and core['fake_modem_starts']==1
    assert edge['normal_host_non_test_library'] and edge['forced_relay_opus_both_directions'] and edge['owner_revocation_stops_media']
    assert media['uid']==984 and media['opened']==1 and media['closed']==1 and media['mirror_revoked']
    print(json.dumps({'fixture':'actual-normal-core-signaling-turn-three-uids','core':core,'edge':edge,'media':media,
        'actors_effective_capabilities':0,'physical_device':False,'synthetic_external_backends':['private ModemManager DBus','PCM','normal TLS TURN registration','remote audio peer'],
        'core_auth_controller_media_authority_mocked':False},sort_keys=True))
except BaseException:
    for path in sorted(ROOT.glob('*.log')):print(path.name,path.read_text()[-7000:],flush=True)
    raise
finally:
    for child,uid,log in children:
        if child.poll() is None:
            subprocess.run(['python3','-c','import os,signal,sys;os.kill(int(sys.argv[1]),signal.SIGTERM)',str(child.pid)],
                user=uid,group=uid,extra_groups=[],check=False,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        log.close()
