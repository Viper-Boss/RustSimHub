#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Private candidate-root worker test; real DTLS, no Core ticket, PCM or dial."""
import argparse,hashlib,ipaddress,json,pathlib,re,shlex,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko

p=argparse.ArgumentParser(description=__doc__)
for name in ('nas','device','artifacts','native','report'):p.add_argument('--'+name,type=pathlib.Path,required=True)
p.add_argument('--root-directory',required=True)
a=p.parse_args()
assert not a.report.exists()
assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs',a.root_directory)
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
build=json.loads((a.artifacts/'build.json').read_text())
assert build['development_fixtures'] is False
source=build['source_archive_sha256'];native_sha=hashlib.sha256(a.native.read_bytes()).hexdigest()
assert hashlib.sha256((a.artifacts/'simhubctl').read_bytes()).hexdigest()==build['artifact_sha256']['simhubctl']
q=shlex.quote;tag=uuid.uuid4().hex[:10];unit='rustsimhub-worker-check-'+tag
report={'passed':False,'new_calls':0,'new_sms':0,'cellular_tests':False,'partition_writes':False,'production_service_installed':False,'source_archive_sha256':source,'native_sha256':native_sha,'unit':unit,'sessions':[]}
nas=dev=ns=ds=None;base=client=None;started=False;containers=[];request_id=0

def save():a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
    s=paramiko.SSHClient();s.load_host_keys(c['known_hosts']);s.set_missing_host_key_policy(paramiko.RejectPolicy())
    s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=10,auth_timeout=10,banner_timeout=10,look_for_keys=False,allow_agent=False);return s
def run(c,body,sudo=False,check=True):
    i,o,e=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=45);o.channel.set_combine_stderr(True)
    if sudo:i.write(nc['password']+'\n')
    i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
    if rc and check:report['failed_output']=raw;save();raise RuntimeError('Worker preflight failed; inspect private report')
    return rc,raw
def put(s,path,data,uid=0,gid=0,mode=0o600):
    with s.open(path,'wx') as f:f.write(data)
    s.chmod(path,mode);s.chown(path,uid,gid)
def read(s,path,limit=24576):
    with s.open(path,'rb') as f:data=f.read(limit+1)
    assert len(data)<=limit;return data
def wait_file(s,path,seconds=10):
    end=time.monotonic()+seconds
    while True:
        try:return read(s,path)
        except FileNotFoundError:
            if time.monotonic()>=end:raise RuntimeError('Bounded private signaling deadline')
            time.sleep(.05)
def pcm_closed():run(dev,'for f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done')
def worker(command,uid=985):
    global request_id
    request_id+=1;prefix=base+('/edge' if uid==985 else '/operator')+'/'+str(request_id)
    put(ds,prefix+'.request.json',json.dumps({'protocol':1,'id':request_id,'command':command}).encode(),uid,uid)
    invocation='/mnt/runtime-bins/simhubctl worker-request /run/rustsimhub/media/worker.sock '+prefix+'.request.json '+prefix+'.reply.json'
    # Use the same candidate userspace and socket bind mounts as the worker.
    cmd=['systemd-run','--quiet','--wait','--pipe','--collect','--unit='+unit+'-request-'+str(request_id)]
    props=['User='+str(uid),'Group='+str(uid),'SupplementaryGroups=rustsimhub-ipc','RootDirectory='+a.root_directory,'MountAPIVFS=yes','TemporaryFileSystem=/run:mode=0755 /mnt:mode=0755 /var/tmp:mode=1777','BindReadOnlyPaths='+base+'/bin:/mnt/runtime-bins','BindPaths='+base+'/runtime-media:/run/rustsimhub/media '+prefix.rsplit('/',1)[0],'PrivateDevices=yes','PrivateNetwork=yes','CapabilityBoundingSet=','AmbientCapabilities=','NoNewPrivileges=yes','ProtectSystem=strict','RuntimeMaxSec=5','TimeoutStopSec=2']
    for prop in props:cmd+=['-p',prop]
    before=time.monotonic()
    rc,_=run(dev,shlex.join(cmd+shlex.split(invocation)),check=False)
    report.setdefault('request_timings',[]).append({'id':request_id,'op':command['op'],'elapsed_seconds':round(time.monotonic()-before,3),'exit':rc})
    save()
    if uid==985 and rc:raise RuntimeError('Edge worker SDK failed; inspect private request timing and unit logs')
    if uid!=985:
        assert rc!=0
        try:ds.stat(prefix+'.reply.json');raise AssertionError('Unauthorized peer received worker reply')
        except FileNotFoundError:pass
        return None
    reply=json.loads(read(ds,prefix+'.reply.json'));assert reply['id']==request_id and reply['protocol']==1
    return reply['result']
