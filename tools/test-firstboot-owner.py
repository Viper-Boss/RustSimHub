#!/usr/bin/env python3
"""Offline signed first owner on a new private production database; no calls/SMS/network writes."""
import argparse,hashlib,json,pathlib,re,shlex,sys,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nas',type=pathlib.Path,required=True);p.add_argument('--device',type=pathlib.Path,required=True);p.add_argument('--ctl',type=pathlib.Path,required=True);p.add_argument('--report',type=pathlib.Path,required=True);args=p.parse_args()
ncfg=json.loads(args.nas.read_text(encoding='utf-8-sig'));dcfg=json.loads(args.device.read_text(encoding='utf-8-sig'))
SOURCE='35ecbce247dd971c3e768e731cdbd3ec0b8cf1af8041c34583caa2fabb9307ee';CTL_SHA='3670332c431f257d5890cce03ecb1388ec407151aee7ab0501520cee53807529';core='/opt/rustsimhub/dev-wifi-p4/rustsimhub-core';unit='rustsimhub-firstboot-owner-p1';q=shlex.quote
report={'production_core':True,'hardware_mutations':False,'new_calls':0,'new_sms':0,'source_archive_sha256':SOURCE};nas=dev=ns=ds=None;started=False

def save():args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(cfg,sock=None):
 c=paramiko.SSHClient();c.load_host_keys(cfg.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));c.set_missing_host_key_policy(paramiko.RejectPolicy());c.connect(cfg['host'],port=cfg.get('port',22),username=cfg['username'],password=cfg.get('password'),key_filename=cfg.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return c
def command(c,text,sudo=False,input_bytes=b'',check=True):
 i,o,e=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=45);o.channel.set_combine_stderr(True)
 if sudo:i.write(ncfg['password']+'\n')
 i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();data=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if check and rc:raise RuntimeError('Firstboot command failed; no retry of provisioning')
 return rc,data
