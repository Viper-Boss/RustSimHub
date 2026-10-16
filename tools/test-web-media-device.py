#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Candidate-root HTTPS checks; optional explicitly marked single 10010 call."""
import argparse,hashlib,ipaddress,json,pathlib,re,shlex,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
from legacy_controller import LegacyControllerWindow
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey,Ed25519PublicKey
from cryptography.hazmat.primitives.serialization import Encoding,PublicFormat,PrivateFormat,NoEncryption
p=argparse.ArgumentParser(description=__doc__)
for name in ('nas','device','artifacts','native','report'):p.add_argument('--'+name,type=pathlib.Path,required=True)
p.add_argument('--root-directory',required=True)
p.add_argument('--call-10010',action='store_true',help='Requires standing human authorization; one marked call, 18s stop protection')
p.add_argument('--exclusive-controller',action='store_true',help='Temporarily stop legacy Go call control, independently restore it within 75s; requires --call-10010')
p.add_argument('--wifi-transport',action='store_true',help='Use existing wlan0 address and the same trusted TLS name; never changes Wi-Fi or cellular')
p.add_argument('--once-tag');a=p.parse_args();assert not a.report.exists()
assert not a.call_10010 or (a.once_tag and re.fullmatch('[a-z0-9-]{8,40}',a.once_tag))
assert not a.exclusive_controller or a.call_10010
assert any(a.report.resolve().is_relative_to((ROOT/name).resolve()) for name in ('.private','.local'))
assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs',a.root_directory)
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
build=json.loads((a.artifacts/'build.json').read_text());assert build['development_fixtures'] is False
q=shlex.quote;tag=uuid.uuid4().hex[:10];prefix='rustsimhub-web-media-'+tag
units={role:prefix+'-'+role for role in ('core','media','edge','duplicate')}
report={'passed':False,'browser_verified':False,'new_calls':0,'new_sms':0,'cellular_tests':False,'partition_writes':False,'units':units,'source_archive_sha256':build['source_archive_sha256'],'harness_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()}
nas=dev=ns=ds=None;base=client=None;started=[];owner=Ed25519PrivateKey.generate();pub=list(owner.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw));token=None
policy=None;timer=None;container=None;controller=None
def save():a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
    s=paramiko.SSHClient();s.load_host_keys(c['known_hosts']);s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=10,auth_timeout=10,banner_timeout=10,look_for_keys=False,allow_agent=False);return s
def run(c,body,sudo=False,check=True):
    i,o,e=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=60);o.channel.set_combine_stderr(True)
    if sudo:i.write(nc['password']+'\n')
    i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');code=o.channel.recv_exit_status()
    if code and check:report['failed_output']=raw;save();raise RuntimeError('HTTPS media fixture command failed; inspect private report')
    return code,raw
def put(s,path,data,uid,gid,mode=0o600):
    temp=path+'.partial-'+uuid.uuid4().hex
    with s.open(temp,'wx') as f:f.write(data)
    s.chmod(temp,mode);s.chown(temp,uid,gid);s.rename(temp,path)
def read(s,path,limit=65536):
    with s.open(path,'rb') as f:data=f.read(limit+1)
    assert len(data)<=limit;return data
def sign(challenge,purpose):
    data=bytes(challenge['transcript']);Ed25519PublicKey.from_public_bytes(bytes(pin)).verify(bytes(challenge['device_signature']),data);value=json.loads(data)
    assert value['domain']=='RustSimHub.Auth.v1' and value['purpose']==purpose and value['kind']=='owner' and value['client_public_key']==pub and value['device_public_key']==pin and value['scopes']==1023 and value['challenge_id']==challenge['id']
    return list(owner.sign(data))
def service(role,arguments,properties):
    cmd=['systemd-run','--unit='+units[role],'--collect']
    for prop in properties:cmd+=['-p',prop]
    run(dev,shlex.join(cmd+arguments));started.append(role)
def request(command=None,path='/api/v2',origin=None,connect_ip=None):
    identifier=uuid.uuid4().hex;body=None if command is None else {'protocol':1 if path=='/api/media/v1' else 2,'id':identifier,**command}
    payload=json.dumps({'host':tls_name,'connect_ip':connect_ip or transport_ip,'path':path,'origin':origin or 'https://'+tls_name+':8448','body':body},separators=(',',':'))
    raw=run(nas,'python3 '+q(client+'/request.py')+" <<'RSH_WEB_MEDIA_END'\n"+payload+'\nRSH_WEB_MEDIA_END')[1]
    response=json.loads(raw.splitlines()[-1]);value=response['body']
    if response['status']==200 and body is not None:assert value['protocol']==body['protocol'] and value['id']==identifier
    return response
