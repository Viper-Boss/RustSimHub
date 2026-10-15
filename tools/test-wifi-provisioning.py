#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit production Wi-Fi provisioning acceptance over USB; no cellular, calls or SMS."""
import argparse,hashlib,json,pathlib,re,secrets,shlex,sqlite3,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey,Ed25519PublicKey
from cryptography.hazmat.primitives.serialization import Encoding,PublicFormat,PrivateFormat,NoEncryption
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--platform',action='store_true',help='Use one Core for telephony/media authorization/history/NAS/network')
p.add_argument('--nas-adapter',help='Optional verified NAS Go adapter for platform acceptance')
for name in ['nas','device','artifacts','report']:p.add_argument('--'+name,type=pathlib.Path,required=True)
a=p.parse_args();assert not a.report.exists()
assert not a.nas_adapter or a.platform
meta=json.loads((a.artifacts/'build.json').read_text());nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
names=['rustsimhub-core','rustsimhub-edge']+(['rustsimhub-migrate'] if a.platform else [])
for name in names:assert hashlib.sha256((a.artifacts/name).read_bytes()).hexdigest()==meta['artifact_sha256'][name]
tag=uuid.uuid4().hex[:12];units=['rustsimhub-wifi-config-core-'+tag,'rustsimhub-wifi-config-edge-'+tag]
report={'passed':False,'source_archive_sha256':meta['source_archive_sha256'],'units':units,'new_calls':0,'new_sms':0,'cellular_tests':False,'browser_verified':False,'different_ssid_success_verified':False,'combined_platform':a.platform,'original_data_read':a.platform}
nas=dev=sf=None;started=False;policy=None;policy_hash=None;base=None;trials=[];original=None;pin=None;client=None
q=shlex.quote
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(body,check=True,remote=None):
 i,o,e=(remote or dev).exec_command('sh -s',timeout=70);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc and check:report['failure_output']=raw;save();raise RuntimeError('Private Wi-Fi acceptance command failed')
 return rc,raw
def write(path,data,uid,gid):
 with sf.open(path,'wx') as f:f.write(data)
 sf.chmod(path,0o600);sf.chown(path,uid,gid)
def call(command):
 identifier=uuid.uuid4().hex;body=json.dumps({'protocol':2,'id':identifier,'command':command},separators=(',',':'))
 _,raw=run('python3 '+q(client+'/request.py')+' '+q(dc['host'])+" <<'RSH_WIFI_REQUEST_END'\n"+body+'\nRSH_WIFI_REQUEST_END',remote=nas)
 value=json.loads(raw.splitlines()[-1]);assert value['protocol']==2 and value['id']==identifier;return value['result']
def pub(k):return list(k.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw))
def sign(c,k,purpose,kind,scopes):
 raw=bytes(c['transcript']);Ed25519PublicKey.from_public_bytes(bytes(pin)).verify(bytes(c['device_signature']),raw);t=json.loads(raw)
 assert t['domain']=='RustSimHub.Auth.v1' and t['purpose']==purpose and t['kind']==kind and t['scopes']==scopes and t['client_public_key']==pub(k) and t['device_public_key']==pin and t['challenge_id']==c['id']
 assert c['expires_in_seconds']==60;return list(k.sign(raw))
def login(k,kind='owner',scopes=1023):
 c=call({'op':'challenge','public_key':pub(k)});return call({'op':'authenticate','challenge_id':c['id'],'signature':sign(c,k,'login',kind,scopes)})['token']
def core_start(mode):
 if mode=='--serve-local-setup':extra=[base+'/setup/code']
 elif a.platform:
  mode='--serve-platform';extra=[base+'/runtime/media.sock',str(mediauid),base+'/archive',base+'/access',base+'/compat',base+'/journal']
 else:extra=[base+'/journal']
 rw=['state','identity','runtime','journal']+(['access','compat'] if a.platform else [])
 ro=['bin','setup']+(['archive'] if a.platform else [])
 args=['systemd-run','--unit='+units[0],'--collect','-p','User=rustsimhub-core','-p','Group=rustsimhub-core','-p','SupplementaryGroups=rustsimhub-ipc','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','BindPaths='+' '.join(base+'/'+n for n in rw),'-p','BindReadOnlyPaths='+' '.join(base+'/'+n for n in ro),'-p','RuntimeMaxSec=900','-p','TimeoutStopSec=8','-p','MemoryMax=96M','-p','TasksMax=16','-p','LimitCORE=0']
 if mode=='--serve-local-setup':args+=['-p','InaccessiblePaths=/run/dbus']
 run(shlex.join(args+[core,mode,socket,str(edgeuid),base+'/state',base+'/identity']+extra));run('n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 20;sleep 1;done');time.sleep(.3)
