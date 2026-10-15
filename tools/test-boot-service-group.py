#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real renamed boot units in candidate userspace; tmpfs ledger, no boot installation."""
import argparse,hashlib,json,pathlib,re,shlex,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey,Ed25519PublicKey
from cryptography.hazmat.primitives.serialization import Encoding,PublicFormat,PrivateFormat,NoEncryption

p=argparse.ArgumentParser(description=__doc__)
for name in ('nas','device','application','artifacts','updater-artifacts','report'):p.add_argument('--'+name,type=pathlib.Path,required=True)
p.add_argument('--root-directory',required=True)
a=p.parse_args();assert not a.report.exists()
assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs',a.root_directory)
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
app=json.loads((a.artifacts/'build.json').read_text());tool=json.loads((a.updater_artifacts/'build.json').read_text())
assert app['development_fixtures'] is False and tool.get('boot_runtime_selection_supported') is True
tag=uuid.uuid4().hex[:10];prefix='rustsimhub-boot-'+tag
original=['rustsimhub-app.target','rustsimhub-app-select.service','rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-app-recover.service']
units={name:prefix+'-'+name.removeprefix('rustsimhub-') for name in original}
report={'passed':False,'new_calls':0,'new_sms':0,'cellular_tests':False,'boot_units_installed':False,'new_os_boot_verified':False,'development_signer':True,'units':units,'application_source':app['source_archive_sha256'],'immutable_updater_source':tool['source_archive_sha256']}
nas=dev=sf=None;base=None;mounted=False;policy=None;unit_files={};runtime='/run/'+prefix
q=shlex.quote
def save():a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c['known_hosts']);s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(body,remote=None,check=True):
 i,o,e=(remote or dev).exec_command('sh -s',timeout=75);o.channel.set_combine_stderr(True);i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc and check:report['failed_output']=raw;save();raise RuntimeError('Boot group command failed; inspect private report, no automatic retry')
 return rc,raw
def put(path,data,mode=0o600,uid=0,gid=0):
 with sf.open(path,'wx') as f:f.write(data)
 sf.chmod(path,mode);sf.chown(path,uid,gid)
def stateunit(name):return run('systemctl is-active '+q(units[name]),check=False)[1].strip()
def wait_group(expected):
 deadline=time.monotonic()+35
 while time.monotonic()<deadline:
  if all(stateunit(n)=='active' for n in original[:4]):
   target=run('readlink '+q(runtime+'/selected'))[1].strip()
   if target==base+'/versions/'+expected:return
  time.sleep(.3)
 report['unit_status']=run('systemctl status --no-pager '+shlex.join(units.values()),check=False)[1];save();raise RuntimeError('Boot group readiness timed out')
def call(command):
 identifier=uuid.uuid4().hex;body=json.dumps({'protocol':2,'id':identifier,'command':command},separators=(',',':'))
 raw=run('python3 '+q(client+'/request.py')+' '+q(dc['host'])+" <<'RSH_BOOT_REQUEST_END'\n"+body+'\nRSH_BOOT_REQUEST_END',remote=nas)[1]
 response=json.loads(raw.splitlines()[-1]);assert response['protocol']==2 and response['id']==identifier;return response['result']
def public(key):return list(key.public_key().public_bytes(Encoding.Raw,PublicFormat.Raw))
def proof(challenge,purpose):
 raw=bytes(challenge['transcript']);Ed25519PublicKey.from_public_bytes(bytes(pin)).verify(bytes(challenge['device_signature']),raw);value=json.loads(raw)
 assert value['domain']=='RustSimHub.Auth.v1' and value['purpose']==purpose and value['kind']=='owner' and value['scopes']==1023
 assert value['client_public_key']==public(owner) and value['device_public_key']==pin and value['challenge_id']==challenge['id']
 return list(owner.sign(raw))
def health():
 identity=call({'op':'identity'});assert identity['public_key']==pin and identity['owner_setup_available'] is False
 challenge=call({'op':'challenge','public_key':public(owner)})
 token=call({'op':'authenticate','challenge_id':challenge['id'],'signature':proof(challenge,'login')})['token']
 status=call({'op':'health','token':token});assert status['mode']=='authenticated_platform' and status['media_available'] is False
 assert all(status[n] for n in ['messaging_api','call_control_api','history_read_api','legacy_read_api','wifi_provisioning_api'])
 deadline=time.monotonic()+10
 while True:
  inbox=call({'op':'inbox','token':token,'after':None,'limit':16})
  assert not inbox.get('error')
  if inbox.get('messages') or time.monotonic()>=deadline:break
  time.sleep(.3)
 return inbox