def rpc(command):
    response=request({'command':command});assert response['status']==200 and not response['body']['result'].get('error');return response['body']['result']
def call_check():
    global policy,timer,container,controller
    once='/var/tmp/rustsimhub-web-10010-'+a.once_tag+'.once';report['once_marker']=once;save();run(dev,'test ! -e '+q(once))
    if a.exclusive_controller:
        controller=LegacyControllerWindow(lambda body:run(dev,body)[1],ds,base,units['core'])
        report['exclusive_controller']=controller.arm();save()
    rule=(ROOT/'firmware/development/voice.rules').read_bytes();policy='/etc/polkit-1/rules.d/49-rustsimhub-web-media-'+tag+'.rules';report['voice_rule_sha256']=hashlib.sha256(rule).hexdigest()
    rc,_=run(dev,"runuser -u rustsimhub-core -- sh -c 'pkcheck --action-id org.freedesktop.ModemManager1.Voice --process \"$$\"'",check=False);assert rc in (1,2,3)
    put(ds,policy,rule,0,0,0o644)
    for _ in range(10):
        if run(dev,"runuser -u rustsimhub-core -- sh -c 'pkcheck --action-id org.freedesktop.ModemManager1.Voice --process \"$$\"'",check=False)[0]==0:break
        time.sleep(.25)
    else:raise RuntimeError('Temporary Voice permission unavailable')
    deadline=time.monotonic()+10
    while True:
        status=rpc({'op':'call_status','token':token})
        if status['available']:assert status['active_calls']==0;break
        assert time.monotonic()<deadline;time.sleep(.2)
    line=rpc({'op':'line_status','token':token})['line']
    op=rpc({'op':'prepare_call','token':token,'line_epoch':line,'idempotency_key':'web-'+a.once_tag+'-dial','action':'dial','call_id':None,'payload':'10010'})['operation']['id']
    nas_route=run(nas,'ip -4 route get '+q(transport_ip))[1].split();nas_ip=str(ipaddress.IPv4Address(nas_route[nas_route.index('src')+1]));assert ipaddress.ip_address(nas_ip).is_private
    ns.put(str(a.native),client+'/rustsimhub-audio-alsa');ns.chmod(client+'/rustsimhub-audio-alsa',0o555)
    run(nas,'printf "%s  %s\\n" '+q(native_sha)+' '+q(client+'/rustsimhub-audio-alsa')+' | sha256sum -c -\nmkdir -m 0700 '+q(client+'/carrier'))
    put(ns,client+'/carrier/config.json',json.dumps({'mode':'offer_synthetic_gated','bind':nas_ip+':0','peer_ip':transport_ip,'seconds':6,'core_socket':None,'core_uid':None}).encode(),1000,1000)
    image=run(nas,'cat /mnt/docker-data/rustsimhub/p0/audio-builder-image.txt')[1].splitlines()[-1];assert image=='sha256:90158fe893786a51eaad5e126e4915031872974b3b27347b44a4ce41d760ba37'
    container=prefix+'-carrier';cmd=['docker','run','-d','--name',container,'--label','org.rustsimhub.purpose=https-media-one-10010','--network','host','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--user','1000:1000','--cpus','1','--memory','128m','--pids-limit','16','--mount','type=bind,src='+client+',dst=/test',image,'/test/rustsimhub-audio-alsa','--rtc-lan-peer','/test/carrier/config.json'];run(nas,shlex.join(cmd),sudo=True)
    def wait_file(path,seconds=10):
        end=time.monotonic()+seconds
        while True:
            try:return read(ns,path)
            except FileNotFoundError:
                assert time.monotonic()<end;time.sleep(.05)
    offer=wait_file(client+'/carrier/offer.sdp').decode();lines=[]
    for item in offer.splitlines():
        if item.startswith('a=candidate:'):
            fields=item.split();assert fields[4]==nas_ip;fields[4]='synthetic-peer.local';item=' '.join(fields)
        lines.append(item)
    offer='\r\n'.join(lines)+'\r\n'
    timer=prefix+'-deadline';run(dev,shlex.join(['systemd-run','--unit='+timer,'--on-active=18s','--timer-property=AccuracySec=100ms','/usr/bin/systemctl','stop',units['core']]))
    run(dev,'(set -C\nprintf "%s\\n%s\\n" '+q(base)+' '+q(op)+' > '+q(once)+')');report['dial_marker_consumed']=True;save()
    # Exactly one submit. All later work only queries this operation.
    submission=rpc({'op':'submit_call','token':token,'operation_id':op,'lease':None,'urgent':False});report['dial_submission_returned']=True;save()
    operation=rpc({'op':'call_operation','token':token,'operation_id':op});lease=operation['lease'];assert lease and lease['call_id'];report['new_calls']=1;save()
    end=time.monotonic()+5
    while True:
        rpc({'op':'call_status','token':token});detail=rpc({'op':'call_detail','token':token,'call_id':lease['call_id']})
        if detail['state']=='active':break
        assert time.monotonic()<end;time.sleep(.1)
    sid=uuid.uuid4().hex
    def media(action):
        response=request({'token':token,'lease':lease,'command':{'session':sid,**action}},path='/api/media/v1');assert response['status']==200
        view=response['body']['result'];assert not any(k in view for k in ('ticket','peer_key','binding','boot'));return view
    answer=media({'op':'prepare','offer':offer});assert answer['answer'] and not answer['pcm_opened'];put(ns,client+'/carrier/answer.sdp',answer['answer'].encode(),1000,1000)
    wait_file(client+'/carrier/verified.json');end=time.monotonic()+3
    while True:
        state=media({'op':'poll'})
        if state['phase']=='verified':break
        assert state['phase']=='handshaking' and time.monotonic()<end;time.sleep(.05)
    state=media({'op':'activate'});assert state['phase'] in ('authorizing','active');put(ns,client+'/carrier/synthetic-start',b'start\n',1000,1000)
    end=time.monotonic()+5
    while True:
        state=media({'op':'poll'});assert state['phase'] in ('authorizing','active') and state['failure'] is None
        if state['pcm_opened'] and state['counters']['captured']>=100 and state['counters']['received']>=75:break
        assert time.monotonic()<end;time.sleep(.15)
    assert media({'op':'activate'})['phase']=='active';report['web_media_active']=state;report['same_active_activation_not_reissued']=True
    # Stop the actual HTTPS process while PCM is live. Its cleanup must retire
    # the worker independently of stopping Core or the call.
    run(dev,'systemctl stop '+q(units['edge']));started.remove('edge')
    run(dev,'systemctl is-active '+q(units['core'])+' '+q(units['media'])+'\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done')
    report['active_pcm_closed_on_edge_exit_core_still_running']=True
    remote=json.loads(wait_file(client+'/carrier/result.json'));assert remote['dtls_verified'] and not remote['pcm_opened'] and remote['received_frames']>=75 and remote['decoded_peak']>0 and remote['closed'];report['nas_media']=remote
    run(dev,'systemctl stop '+q(units['core']));started.remove('core')
    report['https_media_call_passed']=True;report['mdns_mapped_to_observed_tls_peer']=True;save()
