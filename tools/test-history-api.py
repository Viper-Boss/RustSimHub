#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Synthetic archive acceptance through real isolated production Core and strict HTTPS."""
import argparse,contextlib,hashlib,ipaddress,json,pathlib,re,secrets,shlex,sqlite3,ssl,sys,time,uuid
from http.client import HTTPSConnection
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey,Ed25519PublicKey
from cryptography.hazmat.primitives.serialization import Encoding,PublicFormat
p=argparse.ArgumentParser(description=__doc__)
for name in ['nas','device','artifacts','report']:p.add_argument('--'+name,type=pathlib.Path,required=True)
a=p.parse_args();assert not a.report.exists()
metadata=json.loads((a.artifacts/'build.json').read_text());nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
names=['rustsimhub-core','rustsimhub-edge','rustsimhub-migrate']
for name in names:assert hashlib.sha256((a.artifacts/name).read_bytes()).hexdigest()==metadata['artifact_sha256'][name]
tag=uuid.uuid4().hex[:12];units=['rustsimhub-history-api-core-'+tag,'rustsimhub-history-api-edge-'+tag]
report={'source_archive_sha256':metadata['source_archive_sha256'],'units':units,'synthetic_only':True,'original_data_read':False,'hardware_access_disabled':True,'new_calls':0,'new_sms':0,'cellular_tests':False,'browser_verified':False,'passed':False}
q=shlex.quote;nas=dev=sf=None;started=False;context=None;address=None;pin=None;nas_client=None
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(body,check=True,remote=None):
 i,o,e=(remote or dev).exec_command('sh -s',timeout=90);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc and check:report['failure_output']=raw;save();raise RuntimeError('Private history API command failed')
 return rc,raw
def write(path,data,uid,gid):
 with sf.open(path,'wx') as f:f.write(data)
 sf.chmod(path,0o600);sf.chown(path,uid,gid)
def call(command,status=200):
 identifier=uuid.uuid4().hex;c=HTTPSConnection(address,8448,context=context,timeout=20)
 try:
  body=json.dumps({'protocol':2,'id':identifier,'command':command},separators=(',',':'))
  if nas_client:
   _,reply=run('python3 '+q(nas_client+'/client_request.py')+' '+q(address)+" <<'RSH_HTTP_REQUEST_END'\n"+body+'\nRSH_HTTP_REQUEST_END',remote=nas)
   envelope=json.loads(reply.splitlines()[-1]);raw=envelope['body'].encode();actual_status=envelope['status']
  else:
   c.request('POST','/api/v2',body=body,headers={'Host':address+':8448','Origin':'https://'+address+':8448','Content-Type':'application/json','X-RustSimHub-Request':'2'})
   r=c.getresponse();raw=r.read(65537);actual_status=r.status
  assert actual_status==status and len(raw)<=65536
  value=json.loads(raw)
  if status!=200:return value
  assert value['protocol']==2 and value['id']==identifier
  return value['result']
 except Exception as error:
  report['failure_op']=command.get('op');report['failure_kind']=type(error).__name__
  if dev and started:
   diagnostic='for u in '+shlex.join(units)+';do pid=$(systemctl show -p MainPID --value "$u");if test "$pid" -gt 0;then ps -L -p "$pid" -o pid,tid,pcpu,comm,wchan;awk \'/^(State|Uid|VmRSS|Threads):/{print}\' /proc/"$pid"/status;fi;done'
   _,report['live_diagnostic']=run(diagnostic,False)
   raw=json.dumps({'protocol':2,'id':'readonly-timeout-diagnosis','command':{'op':'identity'}})
   code,out=run('timeout 8 runuser -u rustsimhub-edge -- '+q(core)+' --request-auth '+q(socket)+" <<'RSH_DIAG_END'\n"+raw+'\nRSH_DIAG_END',False)
   report['readonly_ipc_diagnostic_exit']=code;report['readonly_ipc_diagnostic']=out
  save();raise
 finally:c.close()
def pub(key):return list(key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw))
def sign(challenge,key,purpose,kind,scopes):
 raw=bytes(challenge['transcript']);assert 0<len(raw)<=4096 and challenge['expires_in_seconds']==60
 Ed25519PublicKey.from_public_bytes(bytes(pin)).verify(bytes(challenge['device_signature']),raw)
 t=json.loads(raw);assert t['domain']=='RustSimHub.Auth.v1' and t['purpose']==purpose and t['kind']==kind and t['scopes']==scopes and t['client_public_key']==pub(key) and t['device_public_key']==pin and t['challenge_id']==challenge['id']
 assert re.fullmatch('[0-9a-f]{64}',t['instance']) and re.fullmatch('[0-9a-f]{64}',t['challenge_id'])
 return list(key.sign(raw))