def updater(command,*extra):
 args=['systemd-run','--quiet','--wait','--pipe','--collect','--unit='+prefix+'-update-'+uuid.uuid4().hex[:6]]
 for prop in ['RootDirectory='+a.root_directory,'MountAPIVFS=yes','TemporaryFileSystem=/var/tmp:mode=1777 /run:mode=0755','PrivateDevices=yes','PrivateNetwork=yes','ProtectSystem=strict','NoNewPrivileges=yes','CapabilityBoundingSet=','AmbientCapabilities=','BindPaths='+base+'/ledger '+base+'/versions '+base+'/versions:'+base+'/ledger/versions','BindReadOnlyPaths='+base+'/bin '+base+'/package1 '+base+'/package2 '+base+'/package3 '+base+'/release.public','RuntimeMaxSec=45']:args+=['-p',prop]
 return run(shlex.join(args+[base+'/bin/rustsimhub-update',command,base+'/ledger',base+'/release.public',*extra]))[1]
def verify_processes(hash_value):
 for role,uid in [('core',986),('edge',985)]:
  pid=run('systemctl show --value -p MainPID '+q(units['rustsimhub-'+role+'.service']))[1].strip();assert pid.isdecimal() and int(pid)>0
  source=base+'/versions/'+hash_value+'/rustsimhub-'+role
  script='\n'.join(['test "$(stat -Lc %d:%i /proc/'+pid+'/root)" = "$(stat -Lc %d:%i '+q(a.root_directory)+')"',
   'test "$(stat -Lc %d:%i /proc/'+pid+'/exe)" = "$(stat -Lc %d:%i '+q(source)+')"',
   'test "$(awk \'/^Uid:/{print $2}\' /proc/'+pid+'/status)" = '+str(uid),
   'for cap in CapEff CapPrm CapAmb CapBnd;do test "$(awk -v c="$cap:" \'$1==c{print $2}\' /proc/'+pid+'/status)" = 0000000000000000;done'])
  run(script)
 report['candidate_root_signed_slot_inodes_and_nonroot_empty_caps_verified']=True