def begin(token,key,ssid,secret):
 r=call({'op':'wifi_begin','token':token,'idempotency_key':key,'ssid':ssid,'passphrase':secret})
 if 'trial_id' in r:trials.append(r['trial_id']);report['trial_ids']=trials;save()
 return r
def state(token,trial):return call({'op':'wifi_status','token':token,'trial_id':trial})
def wait_state(token,trial,desired,seconds=25):
 deadline=time.monotonic()+seconds
 while True:
  r=state(token,trial)
  if r.get('state') in desired:return r
  assert r.get('state') not in ['saved','rolled_back','unknown'] and time.monotonic()<deadline;time.sleep(.25)
def trial_uuid(t):return t[:8]+'-'+t[8:12]+'-4'+t[12:15]+'-8'+t[15:18]+'-'+t[18:30]
def profiles():return run('nmcli -g UUID connection show')[1].splitlines()
def original_ready():
 _,raw=run('nmcli -g GENERAL.CON-UUID,GENERAL.STATE device show wlan0');lines=raw.splitlines();return len(lines)==2 and lines[0]==original and lines[1].startswith('100')
def restore_wait(seconds=55):
 deadline=time.monotonic()+seconds
 while True:
  _,raw=run('busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints')
  if original_ready() and raw.strip()=='ao 0':return
  assert time.monotonic()<deadline;time.sleep(1)
def go(operation,request=None):
 command=q(a.nas_adapter)+' '+operation+' '+q(client+'/adapter.json')
 if request is not None:command+=" <<'RSH_GO_READ_END'\n"+json.dumps(request,separators=(',',':'))+'\nRSH_GO_READ_END'
 _,raw=run(command,remote=nas);return json.loads(raw.splitlines()[-1])
def wait_inbox(token):
 deadline=time.monotonic()+20
 while True:
  inbox=call({'op':'inbox','token':token,'after':None,'limit':16})
  if inbox['messages']:return inbox
  assert time.monotonic()<deadline;time.sleep(.5)
