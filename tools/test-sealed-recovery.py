#!/usr/bin/env python3
"""Recipient key stays NAS-side; encrypted identity is restored into fresh private directories."""
import argparse,hashlib,json,pathlib,re,shlex,sys,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nas',type=pathlib.Path,required=True);p.add_argument('--device',type=pathlib.Path,required=True);p.add_argument('--firstboot-report',type=pathlib.Path,required=True);p.add_argument('--binary',type=pathlib.Path,required=True);p.add_argument('--ctl',type=pathlib.Path,required=True);p.add_argument('--artifacts',type=pathlib.Path,required=True);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
ncfg=json.loads(args.nas.read_text(encoding='utf-8-sig'));dcfg=json.loads(args.device.read_text(encoding='utf-8-sig'));original=json.loads(args.firstboot_report.read_text(encoding='utf-8-sig'));source=original['device_directory'];pin=original['device_pin'];owner_dir=original['nas_owner_directory']
assert re.fullmatch(r'/var/tmp/rustsimhub-firstboot-private-[A-Za-z0-9]{8}',source);assert re.fullmatch(r'/mnt/docker-data/rustsimhub/p0/firstboot-client-private-[A-Za-z0-9]{8}',owner_dir);assert re.fullmatch('[a-f0-9]{64}',pin)
artifacts=json.loads(args.artifacts.read_text(encoding='utf-8'));SHA=artifacts['rustsimhub-recovery_sha256'];SOURCE=artifacts['source_archive_sha256'];directory=artifacts['device_directory']
assert re.fullmatch(r'/opt/rustsimhub/dev-state-p1-[a-f0-9]{7,12}',directory);assert all(re.fullmatch('[a-f0-9]{64}',x) for x in [SHA,SOURCE,artifacts['simhubctl_sha256'],artifacts['rustsimhub-core_sha256']])
recovery=directory+'/rustsimhub-recovery';core=directory+'/rustsimhub-core';unit='rustsimhub-sealed-restored-p1';q=shlex.quote
nas=dev=ns=ds=None;started=False;report={'source_archive_sha256':SOURCE,'recipient_private_key_on_device':False,'original_identity_overwritten':False,'new_calls':0,'new_sms':0}
def save():args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(cfg,sock=None):
 c=paramiko.SSHClient();c.load_host_keys(cfg.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));c.set_missing_host_key_policy(paramiko.RejectPolicy());c.connect(cfg['host'],port=cfg.get('port',22),username=cfg['username'],password=cfg.get('password'),key_filename=cfg.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return c
def command(c,text,sudo=False,check=True):
 i,o,e=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=45);o.channel.set_combine_stderr(True)
 if sudo:i.write(ncfg['password']+'\n')
 i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if check and rc:raise RuntimeError('Private recovery command failed; inspect private state')
 return rc,raw

def write(sftp,path,data,uid,gid):
 with sftp.open(path,'wx') as f:f.write(data)
 sftp.chmod(path,0o600);sftp.chown(path,uid,gid)
def read(sftp,path):
 with sftp.open(path,'rb') as f:return f.read(8193)