try:
 nas=connect(nc);dev=connect(dc,nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12));sf=dev.open_sftp()
 base=run('test "$(df -B1 --output=avail /var/tmp | tail -1)" -gt 180000000\nd=$(mktemp -d /var/tmp/rustsimhub-boot-group-XXXXXXXX);chmod 0711 "$d";mkdir -m 0755 "$d/bin" "$d/helpers";mkdir -m 0700 "$d/ledger" "$d/versions";echo "$d"')[1].strip()
 assert re.fullmatch('/var/tmp/rustsimhub-boot-group-[A-Za-z0-9]{8}',base);report['directory']=base;save()
 run('mount -t tmpfs -o size=1m,mode=0700,nosuid,nodev,noexec tmpfs '+q(base+'/ledger'));mounted=True
 run('test "$(stat -c %d '+q(base+'/ledger')+')" != "$(stat -c %d '+q(base+'/versions')+')"\nmkdir -m 0700 '+q(base+'/ledger/versions')+'\ninstall -d -o 986 -g 986 -m 0700 '+q(base+'/core')+'\nfor n in state identity setup journal archive access compat;do install -d -o 986 -g 986 -m 0700 '+q(base+'/core')+'/$n;done\ninstall -d -o 985 -g 985 -m 0700 '+q(base+'/edge')+'\ninstall -d -o 986 -g 987 -m 2750 '+q(base+'/runtime'))
 report['ledger_and_payloads_are_distinct_actual_filesystems']=True
 for epoch in (1,2,3):
  directory=a.application/f'package{epoch}';remote=base+'/package'+str(epoch);run('mkdir -m 0755 '+q(remote))
  for path in directory.iterdir():
   assert path.is_file() and path.name in ['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa','Opus-COPYING.txt','manifest.json','manifest.sig']
   data=path.read_bytes();put(remote+'/'+path.name,data,0o555 if path.name.startswith('rustsimhub-') or path.name=='simhubctl' else 0o444)
   run('echo '+q(hashlib.sha256(data).hexdigest()+'  '+remote+'/'+path.name)+' | sha256sum -c -')
 put(base+'/release.public',(a.application/'release.public').read_bytes(),0o444)
 hashes={e:hashlib.sha256((a.application/f'package{e}'/'manifest.json').read_bytes()).hexdigest() for e in (1,2,3)}
 for name,origin in [('rustsimhub-update',a.updater_artifacts),('rustsimhub-migrate',a.artifacts)]:
  data=(origin/name).read_bytes();expected=(tool if name=='rustsimhub-update' else app)['artifact_sha256'][name];assert hashlib.sha256(data).hexdigest()==expected
  put(base+'/bin/'+name,data,0o555);info=json.loads(run(q(base+'/bin/'+name)+' --build-info')[1].splitlines()[-1]);assert info['source_archive_sha256']==(tool if name=='rustsimhub-update' else app)['source_archive_sha256']
 overlay=ROOT/'firmware/system/overlay'
 replacements={'/var/lib/rustsimhub/rustsimhub-v1/system/app':base+'/ledger','/var/lib/rustsimhub/rustsimhub-v1/core':base+'/core','/var/lib/rustsimhub/rustsimhub-v1/edge':base+'/edge','/var/lib/rustsimhub-core':'/mnt/rustsimhub-core','/var/lib/rustsimhub-edge':'/mnt/rustsimhub-edge','/usr/libexec/rustsimhub/':base+'/bin/','/usr/local/libexec/':base+'/helpers/','/usr/share/rustsimhub/release.public':base+'/release.public','/usr/share/rustsimhub/factory-app':base+'/package1','/opt/rustsimhub/versions':base+'/versions','/opt/rustsimhub/selected':'/mnt/selected','/run/rustsimhub/core':base+'/runtime','/run/rustsimhub-app':runtime}
 def rewrite(text):
  for old,new in replacements.items():text=text.replace(old,new)
  for old,new in units.items():text=text.replace(old,new)
  return text
 for name in ['rustsimhub-core-prepare','rustsimhub-edge-ready','rustsimhub-app-recover']:
  put(base+'/helpers/'+name,rewrite((overlay/'usr/local/libexec'/name).read_text()).encode(),0o555)
 for name in original:
  text=rewrite((overlay/'etc/systemd/system'/name).read_text())
  text=text.replace('Requires=rustsimhub-firstboot.service\n','').replace('After=rustsimhub-firstboot.service systemd-tmpfiles-setup.service\n','')
  text=text.replace('RuntimeDirectory=rustsimhub-app','RuntimeDirectory='+prefix)
  if name.endswith('.service'):
   text+='\nBindReadOnlyPaths='+base+'/helpers\n'
   if name!='rustsimhub-app-recover.service':
    text+='RootDirectory='+a.root_directory+'\nMountAPIVFS=yes\nTemporaryFileSystem=/var/tmp:mode=1777 /run:mode=0755 /mnt:mode=0755\nBindReadOnlyPaths='+base+'/bin '+base+'/package1 '+base+'/release.public\n'
    if name=='rustsimhub-app-select.service':text+='BindPaths='+base+'/ledger '+base+'/versions\n'
    if name=='rustsimhub-core.service':text+='BindReadOnlyPaths=/run/dbus\n'
  path='/run/systemd/system/'+units[name];assert run('test ! -e '+q(path),check=False)[0]==0
  put(path,text.encode(),0o644);unit_files[path]=hashlib.sha256(text.encode()).hexdigest()
 policy='/etc/polkit-1/rules.d/45-'+prefix+'.rules';policy_bytes=(overlay/'etc/polkit-1/rules.d/45-rustsimhub.rules').read_bytes();put(policy,policy_bytes,0o644)
 run('systemctl daemon-reload')
 prepared=json.loads(run('runuser -u rustsimhub-core -- '+shlex.join([base+'/package1/rustsimhub-core','--prepare-device-setup',base+'/core/state',base+'/core/identity',base+'/core/setup']))[1].splitlines()[-1]);pin=list(bytes.fromhex(prepared['device_public_key_hex']))
 with sf.open(base+'/core/setup/setup.code','rb') as file:code=file.read();assert len(code)==32
 run('runuser -u rustsimhub-edge -- '+shlex.join([base+'/package1/rustsimhub-edge','--init-tls',base+'/edge/tls',dc['host']]))
 put(base+'/edge/config.json',json.dumps({'listen':dc['host']+':8448','origin':'https://'+dc['host']+':8448','core_socket':base+'/runtime/core.sock','core_uid':986,'tls_directory':'/mnt/rustsimhub-edge/tls','legacy_compat':True}).encode(),uid=985,gid=985)
 local=ROOT/'.private'/('boot-group-'+tag);local.mkdir(mode=0o700);sf.get(base+'/edge/tls/cert.pem',str(local/'cert.pem'))
 owner=Ed25519PrivateKey.generate();(local/'owner.seed').write_bytes(owner.private_bytes(Encoding.Raw,PrivateFormat.Raw,NoEncryption()))
 client=run('mktemp -d /mnt/docker-data/rustsimhub/p0/boot-group-client-XXXXXXXX',remote=nas)[1].splitlines()[-1]
 assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/boot-group-client-[A-Za-z0-9]{8}',client)
 report['client_directory']=client;report['owner_directory']=str(local);save()
 helper='''import http.client,json,pathlib,ssl,sys
address=sys.argv[1];c=http.client.HTTPSConnection(address,8448,context=ssl.create_default_context(cafile=str(pathlib.Path(__file__).parent/'cert.pem')),timeout=15)
try:
 body=sys.stdin.read(65537);assert len(body)<=65536
 c.request('POST','/api/v2',body=body,headers={'Host':address+':8448','Origin':'https://'+address+':8448','Content-Type':'application/json','X-RustSimHub-Request':'2'})
 r=c.getresponse();data=r.read(65537);assert r.status==200 and len(data)<=65536;print(data.decode())
finally:c.close()
'''
 (local/'request.py').write_text(helper,encoding='utf-8')
 with nas.open_sftp() as ns:
  for name in ['cert.pem','request.py']:ns.put(str(local/name),client+'/'+name);ns.chmod(client+'/'+name,0o600)
 report['client_directory']=client;report['owner_directory']=str(local);save()
 run('systemctl start '+q(units['rustsimhub-app.target']));wait_group(hashes[1]);verify_processes(hashes[1])
 assert call({'op':'identity'})['public_key']==pin
 challenge=call({'op':'setup_challenge','setup_code':code.hex(),'public_key':public(owner)})
 assert call({'op':'setup_finish','setup_code':code.hex(),'challenge_id':challenge['id'],'signature':proof(challenge,'bootstrap')})['owner_initialized']
 baseline=health();report['factory_group_full_core_owner_and_strict_tls_passed']=True;save()
 run('systemctl stop '+q(units['rustsimhub-app.target']));updater('stage',base+'/package2')
 run('systemctl start '+q(units['rustsimhub-app.target']));wait_group(hashes[2]);verify_processes(hashes[2]);assert health()==baseline
 # One actual SIGKILL, recorded before dispatch. No business operation is submitted.
 report['candidate_kill_armed']=True;save();run('systemctl kill --kill-whom=main --signal=KILL '+q(units['rustsimhub-core.service']))
 wait_group(hashes[1]);verify_processes(hashes[1]);assert health()==baseline
 assert json.loads(updater('status').splitlines()[-1])['floor_epoch']==2
 report['actual_core_kill_triggered_whole_group_automatic_fallback']=True;save()
 run('systemctl stop '+q(units['rustsimhub-app.target']));updater('stage',base+'/package3')
 run('systemctl start '+q(units['rustsimhub-app.target']));wait_group(hashes[3]);assert health()==baseline
 updater('confirm',hashes[3]);run('systemctl stop '+q(units['rustsimhub-app.target'])+'\nsystemctl start '+q(units['rustsimhub-app.target']));wait_group(hashes[3]);assert health()==baseline
 report['explicit_authenticated_health_confirm_and_restart_kept_third_version']=True;save()
 run('systemctl kill --kill-whom=main --signal=KILL '+q(units['rustsimhub-edge.service']))
 deadline=time.monotonic()+20
 while time.monotonic()<deadline and any(stateunit(n)=='active' for n in original[:4]):time.sleep(.3)
 assert not any(stateunit(n)=='active' for n in original[:4]);time.sleep(.6)
 assert not any(stateunit(n)=='active' for n in original[:4])
 assert run('systemctl show --value -p ExecMainStatus '+q(units['rustsimhub-app-recover.service']))[1].strip()!='0'
 report['second_failure_does_not_loop_or_reuse_boot_recovery_budget']=True;report['passed']=True
 print('Real candidate boot units: signed non-root full Core/Edge, actual crash automatic group fallback, health confirmation and bounded recovery passed')