try:
 nas=connect(nc);channel=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,channel);sf=dev.open_sftp()
 _,raw=run('nmcli -g GENERAL.CON-UUID device show wlan0');original=raw.strip();assert re.fullmatch('[0-9a-f-]{36}',original)
 before=profiles();assert original in before
 # Values stay in private Python memory. Do not log the SSID or PSK or include either in reports.
 _,raw=run('nmcli --escape no --show-secrets -g 802-11-wireless.ssid connection show uuid '+q(original));ssid=raw.rstrip('\n')
 _,raw=run('nmcli --escape no --show-secrets -g 802-11-wireless-security.psk connection show uuid '+q(original));secret=raw.rstrip('\n');assert 0<len(ssid.encode())<=32 and 8<=len(secret)<=64
 _,wwan=run('busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager WwanEnabled')
 _,raw=run('busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints');assert raw.strip()=='ao 0'
 _,raw=run('d=$(mktemp -d /var/tmp/rustsimhub-wifi-config-XXXXXXXX);chmod 0711 "$d";install -d -m 0755 "$d/bin";for n in state identity setup journal archive access compat;do install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$d/$n";done;install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";install -d -o rustsimhub-edge -g rustsimhub-edge -m 0700 "$d/edge";printf "BASE=%s\\n" "$d";id -u rustsimhub-core;id -g rustsimhub-core;id -u rustsimhub-edge;id -g rustsimhub-edge')
 m=re.search(r'BASE=(/var/tmp/rustsimhub-wifi-config-[A-Za-z0-9]{8})\n(\d+)\n(\d+)\n(\d+)\n(\d+)',raw);assert m;base=m[1];coreuid,coregid,edgeuid,edgegid=map(int,m.groups()[1:]);report['device_directory']=base
 mediauid=int(run('id -u rustsimhub-media')[1].strip())
 for name in names:
  sf.put(str(a.artifacts/name),base+'/bin/'+name);sf.chmod(base+'/bin/'+name,0o555);_,raw=run('echo '+q(meta['artifact_sha256'][name]+'  '+base+'/bin/'+name)+' | sha256sum -c -\n'+q(base+'/bin/'+name)+' --build-info');info=json.loads(raw.splitlines()[-1]);assert info['source_archive_sha256']==meta['source_archive_sha256']
  if name=='rustsimhub-core':assert not info['development_fixtures']
 core=base+'/bin/rustsimhub-core';edge=base+'/bin/rustsimhub-edge';socket=base+'/runtime/core.sock';setup=secrets.token_bytes(32);write(base+'/setup/code',setup,coreuid,coregid)
 run('runuser -u rustsimhub-edge -- '+q(edge)+' --init-tls '+q(base+'/edge/tls')+' '+q(dc['host']))
 write(base+'/edge/config.json',json.dumps({'listen':dc['host']+':8448','origin':'https://'+dc['host']+':8448','core_socket':socket,'core_uid':coreuid,'tls_directory':base+'/edge/tls','legacy_compat':a.platform}).encode(),edgeuid,edgegid)
 local=ROOT/'.private'/('wifi-config-'+tag);local.mkdir(mode=0o700);sf.get(base+'/edge/tls/cert.pem',str(local/'cert.pem'))
 if a.platform:
  source=local/'synthetic.sqlite'
  with sqlite3.connect(source) as db:db.executescript((ROOT/'crates/compat/src/history/test-schema.sql').read_text(encoding='utf-8'))
  sf.put(str(source),base+'/setup/synthetic.sqlite');sf.chown(base+'/setup/synthetic.sqlite',coreuid,coregid);sf.chmod(base+'/setup/synthetic.sqlite',0o400)
  _,raw=run('runuser -u rustsimhub-core -- '+q(base+'/bin/rustsimhub-migrate')+' import '+q(base+'/setup/synthetic.sqlite')+' '+q('a'*64)+' '+q(base+'/archive'));assert json.loads(raw.splitlines()[-1])['inserted']==15
 _,raw=run('mktemp -d /mnt/docker-data/rustsimhub/p0/wifi-config-client-XXXXXXXX',remote=nas);client=raw.splitlines()[-1];assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/wifi-config-client-[A-Za-z0-9]{8}',client)
 helper='''import http.client,json,pathlib,ssl,sys
address=sys.argv[1];context=ssl.create_default_context(cafile=str(pathlib.Path(__file__).parent/'cert.pem'));c=http.client.HTTPSConnection(address,8448,context=context,timeout=15)
try:
 body=sys.stdin.read(65537);assert len(body)<=65536
 c.request('POST','/api/v2',body=body,headers={'Host':address+':8448','Origin':'https://'+address+':8448','Content-Type':'application/json','X-RustSimHub-Request':'2'})
 r=c.getresponse();data=r.read(65537);assert r.status==200 and len(data)<=65536;print(data.decode())
finally:c.close()
'''
 (local/'request.py').write_text(helper,encoding='utf-8')
 with nas.open_sftp() as ns:
  for name in ['cert.pem','request.py']:ns.put(str(local/name),client+'/'+name);ns.chmod(client+'/'+name,0o600)
 started=True;core_start('--serve-local-setup')
 args=['systemd-run','--unit='+units[1],'--collect','-p','User=rustsimhub-edge','-p','Group=rustsimhub-edge','-p','SupplementaryGroups=rustsimhub-ipc','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','InaccessiblePaths=/run/dbus','-p','BindReadOnlyPaths='+base+'/edge '+base+'/runtime '+base+'/bin','-p','RuntimeMaxSec=890','-p','TimeoutStopSec=6','-p','MemoryMax=64M','-p','TasksMax=8',edge,'--serve',base+'/edge/config.json'];run(shlex.join(args));time.sleep(.4)
 pin=call({'op':'identity'})['public_key'];owner=Ed25519PrivateKey.generate();c=call({'op':'setup_challenge','setup_code':setup.hex(),'public_key':pub(owner)});assert call({'op':'setup_finish','setup_code':setup.hex(),'challenge_id':c['id'],'signature':sign(c,owner,'bootstrap','owner',1023)})['owner_initialized'];del setup
 token=login(owner);assert begin(token,'disabled-mode',ssid,secret).get('error')
 run('systemctl stop '+q(units[0]));core_start('--serve-network-provisioning');token=login(owner);assert call({'op':'health','token':token})['wifi_provisioning_api']
 if a.platform:
  health=call({'op':'health','token':token});assert health['mode']=='authenticated_platform' and all(health[x] for x in ['messaging_api','call_control_api','history_read_api','legacy_read_api','wifi_provisioning_api'])
  sources=call({'op':'history_sources','token':token});assert len(sources['sources'])==1
  original_inbox=wait_inbox(token)
 reader=Ed25519PrivateKey.generate();call({'op':'open_pairing','token':token});c=call({'op':'pair_challenge','public_key':pub(reader),'kind':'client','scopes':['status.read']});pending=call({'op':'pair_proof','challenge_id':c['id'],'signature':sign(c,reader,'pair','client',1)});call({'op':'approve','token':token,'pairing_id':pending['pairing_id'],'public_key':pub(reader),'kind':'client','scopes':['status.read']});readonly=login(reader,'client',1)
 assert begin(readonly,'readonly-denied',ssid,secret).get('error');assert not run('find '+q(base+'/journal')+' -maxdepth 1 -name "*.started"')[1].strip()
 if a.nas_adapter:
  assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/nas-adapter-first/out/rustsimhub-nas-adapter',a.nas_adapter)
  _,raw=run('sha256sum '+q(a.nas_adapter),remote=nas);assert raw.splitlines()[-1].split()[0]=='64c965eda12c91b089a4618b44950455c7c7be5cb6993711b8ba0723deeaf386'
  nk=Ed25519PrivateKey.generate();scopes=['status.read','sms.read','calls.read'];call({'op':'open_pairing','token':token})
  c=call({'op':'pair_challenge','public_key':pub(nk),'kind':'nas','scopes':scopes});pending=call({'op':'pair_proof','challenge_id':c['id'],'signature':sign(c,nk,'pair','nas',11)})
  approved=call({'op':'approve','token':token,'pairing_id':pending['pairing_id'],'public_key':pub(nk),'kind':'nas','scopes':scopes})
  binding=call({'op':'bind_legacy','token':token,'core_id':'synthetic-platform-nas','public_key':pub(nk),'expected_generation':approved['generation'],'scopes':scopes})
  cfg={'url':'https://'+dc['host']+':8448','ca_file':client+'/cert.pem','device_public_key':bytes(pin).hex(),'core_id':'synthetic-platform-nas','binding_epoch':binding['binding_epoch'],'key_file':client+'/nas-seed'}
  with nas.open_sftp() as ns:
   for name,data in [('nas-seed',nk.private_bytes(Encoding.Raw,PrivateFormat.Raw,NoEncryption())),('adapter.json',json.dumps(cfg).encode())]:
    with ns.open(client+'/'+name,'wx') as f:f.write(data)
    ns.chmod(client+'/'+name,0o600)
  go_token=go('session')['token']
 # Install only this test's rule; cleanup checks its exact bytes before removing it.
 rule=ROOT/'firmware/development/wifi.rules';policy_hash=hashlib.sha256(rule.read_bytes()).hexdigest();sf.put(str(rule),base+'/wifi.rules');sf.chmod(base+'/wifi.rules',0o600);policy='/etc/polkit-1/rules.d/49-rustsimhub-wifi-config-'+tag+'.rules'
 run('test ! -e '+q(policy)+';install -o root -g root -m 0644 '+q(base+'/wifi.rules')+' '+q(policy));time.sleep(.5)
 first=begin(token,'correct-cancel-once',ssid,secret);trial=first['trial_id'];ready=wait_state(token,trial,{'ready'})
 if a.platform:
  assert call({'op':'inbox','token':token,'after':None,'limit':16})==original_inbox
  assert call({'op':'history_page','token':token,'source':'a'*64,'entity':'sms','after':None,'limit':16})['records']
  assert call({'op':'call_status','token':token})['control_api'] is True
  assert call({'op':'prepare_call','token':token,'line_epoch':1,'idempotency_key':'network-denied-dial','action':'dial','call_id':None,'payload':'10010'}).get('error')
  report['network_excludes_new_call_intent_but_inbox_history_and_status_remain_available']=True
  if a.nas_adapter:
   bridged=go('read',{'token':go_token,'query':{'entity':'sms','after':None,'limit':16,'record_id':None}})
   assert bridged['data_format']==2 and bridged['data']==original_inbox;report['nas_go_inbox_keeps_working_during_wifi_trial']=True
 assert state(readonly,trial).get('error');assert begin(token,'correct-cancel-once',ssid,secret)['trial_id']==trial
 assert begin(token,'correct-cancel-once',ssid,'0'*64).get('error');assert begin(token,'parallel-denied',ssid,secret).get('error')
 candidate=trial_uuid(trial);assert candidate in profiles();assert call({'op':'wifi_cancel','token':token,'trial_id':trial})['state']=='rolled_back';restore_wait();assert set(profiles())==set(before)
 report['fresh_profile_ready_cancel_and_input_idempotency']=True;save()
 # Independent once key, explicit SIGKILL while its finite NM checkpoint exists.
 wrong=begin(token,'wrong-psk-kill-once',ssid,'0'*64);trial=wrong['trial_id'];deadline=time.monotonic()+8
 while True:
  _,raw=run('busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints')
  if raw.strip()!='ao 0' and trial_uuid(trial) in profiles():break
  assert time.monotonic()<deadline;time.sleep(.1)
 report['fault_kill_armed']=True;save();run('systemctl kill --kill-whom=all --signal=KILL '+q(units[0]));restore_wait();assert set(profiles())==set(before)
 core_start('--serve-network-provisioning');token=login(owner);assert begin(token,'wrong-psk-kill-once',ssid,'0'*64).get('error')
 if a.platform:
  assert wait_inbox(token)==original_inbox
  if a.nas_adapter:
   go_token=go('session')['token'];assert go('read',{'token':go_token,'query':{'entity':'sms','after':None,'limit':16,'record_id':None}})['data']==original_inbox
   report['shared_core_inbox_and_nas_mapping_survive_process_kill']=True
 assert begin(token,'correct-cancel-once',ssid,secret).get('error');report['real_process_kill_rollback_and_no_restart_replay']=True;save()
 final=begin(token,'correct-confirm-once',ssid,secret);trial=final['trial_id'];wait_state(token,trial,{'ready'});candidate=trial_uuid(trial)
 confirmed=call({'op':'wifi_confirm','token':token,'trial_id':trial});assert confirmed['state']=='saved'
 _,raw=run('nmcli -g connection.autoconnect,connection.uuid connection show uuid '+q(candidate));assert raw.splitlines()==['yes',candidate]
 assert call({'op':'wifi_confirm','token':token,'trial_id':trial})['state']=='saved'
 report['explicit_confirmation_persists_own_autoconnect_profile']=True;save()
 run('systemctl stop '+q(units[0]));deadline=time.monotonic()+25
 while True:
  _,raw=run('nmcli -g GENERAL.CON-UUID,GENERAL.STATE device show wlan0');lines=raw.splitlines()
  if len(lines)==2 and lines[0]==candidate and lines[1].startswith('100'):break
  assert time.monotonic()<deadline;time.sleep(.5)
 assert candidate in profiles();core_start('--serve-network-provisioning');token=login(owner);assert state(token,trial).get('error');assert begin(token,'correct-confirm-once',ssid,secret).get('error')
 report['confirmed_profile_survives_core_restart']=True
 for unit,uid in zip(units,[coreuid,edgeuid]):
  _,raw=run('pid=$(systemctl show -p MainPID --value '+q(unit)+');test "$pid" -gt 0;awk \'/^(Uid|CapEff|CapPrm|CapAmb|CapBnd):/{print}\' /proc/"$pid"/status');assert re.search(r'Uid:\s+'+str(uid)+r'\s+'+str(uid),raw)
  for field in ['CapEff','CapPrm','CapAmb','CapBnd']:assert re.search(field+r':\s+0+\b',raw)
 report['actual_nonroot_caps_verified']=True;report['passed']=True