try:
    save();nas=connect(nc);channel=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=10);dev=connect(dc,channel);ds=dev.open_sftp();ns=nas.open_sftp();ip=str(ipaddress.IPv4Address(dc['host']));assert ipaddress.ip_address(ip).is_private
    wifi=None;transport_ip=ip;tls_name=ip
    if a.wifi_transport:
        info=json.loads(run(dev,'ip -j -4 addr show dev wlan0')[1]);addresses=[row['local'] for device in info for row in device['addr_info'] if row['family']=='inet' and row['scope']=='global'];assert len(addresses)==1
        wifi=str(ipaddress.IPv4Address(addresses[0]));assert ipaddress.ip_address(wifi).is_private and wifi!=ip;transport_ip=wifi;tls_name='rustsimhub-test.local';report['wifi_transport']=True;report['transport_address']=wifi
    run(dev,'test "$(systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs | grep -c ^active$)" = 4\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done\ntest "$(df -Pk /var/tmp | awk \'NR==2{print $4}\')" -gt 50000\ntest -z "$(ss -H -ltn \'sport = :8448\')"')
    raw=run(dev,'d=$(mktemp -d /var/tmp/rustsimhub-web-media-XXXXXXXX)\nchmod 0711 "$d"\ninstall -d -m 0555 "$d/bin"\ninstall -d -m 0700 "$d/operator"\ninstall -d -o 986 -g 986 -m 0700 "$d/state" "$d/identity"\ninstall -d -o 985 -g 985 -m 0700 "$d/edge"\ninstall -d -o 984 -g 984 -m 0700 "$d/media"\ninstall -d -o 986 -g 987 -m 2750 "$d/runtime-core"\ninstall -d -o 984 -g 987 -m 2750 "$d/runtime-media"\nprintf "DIR=%s\\n" "$d"')[1]
    base=re.search(r'DIR=(/var/tmp/rustsimhub-web-media-[A-Za-z0-9]{8})',raw).group(1);report['device_directory']=base;save()
    with a.report.with_suffix('.owner-private').open('xb') as keyfile:keyfile.write(owner.private_bytes(Encoding.Raw,PrivateFormat.Raw,NoEncryption()))
    names=['rustsimhub-core','rustsimhub-edge']
    for name in names:
        local=a.artifacts/name;assert hashlib.sha256(local.read_bytes()).hexdigest()==build['artifact_sha256'][name];ds.put(str(local),base+'/bin/'+name);ds.chmod(base+'/bin/'+name,0o555)
    native_sha=hashlib.sha256(a.native.read_bytes()).hexdigest();ds.put(str(a.native),base+'/bin/rustsimhub-audio-alsa');ds.chmod(base+'/bin/rustsimhub-audio-alsa',0o555)
    for name,sha in [(name,build['artifact_sha256'][name]) for name in names]+[('rustsimhub-audio-alsa',native_sha)]:run(dev,'printf "%s  %s\\n" '+q(sha)+' '+q(base+'/bin/'+name)+' | sha256sum -c -\n'+q(base+'/bin/'+name)+' --build-info | grep -q '+q('"source_archive_sha256":"'+build['source_archive_sha256']+'"'))
    inp,out,err=dev.exec_command(shlex.join(['runuser','-u','rustsimhub-core','--',base+'/bin/rustsimhub-core','--provision-owner',base+'/state',base+'/identity',bytes(pub).hex()]),timeout=20)
    challenge=json.loads(out.readline(4097));pin=json.loads(bytes(challenge['transcript']))['device_public_key'];proof=sign(challenge,'bootstrap');inp.write(json.dumps(proof));inp.channel.shutdown_write();tail=out.read().decode(errors='replace');assert out.channel.recv_exit_status()==0 and json.loads(tail.splitlines()[-1])['owner_initialized'];report['signed_production_owner_initialized']=True;save()
    put(ds,base+'/media/config.json',json.dumps({'control_socket':'/run/rustsimhub/media/worker.sock','core_socket':'/run/rustsimhub/core/media.sock','bind_ip':ip,'additional_bind_ips':[] if not wifi else [wifi]}).encode(),984,984)
    common=['RootDirectory='+a.root_directory,'MountAPIVFS=yes','TemporaryFileSystem=/run:mode=0755 /mnt:mode=0755 /var/tmp:mode=1777 /var/lib:mode=0755','CapabilityBoundingSet=','AmbientCapabilities=','NoNewPrivileges=yes','ProtectSystem=strict','ProtectHome=yes','RuntimeMaxSec=120','TimeoutStopSec=5','TasksMax=12']
    service('core',['/mnt/runtime-bins/rustsimhub-core','--serve-media-authorizing','/run/rustsimhub/core/core.sock','985',base+'/state',base+'/identity','/run/rustsimhub/core/media.sock','984'],common+['MemoryMax=96M','User=rustsimhub-core','Group=rustsimhub-core','SupplementaryGroups=rustsimhub-ipc','PrivateDevices=yes','BindReadOnlyPaths='+base+'/bin:/mnt/runtime-bins /run/dbus','BindPaths='+base+'/runtime-core:/run/rustsimhub/core '+base+'/state '+base+'/identity'])
    devices=['DeviceAllow=/dev/snd/'+name+' rw' for name in ('controlC0','pcmC0D0p','pcmC0D1c','pcmC0D4p','pcmC0D4c')]
    service('media',['/mnt/runtime-bins/rustsimhub-audio-alsa','--serve-worker','/var/lib/rustsimhub/media/config.json'],common+['User=rustsimhub-media','Group=rustsimhub-media','SupplementaryGroups=audio rustsimhub-ipc','PrivateDevices=no','DevicePolicy=closed','MemoryMax=128M','BindReadOnlyPaths='+base+'/bin:/mnt/runtime-bins '+base+'/runtime-core:/run/rustsimhub/core','BindPaths='+base+'/runtime-media:/run/rustsimhub/media '+base+'/media:/var/lib/rustsimhub/media /dev/snd']+devices)
    run(dev,'runuser -u rustsimhub-edge -- '+q(base+'/bin/rustsimhub-edge')+' --init-tls '+q(base+'/edge/tls')+' '+q(tls_name))
    config={'listen':ip+':8448','origin':'https://'+tls_name+':8448','core_socket':'/run/rustsimhub/core/core.sock','core_uid':986,'tls_directory':base+'/edge/tls','media_worker_socket':'/run/rustsimhub/media/worker.sock','additional_listen_ips':[] if not wifi else [wifi]}
    put(ds,base+'/edge/config.json',json.dumps(config).encode(),985,985)
    edge_props=common+['MemoryMax=96M','User=rustsimhub-edge','Group=rustsimhub-edge','SupplementaryGroups=rustsimhub-ipc','PrivateDevices=yes','BindReadOnlyPaths='+base+'/bin:/mnt/runtime-bins '+base+'/edge '+base+'/runtime-core:/run/rustsimhub/core '+base+'/runtime-media:/run/rustsimhub/media']
    run(dev,'n=0;while ! test -S '+q(base+'/runtime-core/core.sock')+' || ! test -S '+q(base+'/runtime-media/worker.sock')+';do n=$((n+1));test "$n" -lt 10;sleep 1;done')
    service('edge',['/mnt/runtime-bins/rustsimhub-edge','--serve',base+'/edge/config.json'],edge_props)
    raw=run(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/web-media-private-XXXXXXXX)\nprintf "DIR=%s\\n" "$d"')[1];client=re.search(r'DIR=(/mnt/docker-data/rustsimhub/p0/web-media-private-[A-Za-z0-9]{8})',raw).group(1);report['nas_directory']=client;save()
    put(ns,client+'/cert.pem',read(ds,base+'/edge/tls/cert.pem'),1000,1000)
    code='''import http.client,json,pathlib,socket,ssl,sys
d=json.load(sys.stdin)
class PinnedConnection(http.client.HTTPSConnection):
 def connect(self):
  raw=socket.create_connection((d['connect_ip'],8448),timeout=5)
  self.sock=self._context.wrap_socket(raw,server_hostname=self.host)
c=PinnedConnection(d['host'],8448,context=ssl.create_default_context(cafile=str(pathlib.Path(__file__).parent/'cert.pem')),timeout=5)
h={'Origin':d['origin'],'X-RustSimHub-Request':'2','Content-Type':'application/json'}
c.request('GET' if d['body'] is None else 'POST',d['path'],None if d['body'] is None else json.dumps(d['body'],separators=(',',':')),h)
r=c.getresponse();raw=r.read(65537);assert len(raw)<=65536
print(json.dumps({'status':r.status,'headers':dict(r.getheaders()),'body':json.loads(raw)}));c.close()
'''
    put(ns,client+'/request.py',code.encode(),1000,1000)
    deadline=time.monotonic()+15
    while True:
        try:capability=request(path='/api/media/capabilities');assert capability['status']==200;break
        except Exception:
            report['readiness_retries']=report.get('readiness_retries',0)+1
            if 'failed_output' in report:report.setdefault('readiness_diagnostics',[]).append(report.pop('failed_output'))
            assert time.monotonic()<deadline;time.sleep(.25)
    assert capability['body']['enabled'] is True and capability['body']['remote_relay'] is False
    challenge=rpc({'op':'challenge','public_key':pub});token=rpc({'op':'authenticate','challenge_id':challenge['id'],'signature':sign(challenge,'login')})['token'];assert len(token)==64;report['strict_tls_signed_owner_login_passed']=True
    if wifi:
        assert request({'command':{'op':'identity'}},connect_ip=ip)['body']['result']['public_key']==pin
        assert rpc({'op':'identity'})['public_key']==pin
        assert request({'command':{'op':'health','token':token}},connect_ip=ip)['body']['result']['ok'] is True
        report['usb_wifi_same_identity_certificate_origin_and_owner_token_verified']=True
    fake={'call_id':'00000000-0000-4000-8000-000000000001','boot':'a'*64,'line':1,'generation':1,'expires_in_seconds':30}
    media={'token':token,'lease':fake,'command':{'op':'prepare','session':'a'*32,'offer':'bounded invalid before authentication'}}
    assert request(media,path='/api/media/v1')['status']==403
    assert request(media,path='/api/media/v1',origin='https://evil.invalid')['status']==403
    media['command']={'op':'stop_all','session':'a'*32};assert request(media,path='/api/media/v1')['status']==400
    assert request({'command':{'op':'issue_media','token':token,'lease':fake,'peer_key':[7]*32}})['status']==403
    assert request({'command':{'op':'check_media_lease','token':token,'lease':fake}})['status']==403
    report['inactive_lease_cross_origin_local_command_and_direct_ticket_denied']=True
    for role,uid,name in [('core',986,'rustsimhub-core'),('media',984,'rustsimhub-audio-alsa'),('edge',985,'rustsimhub-edge')]:
        pid=run(dev,'systemctl show --value -p MainPID '+q(units[role]))[1].strip();assert pid.isdecimal() and int(pid)>0
        run(dev,'test "$(stat -Lc %d:%i /proc/'+pid+'/root)" = "$(stat -Lc %d:%i '+q(a.root_directory)+')"\ntest "$(stat -Lc %d:%i /proc/'+pid+'/exe)" = "$(stat -Lc %d:%i '+q(base+'/bin/'+name)+')"\ntest "$(awk \'/^Uid:/{print $2}\' /proc/'+pid+'/status)" = '+str(uid)+'\nfor cap in CapEff CapPrm CapAmb CapBnd;do test "$(awk -v c="$cap:" \'$1==c{print $2}\' /proc/'+pid+'/status)" = 0000000000000000;done')
    report['candidate_roots_exes_fixed_uids_empty_caps_verified']=True
    cmd=['systemd-run','--quiet','--wait','--pipe','--collect','--unit='+units['duplicate']]
    for prop in edge_props:cmd+=['-p',prop]
    rc,_=run(dev,shlex.join(cmd+['/mnt/runtime-bins/rustsimhub-edge','--serve',base+'/edge/config.json']),check=False);assert rc!=0
    assert request(path='/api/media/capabilities')['status']==200;report['duplicate_edge_refused_original_healthy']=True
    if a.call_10010:call_check()
    else:run(dev,'systemctl stop '+q(units['edge']));started.remove('edge')
    assert run(dev,'systemctl is-active '+q(units['media']))[1].strip()=='active'
    report.update(passed=True,edge_graceful_exit_kept_idle_worker_healthy=True)
    print('Candidate HTTPS media and lifecycle passed; calls='+str(report['new_calls'])+'; SMS/cellular tests=0; browser unverified')