finally:
 if dev and base:
  try:
   run('systemctl stop '+shlex.join(units.values()),check=False)
   assert all(stateunit(n)!='active' for n in original)
   if mounted:
    run('test -z "$(find '+q(base+'/ledger/versions')+' -mindepth 1 -print -quit)"\ncp -a '+q(base+'/ledger')+' '+q(base+'/ledger-evidence')+'\numount '+q(base+'/ledger')+'\n! mountpoint -q '+q(base+'/ledger'));mounted=False
    report['no_payload_was_written_into_ledger_filesystem']=True
   for path,digest in unit_files.items():
    run('echo '+q(digest+'  '+path)+' | sha256sum -c -');sf.remove(path)
   if policy:
    run('echo '+q(hashlib.sha256(policy_bytes).hexdigest()+'  '+policy)+' | sha256sum -c -');sf.remove(policy)
   run('systemctl daemon-reload\nsystemctl reset-failed '+shlex.join(units.values())+' 2>/dev/null || true')
   old=run('systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$f")" = closed;done')[1];assert old.splitlines()==['active']*4
   report['cleanup_verified']=True
  except Exception as error:report['cleanup_verified']=False;report['cleanup_error']=str(error);report['passed']=False
 save()
 if sf:sf.close()
 if dev:dev.close()
 if nas:nas.close()
if not report['passed'] or not report.get('cleanup_verified'):raise SystemExit(1)