finally:
 if dev and started:
  run('systemctl stop '+shlex.join(list(reversed(units))),False)
  for trial in set(trials):
   identifier=trial_uuid(trial)
   if identifier in profiles():run('nmcli connection delete uuid '+q(identifier))
  if original and not original_ready():run('nmcli connection up uuid '+q(original)+' ifname wlan0')
  if original:restore_wait()
  if policy and run('test -e '+q(policy),False)[0]==0:
   run('test "$(stat -c %u '+q(policy)+')" = 0;test "$(stat -c %a '+q(policy)+')" = 644;test "$(stat -c %h '+q(policy)+')" = 1;test ! -L '+q(policy)+';echo '+q(policy_hash+'  '+policy)+' | sha256sum -c -;rm -- '+q(policy));report['temporary_policy_removed']=True
  rc,raw=run('systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs;for f in /proc/asound/card*/pcm*/sub*/status;do test "$(sed -n \'1p\' "$f")" = closed;done;busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Checkpoints;busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager WwanEnabled',False)
  report['cleanup_verified']=rc==0 and raw.splitlines()[:4]==['active']*4 and raw.splitlines()[4:] == ['ao 0',wwan.strip()] and set(profiles())==set(before) and original_ready()
 save()
 for item in [sf,dev,nas]:
  if item:item.close()
if report['passed'] and report.get('cleanup_verified'):print('Production Wi-Fi provisioning passed: fresh profile, explicit save, private credentials, rollback/restart fences and cleanup; no cellular/calls/SMS')
else:raise SystemExit(1)