try:
 assert hashlib.sha256(args.binary.read_bytes()).hexdigest()==SHA
 assert hashlib.sha256(args.ctl.read_bytes()).hexdigest()==artifacts['simhubctl_sha256']
 nas=connect(ncfg);ch=nas.get_transport().open_channel('direct-tcpip',(dcfg['host'],dcfg.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dcfg,ch);ns=nas.open_sftp();ds=dev.open_sftp()
 command(dev,'if systemctl is-active --quiet '+unit+';then exit 10;fi;test ! -S '+q(source+'/runtime/core.sock')+';echo '+q(SHA+'  '+recovery)+' | sha256sum -c -;echo '+q(artifacts['rustsimhub-core_sha256']+'  '+core)+' | sha256sum -c -;'+recovery+' --build-info | grep -Fq '+q('"source_archive_sha256":"'+SOURCE+'"'))
 _,raw=command(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/identity-recovery-private-XXXXXXXX);mkdir -m 0700 "$d/key" "$d/restored" "$d/wrong-key" "$d/wrong-recipient" "$d/wrong-pin" "$d/tampered";printf "BASE=%s\\n" "$d";cat /mnt/docker-data/rustsimhub/p0/audio-builder-image.txt')
 m=re.search(r'BASE=(/mnt/docker-data/rustsimhub/p0/identity-recovery-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',raw);assert m;client,image=m.groups();report['nas_directory']=client
 ns.put(str(args.binary),client+'/rustsimhub-recovery');ns.chmod(client+'/rustsimhub-recovery',0o755)
 ns.put(str(args.ctl),client+'/simhubctl');ns.chmod(client+'/simhubctl',0o755)
 docker='docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 96m --pids-limit 16 --mount '+q('type=bind,src='+client+',dst=/client')+' '+q(image)+' /client/rustsimhub-recovery '
 command(nas,docker+'keygen /client/key',sudo=True);command(nas,docker+'keygen /client/wrong-key',sudo=True)
 _,raw=command(dev,'d=$(mktemp -d /var/tmp/rustsimhub-sealed-backup-XXXXXXXX);chown rustsimhub-core:rustsimhub-core "$d";chmod 0700 "$d";printf "BASE=%s\\n" "$d";id -u rustsimhub-core;id -g rustsimhub-core')
 m=re.search(r'BASE=(/var/tmp/rustsimhub-sealed-backup-[A-Za-z0-9]{8})\n([0-9]+)\n([0-9]+)',raw);assert m;backup,uid,gid=m.groups();uid=int(uid);gid=int(gid);report['device_backup_directory']=backup
 write(ds,backup+'/recipient.txt',read(ns,client+'/key/recipient.txt'),uid,gid)
 before=hashlib.sha256(read(ds,source+'/identity/identity.seed')).hexdigest()
 command(dev,'runuser -u rustsimhub-core -- '+recovery+' backup '+q(source+'/identity')+' '+q(backup+'/recipient.txt')+' '+q(backup+'/identity.age'))
 rc,_=command(dev,'runuser -u rustsimhub-core -- '+recovery+' backup '+q(source+'/identity')+' '+q(backup+'/recipient.txt')+' '+q(backup+'/identity.age'),check=False);assert rc!=0;report['backup_overwrite_denied']=True
 encrypted=read(ds,backup+'/identity.age');assert encrypted.startswith(b'age-encryption.org/v1\n') and len(encrypted)<8192;write(ns,client+'/identity.age',encrypted,1000,1000)
 tampered=bytearray(encrypted);tampered[-1]^=1;write(ns,client+'/tampered.age',tampered,1000,1000)
 for tag,key,blob,expected in [('wrong-recipient','wrong-key','identity.age',pin),('wrong-pin','key','identity.age','0'*64),('tampered','key','tampered.age',pin)]:
  rc,_=command(nas,docker+'restore /client/'+key+'/recovery.agekey /client/'+blob+' '+q(expected)+' /client/'+tag,sudo=True,check=False);assert rc!=0
  assert ns.listdir(client+'/'+tag)==[]
 report['wrong_key_pin_and_tamper_denied_without_writes']=True
 command(nas,docker+'restore /client/key/recovery.agekey /client/identity.age '+q(pin)+' /client/restored',sudo=True)
 rc,_=command(nas,docker+'restore /client/key/recovery.agekey /client/identity.age '+q(pin)+' /client/restored',sudo=True,check=False);assert rc!=0;report['restore_overwrite_denied']=True
 seed=read(ns,client+'/restored/identity.seed');assert hashlib.sha256(seed).hexdigest()==before
 _,raw=command(dev,'d=$(mktemp -d /var/tmp/rustsimhub-sealed-restored-XXXXXXXX);chmod 0711 "$d";cp -a '+q(source+'/state')+' "$d/state";install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$d/identity";install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";printf "BASE=%s\\n" "$d"')
 m=re.search(r'BASE=(/var/tmp/rustsimhub-sealed-restored-[A-Za-z0-9]{8})',raw);assert m;restored=m.group(1);report['device_restored_directory']=restored;write(ds,restored+'/identity/identity.seed',seed,uid,gid);write(ds,restored+'/identity/.identity.lock',b'',uid,gid)
 # Recovered plaintext identity returns only to a new private identity directory over pinned SSH.
 del seed
 socket=restored+'/runtime/core.sock'
 command(dev,'systemd-run --unit '+unit+' --collect -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes -p PrivateDevices=yes -p InaccessiblePaths=/run/dbus -p ProtectSystem=strict -p '+q('ReadWritePaths='+restored+'/state '+restored+'/identity '+restored+'/runtime')+' -p RuntimeMaxSec=90 -p TimeoutStopSec=5 -p MemoryMax=96M -p TasksMax=8 '+core+' --serve-authenticated '+q(socket)+' 0 '+q(restored+'/state')+' '+q(restored+'/identity'));started=True
 command(dev,'n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 15;sleep 1;done')
 def rpc(body):
  raw=json.dumps({'protocol':2,'id':str(uuid.uuid4()),'command':body},separators=(',',':'));delimiter='RUSTSIMHUB_RECOVERY_RPC_END'
  _,r=command(dev,core+' --request-auth '+q(socket)+" <<'"+delimiter+"'\n"+raw+'\n'+delimiter);return json.loads(r.splitlines()[-1])['result']
 assert bytes(rpc({'op':'identity'})['public_key']).hex()==pin
 signing='docker run --rm -i --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 96m --pids-limit 16 --mount '+q('type=bind,src='+owner_dir+',dst=/owner,readonly')+' --mount '+q('type=bind,src='+client+'/simhubctl,dst=/usr/local/bin/simhubctl,readonly')+' '+q(image)+' /usr/local/bin/simhubctl '
 # Derive only the public key on the NAS; no owner signing seed is transferred to device or PC.
 _,r=command(nas,"python3 - <<'PY'\nimport hashlib,json,pathlib,subprocess\np=pathlib.Path("+repr(owner_dir+'/key/client.seed')+")\nb=p.read_bytes();assert len(b)==72 and b[:8]==b'RSHCLI01' and hashlib.sha256(b[:40]).digest()==b[40:]\nder=bytes.fromhex('302e020100300506032b657004220420')+b[8:40]\nr=subprocess.run(['openssl','pkey','-inform','DER','-pubout','-outform','DER'],input=der,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True).stdout\nassert len(r)==44 and r[:12]==bytes.fromhex('302a300506032b6570032100')\nprint(json.dumps(list(r[12:])))\nPY")
 public=json.loads(r.splitlines()[-1]);assert len(public)==32
 challenge=rpc({'op':'challenge','public_key':public});payload=json.dumps(challenge,separators=(',',':'));delimiter='RUSTSIMHUB_RECOVERY_CHALLENGE_END'
 _,r=command(nas,signing+'sign /owner/key '+q(pin)+" login <<'"+delimiter+"'\n"+payload+'\n'+delimiter,sudo=True);proof=json.loads(r.splitlines()[-1]);token=rpc({'op':'authenticate','challenge_id':challenge['id'],'signature':proof})['token']
 assert rpc({'op':'health','token':token}).get('ok');inbox=rpc({'op':'inbox','token':token,'after':None,'limit':32});report['restored_inbox_count']=len(inbox['messages']);assert len(inbox['messages'])==original['inbox_count_immediately_after_restart']
 assert hashlib.sha256(json.dumps(inbox,sort_keys=True,separators=(',',':')).encode()).hexdigest()==original['inbox_sha256']
 assert hashlib.sha256(read(ds,source+'/identity/identity.seed')).hexdigest()==before
 report.update({'expected_identity_pin_verified':True,'matching_owner_login_after_restore':True,'matching_database_messages_preserved':True,'recipient_private_key_on_device':False,'passed':True});save();print('Private encrypted identity and matching owner/data restore passed; original unchanged; no hardware actions')
finally:
 if dev and started:command(dev,'systemctl stop '+unit,check=False)
 if dev:
  rc,_=command(dev,'for n in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$n";done;for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done',check=False);report['cleanup_verified']=rc==0
 save()
 for c in [ds,ns,dev,nas]:
  if c:c.close()