def wait_closed(session):
    end=time.monotonic()+4
    while True:
        result=worker({'op':'inspect','session':session});snapshot=result['Ok']
        if snapshot['phase']=='closed':assert snapshot['pcm_opened'] is False;pcm_closed();return snapshot
        assert time.monotonic()<end
        time.sleep(.05)
def verify_pid(pid):
    current=run(dev,'systemctl show --value -p MainPID '+q(unit))[1].strip();assert current==pid
    run(dev,'test "$(stat -Lc %d:%i /proc/'+pid+'/root)" = "$(stat -Lc %d:%i '+q(a.root_directory)+')"\ntest "$(stat -Lc %d:%i /proc/'+pid+'/exe)" = "$(stat -Lc %d:%i '+q(base+'/bin/rustsimhub-audio-alsa')+')"\ntest "$(awk \'/^Uid:/{print $2}\' /proc/'+pid+'/status)" = 984\nfor cap in CapEff CapPrm CapAmb CapBnd;do test "$(awk -v c="$cap:" \'$1==c{print $2}\' /proc/'+pid+'/status)" = 0000000000000000;done')

try:
    save();nas=connect(nc);channel=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=10);dev=connect(dc,channel);ds=dev.open_sftp();ns=nas.open_sftp()
    pcm_closed();run(dev,'test "$(systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs | grep -c ^active$)" = 4\ntest -d '+q(a.root_directory))
    dev_ip=str(ipaddress.IPv4Address(dc['host']));route=run(nas,'ip -4 route get '+q(dev_ip))[1].split();nas_ip=str(ipaddress.IPv4Address(route[route.index('src')+1]));assert ipaddress.ip_address(nas_ip).is_private
    raw=run(dev,'d=$(mktemp -d /var/tmp/rustsimhub-worker-check-XXXXXXXX)\nchmod 0711 "$d"\ninstall -d -m 0555 "$d/bin"\ninstall -d -o 985 -g 985 -m 0700 "$d/edge"\ninstall -d -m 0700 "$d/operator"\ninstall -d -o 984 -g 984 -m 0700 "$d/media"\ninstall -d -o 984 -g 987 -m 2750 "$d/runtime-media"\nprintf "DIR=%s\\n" "$d"')[1]
    base=re.search(r'DIR=(/var/tmp/rustsimhub-worker-check-[A-Za-z0-9]{8})',raw).group(1);report['device_directory']=base;save()
    for local,name in ((a.native,'rustsimhub-audio-alsa'),(a.artifacts/'simhubctl','simhubctl')):
        ds.put(str(local),base+'/bin/'+name);ds.chmod(base+'/bin/'+name,0o555)
    run(dev,'printf "%s  %s\\n" '+q(native_sha)+' '+q(base+'/bin/rustsimhub-audio-alsa')+' | sha256sum -c -\n'+q(base+'/bin/rustsimhub-audio-alsa')+' --build-info | grep -q '+q('"source_archive_sha256":"'+source+'"')+'\nprintf "%s  %s\\n" '+q(build['artifact_sha256']['simhubctl'])+' '+q(base+'/bin/simhubctl')+' | sha256sum -c -')
    put(ds,base+'/media/config.json',json.dumps({'control_socket':'/run/rustsimhub/media/worker.sock','core_socket':'/run/rustsimhub/core/media.sock','bind_ip':dev_ip}).encode(),984,984)
    props=['User=rustsimhub-media','Group=rustsimhub-media','SupplementaryGroups=audio rustsimhub-ipc','RootDirectory='+a.root_directory,'MountAPIVFS=yes','TemporaryFileSystem=/run:mode=0755 /mnt:mode=0755 /var/tmp:mode=1777 /var/lib:mode=0755','BindReadOnlyPaths='+base+'/bin:/mnt/runtime-bins','BindPaths='+base+'/runtime-media:/run/rustsimhub/media '+base+'/media:/var/lib/rustsimhub/media /dev/snd','PrivateDevices=no','DevicePolicy=closed','NoNewPrivileges=yes','CapabilityBoundingSet=','AmbientCapabilities=','ProtectSystem=strict','ProtectHome=yes','PrivateTmp=yes','RuntimeMaxSec=180','TimeoutStopSec=3','MemoryMax=128M','TasksMax=12']
    for name in ('controlC0','pcmC0D0p','pcmC0D1c','pcmC0D4p','pcmC0D4c'):props+=['DeviceAllow=/dev/snd/'+name+' rw']
    cmd=['systemd-run','--unit='+unit,'--collect']
    for prop in props:cmd+=['-p',prop]
    run(dev,shlex.join(cmd+['/mnt/runtime-bins/rustsimhub-audio-alsa','--serve-worker','/var/lib/rustsimhub/media/config.json']));started=True
    run(dev,'n=0;while ! test -S '+q(base+'/runtime-media/worker.sock')+';do n=$((n+1));test "$n" -lt 8;sleep 1;done')
    pid=run(dev,'systemctl show --value -p MainPID '+q(unit))[1].strip();assert pid.isdecimal() and int(pid)>0;verify_pid(pid);report['worker_pid']=int(pid)
    worker({'op':'inspect','session':'a'*32},uid=0);report['root_control_caller_denied']=True;pcm_closed()
    assert worker({'op':'inspect','session':'a'*32})=={'Err':'closed'}
    report['edge_control_inspect_works']=True
    raw=run(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/worker-check-private-XXXXXXXX)\nprintf "DIR=%s\\n" "$d"\ncat /mnt/docker-data/rustsimhub/p0/audio-builder-image.txt')[1]
    m=re.search(r'DIR=(/mnt/docker-data/rustsimhub/p0/worker-check-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',raw);assert m;client,image=m.groups();report['nas_directory']=client;save()
    ns.put(str(a.native),client+'/rustsimhub-audio-alsa');ns.chmod(client+'/rustsimhub-audio-alsa',0o555)
    run(nas,'printf "%s  %s\\n" '+q(native_sha)+' '+q(client+'/rustsimhub-audio-alsa')+' | sha256sum -c -')
    for round_number in (1,2):
        peer=client+'/peer'+str(round_number);run(nas,'mkdir -m 0700 '+q(peer))
        put(ns,peer+'/config.json',json.dumps({'mode':'offer_synthetic_gated','bind':nas_ip+':0','peer_ip':dev_ip,'seconds':1,'core_socket':None,'core_uid':None}).encode(),1000,1000)
        container=unit+'-peer'+str(round_number);containers.append(container)
        cmd=['docker','run','-d','--name',container,'--label','org.rustsimhub.purpose=worker-no-call-preflight','--network','host','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--user','1000:1000','--cpus','1','--memory','128m','--pids-limit','16','--mount','type=bind,src='+client+',dst=/test',image,'/test/rustsimhub-audio-alsa','--rtc-lan-peer','/test/peer'+str(round_number)+'/config.json']
        run(nas,shlex.join(cmd),sudo=True)
        offer=wait_file(ns,peer+'/offer.sdp').decode();session=uuid.uuid4().hex;prepare={'op':'prepare','session':session,'peer_ip':nas_ip,'offer':offer}
        first=worker(prepare)['Ok'];assert not first['pcm_opened'];answer=first['answer'];assert answer
        if round_number==1:
            assert worker(prepare)['Ok']['answer']==answer
            assert worker(dict(prepare,session=uuid.uuid4().hex))=={'Err':'busy'}
            assert worker(dict(prepare,offer=offer+'changed'))=={'Err':'denied'}
        put(ns,peer+'/answer.sdp',answer.encode(),1000,1000)
        wait_file(ns,peer+'/verified.json')
        deadline=time.monotonic()+5
        while True:
            snapshot=worker({'op':'inspect','session':session})['Ok'];assert not snapshot['pcm_opened'];pcm_closed()
            if snapshot['phase']=='verified':break
            assert time.monotonic()<deadline;time.sleep(.05)
        actual=snapshot['peer_key'];assert len(actual)==32 and any(actual)
        credentials={'ticket':[9]*32,'boot':'a'*64,'peer_key':([0]*32 if round_number==1 else actual)}
        result=worker({'op':'authorize','session':session,'credentials':credentials})
        if round_number==1:assert result=={'Err':'denied'}
        else:assert result['Ok']['phase'] in ('authorizing','closed') and not result['Ok']['pcm_opened']
        closed=wait_closed(session);assert worker(prepare)['Ok']['phase']=='closed';verify_pid(pid)
        report['sessions'].append({'id':session,'real_dtls_verified':True,'pcm_opened':False,'terminal':closed['phase'],'failure':closed['failure'],'case':'substituted-peer' if round_number==1 else 'absent-core-no-ticket'})
        run(nas,'docker stop --time 2 '+q(container),sudo=True);save()
    run(dev,'systemctl stop '+q(unit));started=False;pcm_closed()
    run(dev,'test ! -S '+q(base+'/runtime-media/worker.sock'))
    report.update(passed=True,persistent_pid_survived_both_sessions=True,worker_socket_removed_on_stop=True)
    print('Two real DTLS sessions, fixed Media identity, denied controls and no unauthorized PCM passed')
except Exception as exc:
    report['error']=str(exc)
    if dev:report['unit_status']=run(dev,'systemctl status --no-pager '+q(unit),check=False)[1]
    raise
finally:
    if dev:
        if started:run(dev,'systemctl stop '+q(unit),check=False)
        rc,raw=run(dev,'test "$(systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs | grep -c ^active$)" = 4\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done',check=False)
        report['cleanup_verified']=rc==0
    if nas:
        for container in containers:run(nas,'docker stop --time 2 '+q(container),sudo=True,check=False)
    save()
    for c in (ds,ns,dev,nas):
        if c:c.close()