try:
 assert hashlib.sha256(args.ctl.read_bytes()).hexdigest()==CTL_SHA
 nas=connect(ncfg);ch=nas.get_transport().open_channel('direct-tcpip',(dcfg['host'],dcfg.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dcfg,ch)
 command(dev,'if systemctl is-active --quiet '+unit+';then exit 10;fi;'+core+' --build-info | grep -q \'"development_fixtures":false\';'+core+' --build-info | grep -q '+q('"source_archive_sha256":"'+SOURCE+'"'))
 _,b=command(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/firstboot-client-private-XXXXXXXX);mkdir -m 0700 "$d/key";printf "KEY_DIR=%s\\n" "$d";cat /mnt/docker-data/rustsimhub/p0/audio-builder-image.txt')
 m=re.search(r'KEY_DIR=(/mnt/docker-data/rustsimhub/p0/firstboot-client-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',b);assert m;client,image=m.groups();report['nas_owner_directory']=client;ns=nas.open_sftp();ds=dev.open_sftp()
 ns.put(str(args.ctl),client+'/simhubctl');ns.chmod(client+'/simhubctl',0o755)
 docker='docker run --rm -i --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 96m --pids-limit 16 --mount '+q('type=bind,src='+client+',dst=/client')+' '+q(image)+' /client/simhubctl '
 _,b=command(nas,docker+'keygen /client/key',sudo=True);owner=json.loads(b.splitlines()[-1]);public=owner['public_key'];public_hex=owner['public_key_hex']
 _,b=command(dev,'d=$(mktemp -d /var/tmp/rustsimhub-firstboot-private-XXXXXXXX);chmod 0711 "$d";install -d -o rustsimhub-core -g rustsimhub-core -m 0700 "$d/state" "$d/identity";install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";printf "BASE=%s\\n" "$d"')
 m=re.search(r'BASE=(/var/tmp/rustsimhub-firstboot-private-[A-Za-z0-9]{8})',b);assert m;base=m.group(1);report['device_directory']=base;socket=base+'/runtime/core.sock';save()
 inp,out,err=dev.exec_command('runuser -u rustsimhub-core -- '+core+' --provision-owner '+q(base+'/state')+' '+q(base+'/identity')+' '+q(public_hex),timeout=20)
 challenge=json.loads(out.readline(4097));transcript=json.loads(bytes(challenge['transcript']));pin=bytes(transcript['device_public_key']).hex();report['device_pin']=pin
 def sign(challenge,purpose):
  payload=json.dumps(challenge,separators=(',',':'));delimiter='RUSTSIMHUB_CHALLENGE_END'
  _,value=command(nas,docker+'sign /client/key '+q(pin)+' '+q(purpose)+" <<'"+delimiter+"'\n"+payload+'\n'+delimiter,sudo=True);proof=json.loads(value.splitlines()[-1]);assert len(proof)==64;return proof
 proof=sign(challenge,'bootstrap');inp.write(json.dumps(proof));inp.channel.shutdown_write();tail=out.read().decode(errors='replace');rc=out.channel.recv_exit_status();assert rc==0 and json.loads(tail.splitlines()[-1]).get('owner_initialized');report['offline_signed_owner_initialized']=True
 def start():
  global started
  command(dev,'systemd-run --unit '+unit+' --collect -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p AmbientCapabilities= -p NoNewPrivileges=yes -p PrivateDevices=yes -p ProtectSystem=strict -p '+q('ReadWritePaths='+base+'/state '+base+'/identity '+base+'/runtime')+' -p RuntimeMaxSec=120 -p TimeoutStopSec=5 -p MemoryMax=96M -p TasksMax=8 '+core+' --serve-observing '+q(socket)+' 0 '+q(base+'/state')+' '+q(base+'/identity'));started=True
  command(dev,'n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 15;sleep 1;done')
 def rpc(body):
  payload=json.dumps({'protocol':2,'id':str(uuid.uuid4()),'command':body},separators=(',',':'));delimiter='RUSTSIMHUB_RPC_END'
  _,raw=command(dev,core+' --request-auth '+q(socket)+" <<'"+delimiter+"'\n"+payload+'\n'+delimiter);return json.loads(raw.splitlines()[-1])['result']
 def login():
  c=rpc({'op':'challenge','public_key':public});proof=sign(c,'login');answer=rpc({'op':'authenticate','challenge_id':c['id'],'signature':proof});assert len(answer['token'])==64;return answer['token']
 start();assert bytes(rpc({'op':'identity'})['public_key']).hex()==pin;token=login();assert rpc({'op':'health','token':token}).get('ok')
 for _ in range(15):
  line=rpc({'op':'line_status','token':token})
  if line['status']=='ready':break
  time.sleep(1)
 assert line['status']=='ready';inbox=rpc({'op':'inbox','token':token,'after':None,'limit':32});report['live_inbox_messages']=len(inbox['messages']);before_ids=[x['id'] for x in inbox['messages']]
 command(dev,'systemctl stop '+unit);started=False;start();assert bytes(rpc({'op':'identity'})['public_key']).hex()==pin
 assert rpc({'op':'health','token':token}).get('ok') is False;report['old_token_denied_after_restart']=True
 token2=login();inbox2=rpc({'op':'inbox','token':token2,'after':None,'limit':32});after_ids=[x['id'] for x in inbox2['messages']];assert all(i in after_ids for i in before_ids);report['identity_owner_and_messages_preserved']=True
 report['restart_duplicate_candidates_detected']=len(after_ids)>len(before_ids)
 report['inbox_count_immediately_after_restart']=len(after_ids);report['ready']=True;save();print('Private production first owner, signed login, SIM read and restart persistence passed; no hardware actions')
finally:
 if dev and started:
  command(dev,'systemctl stop '+unit,check=False)
 if dev:
  rc,b=command(dev,'for n in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$n";done;for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done',check=False);report['cleanup_verified']=rc==0
 save()
 for c in [ds,ns,dev,nas]:
  if c:c.close()