def login(key,kind,scopes):
 c=call({'op':'challenge','public_key':pub(key)})
 return call({'op':'authenticate','challenge_id':c['id'],'signature':sign(c,key,'login',kind,scopes)})['token']
def start_core(mode):
 args=[core,mode,socket,str(edgeuid),base+'/state',base+'/identity']
 if mode=='--serve-local-setup':args.append(base+'/setup/code')
 else:args += [base+'/runtime/media.sock',str(mediauid),base+'/archive',base+'/access']
 command=['systemd-run','--unit='+units[0],'--collect','-p','User=rustsimhub-core','-p','Group=rustsimhub-core','-p','SupplementaryGroups=rustsimhub-ipc','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','InaccessiblePaths=/run/dbus','-p','BindPaths='+' '.join(base+'/'+name for name in ['state','identity','runtime','access']),'-p','BindReadOnlyPaths='+base+'/setup '+base+'/archive '+base+'/bin','-p','RuntimeMaxSec=900','-p','TimeoutStopSec=6','-p','MemoryMax=96M','-p','TasksMax=16','-p','LimitCORE=0']+args
 run(shlex.join(command));run('n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 20;sleep 1;done')
def grant(token,target,user,source='a'*64,entities=None,generation=None):
 return call({'op':'grant_history','token':token,'public_key':target['public_key'],'expected_generation':target['generation'] if generation is None else generation,'source':source,'legacy_user':user,'entities':entities or ['sms','calls','contacts']})
def pair(owner,key,kind):
 scopes=['status.read','sms.read','calls.read'];call({'op':'open_pairing','token':owner});c=call({'op':'pair_challenge','public_key':pub(key),'kind':kind,'scopes':scopes})
 pending=call({'op':'pair_proof','challenge_id':c['id'],'signature':sign(c,key,'pair',kind,11)})
 current=call({'op':'approve','token':owner,'pairing_id':pending['pairing_id'],'public_key':pub(key),'kind':kind,'scopes':scopes})
 return current,login(key,kind,11)