except Exception as exc:
    report['error']=str(exc)
    if dev:report['unit_status']=run(dev,'systemctl status --no-pager '+shlex.join(units.values()),check=False)[1]
    raise
finally:
    if dev:
        for role in reversed(started):run(dev,'systemctl stop '+q(units[role]),check=False)
        if timer:run(dev,'systemctl stop '+q(timer+'.timer')+' '+q(timer+'.service'),check=False)
        if controller:
            try:report['legacy_controller_restored']=controller.release()
            except Exception as exc:report['legacy_controller_restore_error']=str(exc)
        if policy:
            rc,_=run(dev,'printf "%s  %s\\n" '+q(report['voice_rule_sha256'])+' '+q(policy)+' | sha256sum -c -\nrm -- '+q(policy),check=False);report['voice_rule_removed']=rc==0
            for _ in range(10):
                denied=run(dev,"runuser -u rustsimhub-core -- sh -c 'pkcheck --action-id org.freedesktop.ModemManager1.Voice --process \"$$\"'",check=False)[0]
                if denied in (1,2,3):break
                time.sleep(.25)
            report['voice_denied_after_cleanup']=denied in (1,2,3)
        rc,_=run(dev,'test "$(systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs | grep -c ^active$)" = 4\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done',check=False);report['cleanup_verified']=rc==0 and (not controller or report.get('legacy_controller_restored') is True) and (not policy or (report.get('voice_rule_removed') and report.get('voice_denied_after_cleanup')))
        if a.call_10010:
            rc,_=run(dev,"calls=$(mmcli -m 0 --voice-list-calls)\nif printf '%s\\n' \"$calls\" | grep -Ev '^\\s*/org/freedesktop/ModemManager1/Call/[0-9]+ (incoming|outgoing) \\(terminated\\)$|^No calls were found\\.?$';then exit 4;fi",check=False);report['calls_idle_after_cleanup']=rc==0;report['cleanup_verified']=report['cleanup_verified'] and rc==0
    if nas and container:run(nas,'docker stop --time 2 '+q(container),sudo=True,check=False)
    save()
    for c in (ds,ns,dev,nas):
        if c:c.close()