def page(token,entity,source='a'*64,after=None,limit=16):return call({'op':'history_page','token':token,'source':source,'entity':entity,'after':after,'limit':limit})
def detail(token,entity,key):return call({'op':'history_detail','token':token,'source':'a'*64,'entity':entity,'record_id':ids[(entity,key)]})
try:
 nas=connect(nc);channel=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,channel);sf=dev.open_sftp()
 _,raw=run('ip -4 -o address show dev wlan0');address=re.search(r'\binet ([0-9.]+)/',raw)[1];assert ipaddress.ip_address(address).is_private
 _,raw=run('d=$(mktemp -d /var/tmp/rustsimhub-history-api-XXXXXXXX);chmod 0711 "$d";install -d -m 0755 "$d/bin";for n in state identity setup archive access input;do install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$d/$n";done;install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";install -d -o rustsimhub-edge -g rustsimhub-edge -m 0700 "$d/edge";printf "BASE=%s\\n" "$d";id -u rustsimhub-core;id -g rustsimhub-core;id -u rustsimhub-edge;id -g rustsimhub-edge;id -u rustsimhub-media')
 match=re.search(r'BASE=(/var/tmp/rustsimhub-history-api-[A-Za-z0-9]{8})\n(\d+)\n(\d+)\n(\d+)\n(\d+)\n(\d+)',raw);assert match;base=match[1];coreuid,coregid,edgeuid,edgegid,mediauid=map(int,match.groups()[1:]);report['device_directory']=base
 for name in names:
  sf.put(str(a.artifacts/name),base+'/bin/'+name);sf.chmod(base+'/bin/'+name,0o555)
  _,raw=run('echo '+q(metadata['artifact_sha256'][name]+'  '+base+'/bin/'+name)+' | sha256sum -c -\n'+q(base+'/bin/'+name)+' --build-info');info=json.loads(raw.splitlines()[-1]);assert info['source_archive_sha256']==metadata['source_archive_sha256']
  if name=='rustsimhub-core':assert info['development_fixtures'] is False
 core=base+'/bin/rustsimhub-core';edge=base+'/bin/rustsimhub-edge';socket=base+'/runtime/core.sock'
 secret=secrets.token_bytes(32);write(base+'/setup/code',secret,coreuid,coregid)
 local=ROOT/'.private'/('history-api-'+tag);local.mkdir(mode=0o700)
 snapshot=local/'input.sqlite'
 with contextlib.closing(sqlite3.connect(snapshot)) as db:
  db.executescript((ROOT/'crates/compat/src/history/test-schema.sql').read_text(encoding='utf-8'))
  db.executescript("INSERT INTO modemdeck_lines(line_id,line_label) VALUES('private_b','Private B');INSERT INTO modemdeck_user_lines VALUES('legacy_user_b','private_b',NULL);INSERT INTO sms(id,line_id,peer,content,type) VALUES(55,'private_b','+12025550999','only user b',1);INSERT INTO contacts(id,owner_user_id,display_name) VALUES('contact_b','legacy_user_b','private contact b');")
 sf.put(str(snapshot),base+'/input/snapshot.sqlite');sf.chown(base+'/input/snapshot.sqlite',coreuid,coregid);sf.chmod(base+'/input/snapshot.sqlite',0o400)
 for source in ['a'*64,'b'*64]:
  _,raw=run('runuser -u rustsimhub-core -- '+q(base+'/bin/rustsimhub-migrate')+' import '+q(base+'/input/snapshot.sqlite')+' '+source+' '+q(base+'/archive'));assert json.loads(raw.splitlines()[-1])['inserted']==19
 sf.get(base+'/archive/history.sqlite',str(local/'archive.sqlite'))
 with contextlib.closing(sqlite3.connect(local/'archive.sqlite')) as db:
  ids={({'call_history':'calls'}.get(entity,entity),key):str(seq) for seq,entity,key in db.execute('SELECT seq,entity,legacy_key FROM history_records WHERE source=?',('a'*64,))}
 report['archive_original_sha256']=hashlib.sha256((local/'archive.sqlite').read_bytes()).hexdigest()
 run('runuser -u rustsimhub-edge -- '+q(edge)+' --init-tls '+q(base+'/edge/tls')+' '+address)
 config={'listen':address+':8448','origin':'https://'+address+':8448','core_socket':socket,'core_uid':coreuid,'tls_directory':base+'/edge/tls'};write(base+'/edge/config.json',json.dumps(config).encode(),edgeuid,edgegid)
 started=True;start_core('--serve-local-setup')
 command=['systemd-run','--unit='+units[1],'--collect','-p','User=rustsimhub-edge','-p','Group=rustsimhub-edge','-p','SupplementaryGroups=rustsimhub-ipc','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','InaccessiblePaths=/run/dbus','-p','BindReadOnlyPaths='+base+'/edge '+base+'/runtime '+base+'/bin','-p','RuntimeMaxSec=880','-p','TimeoutStopSec=5','-p','MemoryMax=64M','-p','TasksMax=8','-p','LimitCORE=0',edge,'--serve',base+'/edge/config.json'];run(shlex.join(command))
 cert=local/'cert.pem';sf.get(base+'/edge/tls/cert.pem',str(cert));context=ssl.create_default_context(cafile=str(cert));time.sleep(1)
 _,raw=run('mktemp -d /mnt/docker-data/rustsimhub/p0/history-api-client-XXXXXXXX',remote=nas);nas_client=raw.splitlines()[-1];assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/history-api-client-[A-Za-z0-9]{8}',nas_client)
 client_script=local/'client_request.py';client_script.write_text('''import http.client,json,pathlib,ssl,sys
address=sys.argv[1];base=pathlib.Path(__file__).parent
context=ssl.create_default_context(cafile=str(base/'cert.pem'))
connection=http.client.HTTPSConnection(address,8448,context=context,timeout=15)
try:
 body=sys.stdin.read(65537);assert len(body)<=65536
 connection.request('POST','/api/v2',body=body,headers={'Host':address+':8448','Origin':'https://'+address+':8448','Content-Type':'application/json','X-RustSimHub-Request':'2'})
 response=connection.getresponse();data=response.read(65537);assert len(data)<=65536
 print(json.dumps({'status':response.status,'body':data.decode('utf-8')}))
finally:connection.close()
''',encoding='utf-8')
 with nas.open_sftp() as ns:
  for name,path in [('cert.pem',cert),('client_request.py',client_script)]:ns.put(str(path),nas_client+'/'+name);ns.chmod(nas_client+'/'+name,0o600)
 report['https_client']='NAS';report['same_wifi_origin_and_port']=True
 pin=call({'op':'identity'})['public_key'];owner=Ed25519PrivateKey.generate();c=call({'op':'setup_challenge','setup_code':secret.hex(),'public_key':pub(owner)})
 assert call({'op':'setup_finish','setup_code':secret.hex(),'challenge_id':c['id'],'signature':sign(c,owner,'bootstrap','owner',1023)})['owner_initialized'];del secret
 old_owner=login(owner,'owner',1023);assert call({'op':'history_sources','token':old_owner}).get('error')
 run('systemctl stop '+q(units[0]));start_core('--serve-media-history')
 assert call({'op':'health','token':old_owner}).get('error');ot=login(owner,'owner',1023)
 assert call({'op':'health','token':ot})['history_read_api'] is True
 ak,bk,nk=[Ed25519PrivateKey.generate() for _ in range(3)];ap,at=pair(ot,ak,'client');bp,bt=pair(ot,bk,'client');np,nt=pair(ot,nk,'nas')
 assert page(at,'sms').get('error') and page(nt,'sms').get('error')
 assert grant(at,ap,'legacy_user_a').get('error') and grant(ot,ap,'legacy_user_a',generation=ap['generation']+1).get('error')
 assert grant(ot,ap,None).get('error')
 for target,user in [(ap,'legacy_user_a'),(bp,'legacy_user_b'),(np,None)]:assert grant(ot,target,user)['ok']
 assert len(call({'op':'history_sources','token':at})['sources'])==1
 assert len(call({'op':'history_sources','token':ot})['sources'])==2
 first=page(at,'sms',limit=1);second=page(at,'sms',after=first['cursor'],limit=1);assert first['records'][0]['id']!=second['records'][0]['id']
 assert page(bt,'sms',after=first['cursor']).get('error') and page(at,'sms',source='b'*64,after=first['cursor']).get('error')
 assert len(page(at,'sms')['records'])==3 and len(page(bt,'sms')['records'])==4 and len(page(nt,'sms')['records'])==4
 assert detail(at,'sms','[55]').get('error') and detail(bt,'sms','[55]')['content']=='only user b'
 aa=detail(at,'sms','[42]');bb=detail(bt,'sms','[42]');assert aa['user_state']['read'] and not bb['user_state']['read'];assert aa['user_state']['favorite'] and not bb['user_state']['favorite'];assert bb['user_state']['marked_unread']
 for field in ['imsi','iccid','line_id','endpoint_line_id','legacy_user']:assert field not in aa
 tomb=detail(at,'sms','[43]');assert tomb['deleted'] and not any(field in tomb for field in ['content','sender','recipient','peer'])
 assert detail(at,'calls','["call_synthetic"]')['user_state']['read'] and not detail(bt,'calls','["call_synthetic"]')['user_state']['read']
 assert detail(at,'contacts','["contact_synthetic"]')['phones'][0]['canonical_e164']=='+12025550123' and detail(bt,'contacts','["contact_synthetic"]').get('error')
 assert call({'op':'inbox','token':ot,'after':None,'limit':16})['messages']==[] and call({'op':'calls','token':ot,'after':None,'limit':16})['calls']==[]
 report['scoped_queries_and_no_business_replay']=True
 assert call({'op':'revoke_history','token':ot,'public_key':ap['public_key'],'source':'a'*64})['ok'];assert page(at,'sms',after=first['cursor']).get('error')
 assert grant(ot,ap,'legacy_user_a')['ok'];assert call({'op':'revoke','token':ot,'public_key':ap['public_key']})['ok'];assert page(at,'sms').get('error')
 newer,at2=pair(ot,ak,'client');assert newer['generation']>ap['generation'];assert page(at2,'sms').get('error');assert grant(ot,newer,'legacy_user_a',generation=ap['generation']).get('error');assert grant(ot,newer,'legacy_user_a')['ok']
 assert page(at2,'sms',after=first['cursor']).get('error');assert len(page(at2,'sms')['records'])==3
 report['revocation_and_reapproval_do_not_inherit_archive_access']=True
 run('systemctl stop '+q(units[0]));start_core('--serve-media-history')
 assert page(at2,'sms').get('error');new_token=login(ak,'client',11);assert len(page(new_token,'sms')['records'])==3
 assert report['archive_original_sha256'] in run('sha256sum '+q(base+'/archive/history.sqlite'))[1]
 report['restart_persistence_fresh_login_and_immutable_archive']=True
 for unit,expected_uid in zip(units,[coreuid,edgeuid]):
  _,raw=run('pid=$(systemctl show -p MainPID --value '+q(unit)+');test "$pid" -gt 0;awk \'/^(Uid|CapEff|CapPrm|CapAmb|CapBnd):/{print}\' /proc/"$pid"/status');assert re.search(r'Uid:\s+'+str(expected_uid)+r'\s+'+str(expected_uid),raw)
  for field in ['CapEff','CapPrm','CapAmb','CapBnd']:assert re.search(field+r':\s+0+\b',raw)
 report['actual_nonroot_caps_verified']=True;report['passed']=True
finally:
 if dev and started:
  rc,_=run('systemctl stop '+shlex.join(list(reversed(units))),False)
  rc2,raw=run('for u in '+shlex.join(units)+';do if systemctl is-active --quiet "$u";then exit 10;fi;done;systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs;for f in /proc/asound/card*/pcm*/sub*/status;do test "$(sed -n \'1p\' "$f")" = closed;done',False)
  report['cleanup_verified']=rc==0 and rc2==0 and raw.splitlines()==['active']*4
 save()
 for item in [sf,dev,nas]:
  if item:item.close()
if report['passed'] and report.get('cleanup_verified'):print('Production Core + strict HTTPS history acceptance passed: user/source isolation, tombstones, explicit delegation, revoke/generation/restart, no hardware operations')
else:raise SystemExit(1)
