#!/usr/bin/env python3
"""Authorized single 10010 encrypted carrier diagnostic: silence uplink, no recording, bounded hangup."""
import argparse,ipaddress,json,pathlib,re,shlex,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nas',type=pathlib.Path,required=True);p.add_argument('--device',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--expected-source-sha256',required=True);p.add_argument('--call-10010',action='store_true',required=True);p.add_argument('--once-tag',required=True);args=p.parse_args()
if not re.fullmatch(r'[a-z0-9-]{1,40}',args.once_tag):raise SystemExit('Invalid unique once tag')
if not re.fullmatch(r'[a-f0-9]{64}',args.expected_source_sha256):raise SystemExit('Invalid expected source digest')
nas_config=json.loads(args.nas.read_text(encoding='utf-8-sig'));dev_config=json.loads(args.device.read_text(encoding='utf-8-sig'))
def connect(config,sock=None):
 c=paramiko.SSHClient();c.load_host_keys(config.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));c.set_missing_host_key_policy(paramiko.RejectPolicy());c.connect(config['host'],port=config.get('port',22),username=config['username'],password=config.get('password'),key_filename=config.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return c
def command(c,text,sudo=False,check=True):
 inp,out,err=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=20);out.channel.set_combine_stderr(True)
 if sudo:inp.write(nas_config['password']+'\n')
 inp.write('set -eu\numask 077\n'+text+'\n');inp.channel.shutdown_write();b=out.read().decode(errors='replace');rc=out.channel.recv_exit_status()
 if rc:note(b)
 if check and rc:raise RuntimeError('Owned RTC test command failed; inspect private report')
 return rc,b
q=shlex.quote;nas=None;dev=None;nsftp=None;dsftp=None;test_dir=None;dev_dir=None;container='rustsimhub-rtc-carrier-peer-p3';unit='rustsimhub-carrier-rtc-media-p3';logs=[];nas_started=False;dev_started=False;cleanup_ok=True
def note(text):logs.append(text)
def remote_read(sftp,path):
 with sftp.open(path,'rb') as f:return f.read(16385)
def wait_file(sftp,path,end):
 while time.monotonic()<end:
  try:return remote_read(sftp,path)
  except FileNotFoundError:time.sleep(.03)
 raise RuntimeError('Bounded RTC signaling deadline')
def put(sftp,path,data,uid,gid):
 temporary=path+'.partial'
 with sftp.open(temporary,'wx') as f:f.write(data)
 sftp.chmod(temporary,0o600);sftp.chown(temporary,uid,gid);sftp.rename(temporary,path)
core='/opt/rustsimhub/dev-rtc-core-p3/rustsimhub-core';ctl='/opt/rustsimhub/dev-rtc-core-p3/simhubctl';audio='/opt/rustsimhub/dev-rtc-p3/rustsimhub-audio-alsa'
core_unit='rustsimhub-carrier-rtc-core-p3';deadline='rustsimhub-carrier-rtc-deadline-p3';once='/var/tmp/rustsimhub-carrier-rtc-'+args.once_tag+'.once'
base=None;core_started=False;dial_consumed=False
try:
 nas=connect(nas_config);channel=nas.get_transport().open_channel('direct-tcpip',(dev_config['host'],dev_config.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dev_config,channel)
 dev_ip=str(ipaddress.IPv4Address(dev_config['host']));_,route=command(nas,'ip -4 route get '+q(dev_ip));t=route.split();nas_ip=str(ipaddress.IPv4Address(t[t.index('src')+1]))
 if not ipaddress.ip_address(nas_ip).is_private:raise RuntimeError('Private LAN required')
 command(nas,'if docker inspect '+container+' >/dev/null 2>&1;then exit 10;fi',sudo=True)
 command(dev,'test ! -e '+once+'; for u in '+core_unit+' '+unit+';do if systemctl is-active --quiet "$u";then exit 10;fi;done; for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done')
 _,b=command(nas,'base=/mnt/docker-data/rustsimhub/p0; d=$(mktemp -d "$base/rtc-lan-private-XXXXXXXX");chown 1000:1000 "$d";chmod 0700 "$d";install -o 1000 -g 1000 -m 0755 "$base/audio-target/release/rustsimhub-audio-alsa" "$d/rustsimhub-audio-alsa"; printf "TEST_DIR=%s\n" "$d";cat "$base/audio-builder-image.txt"',sudo=True)
 m=re.search(r'TEST_DIR=(/mnt/docker-data/rustsimhub/p0/rtc-lan-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',b)
 if not m:raise RuntimeError('NAS namespace denied')
 test_dir,image=m.groups()
 _,info=command(nas,'docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --mount '+q('type=bind,src='+test_dir+',dst=/test,readonly')+' '+q(image)+' /test/rustsimhub-audio-alsa --build-info',sudo=True)
 if json.loads(info.splitlines()[-1]).get('source_archive_sha256')!=args.expected_source_sha256:raise RuntimeError('NAS binary mismatch')
 _,info=command(dev,audio+' --build-info')
 if json.loads(info.splitlines()[-1]).get('source_archive_sha256')!=args.expected_source_sha256:raise RuntimeError('Device binary mismatch')
 _,b=command(dev,'b=$(mktemp -d /var/tmp/rustsimhub-carrier-rtc-XXXXXXXX); '+core+' --inbox-prepare-fixture "$b"; m=$(mktemp -d /var/tmp/rustsimhub-carrier-rtc-runtime-XXXXXXXX);chgrp audio "$m";chmod 2750 "$m";install -d -o nassimhub -g nassimhub -m 0700 "$m/client"; printf "BASE=%s\nMEDIA=%s\n" "$b" "$m"; id -u nassimhub;id -g nassimhub')
 m=re.search(r'BASE=(/var/tmp/rustsimhub-carrier-rtc-[A-Za-z0-9]{8})\nMEDIA=(/var/tmp/rustsimhub-carrier-rtc-runtime-[A-Za-z0-9]{8})\n([0-9]+)\n([0-9]+)',b)
 if not m:raise RuntimeError('Device namespace denied')
 base,media,uid,gid=m.groups();uid=int(uid);gid=int(gid);dev_dir=media+'/client';sock=base+'/runtime/core.sock';media_sock=media+'/media.sock'
 note(json.dumps({'nas_fixture':test_dir,'device_fixture':base,'media_fixture':media,'source_archive_sha256':args.expected_source_sha256,'once':once}));nsftp=nas.open_sftp();dsftp=dev.open_sftp()
 put(nsftp,test_dir+'/config.json',json.dumps({'mode':'offer_synthetic','bind':nas_ip+':0','peer_ip':dev_ip,'seconds':10,'core_socket':None,'core_uid':None}).encode(),1000,1000)
 put(dsftp,dev_dir+'/config.json',json.dumps({'mode':'answer_authorized','bind':dev_ip+':0','peer_ip':nas_ip,'seconds':3,'core_socket':media_sock,'core_uid':0}).encode(),uid,gid)
 command(dev,'systemd-run --unit '+core_unit+' -p User=root -p Group=root -p NoNewPrivileges=yes -p ProtectSystem=strict -p '+q('ReadWritePaths='+base+' '+media)+' -p RuntimeMaxSec=90 -p TimeoutStopSec=20 '+core+' --serve-media-authorizing '+q(sock)+' 0 '+q(base+'/state')+' '+q(base+'/identity')+' '+q(media_sock)+' '+str(uid));core_started=True
 command(dev,'count=0;while ! test -S '+q(sock)+' || ! test -S '+q(media_sock)+';do count=$((count+1));test "$count" -lt 15;sleep 1;done; pin=$(cat '+q(base+'/device.pubhex')+');'+ctl+' login '+q(sock)+' '+q(base+'/owner-key')+' "$pin" '+q(base+'/sessions/owner.json'))
 prefix=ctl+' ';auth=q(sock)+' '+q(base+'/sessions/owner.json')
 command(dev,'count=0;while :;do if '+prefix+'call-status '+auth+' > '+q(base+'/before.json')+' && grep -q '+q('"available":true')+' '+q(base+'/before.json')+';then break;fi;count=$((count+1));test "$count" -lt 20;sleep 1;done;grep -q '+q('"active_calls":0')+' '+q(base+'/before.json'))
 _,line=command(dev,prefix+'line '+auth);epoch=json.loads(line.splitlines()[-1])['line']
 put(dsftp,base+'/dial-draft.json',json.dumps({'line_epoch':epoch,'idempotency_key':'rtc-carrier-'+args.once_tag+'-dial','action':'dial','call_id':None,'payload':'10010'}).encode(),0,0)
 _,b=command(dev,prefix+'prepare-call '+auth+' '+q(base+'/dial-draft.json'));operation=json.loads(b.splitlines()[-1])['operation']['id']
 command(dev,'systemd-run --unit '+deadline+' --on-active=18s --timer-property=AccuracySec=100ms /usr/bin/systemctl stop '+core_unit+'; (set -C;printf "%s\n%s\n" '+q(base)+' '+q(operation)+' > '+once+')')
 dial_consumed=True
 rc,b=command(dev,prefix+'submit-call '+auth+' '+q(operation)+' none normal > '+q(base+'/dial-result.json'),check=False);note('Dial submit exit='+str(rc)+'; never redial')
 command(dev,prefix+'call-operation '+auth+' '+q(operation)+' > '+q(base+'/operation.json')+';'+prefix+'save-call-lease '+q(base+'/operation.json')+' '+q(base+'/lease.json'))
 lease=json.loads(remote_read(dsftp,base+'/lease.json'));call_id=lease['call_id']
 command(dev,'count=0;while test "$count" -lt 5;do '+prefix+'call-status '+auth+' >/dev/null;'+prefix+'call '+auth+' '+q(call_id)+' > '+q(base+'/live.json')+';if grep -q '+q('"state":"active"')+' '+q(base+'/live.json')+';then exit 0;fi;count=$((count+1));sleep 1;done;exit 10')
 command(nas,'docker run -d --name '+container+' --label org.rustsimhub.purpose=rtc-carrier-acceptance --network host --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --cpus 1 --memory 128m --pids-limit 16 --mount '+q('type=bind,src='+test_dir+',dst=/test')+' '+q(image)+' /test/rustsimhub-audio-alsa --rtc-lan-peer /test/config.json',sudo=True);nas_started=True
 end=time.monotonic()+10;offer=wait_file(nsftp,test_dir+'/offer.sdp',end);put(dsftp,dev_dir+'/offer.sdp',offer,uid,gid)
 command(dev,'systemd-run --unit='+unit+' --collect -p User=nassimhub -p Group=nassimhub -p SupplementaryGroups=audio -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateTmp=yes -p PrivateDevices=no -p DevicePolicy=closed -p '+q('BindPaths='+media)+' -p "DeviceAllow=/dev/snd/controlC0 rw" -p "DeviceAllow=/dev/snd/pcmC0D0p rw" -p "DeviceAllow=/dev/snd/pcmC0D1c rw" -p "DeviceAllow=/dev/snd/pcmC0D4p rw" -p "DeviceAllow=/dev/snd/pcmC0D4c rw" -p RuntimeMaxSec=15 -p TimeoutStopSec=2 -p MemoryMax=128M -p TasksMax=8 '+audio+' --rtc-lan-peer '+q(dev_dir+'/config.json'));dev_started=True
 answer=wait_file(dsftp,dev_dir+'/answer.sdp',end);put(nsftp,test_dir+'/answer.sdp',answer,1000,1000)
 v=json.loads(wait_file(dsftp,dev_dir+'/verified.json',end))
 if not v.get('dtls_verified') or len(v['peer_key'])!=32:raise RuntimeError('Actual DTLS identity required')
 peer=bytes(v['peer_key']).hex()
 command(dev,prefix+'issue-media '+auth+' '+q(base+'/lease.json')+' '+q(peer)+' '+q(base+'/issued-ticket.json'))
 put(dsftp,dev_dir+'/ticket.json',remote_read(dsftp,base+'/issued-ticket.json'),uid,gid)
 result=json.loads(wait_file(dsftp,dev_dir+'/result.json',end));note(json.dumps({'device':result}))
 put(dsftp,base+'/hangup-draft.json',json.dumps({'line_epoch':epoch,'idempotency_key':'rtc-carrier-'+args.once_tag+'-hangup','action':'hangup','call_id':call_id,'payload':''}).encode(),0,0)
 _,b=command(dev,prefix+'prepare-call '+auth+' '+q(base+'/hangup-draft.json'));hangup=json.loads(b.splitlines()[-1])['operation']['id']
 command(dev,prefix+'submit-call '+auth+' '+q(hangup)+' '+q(base+'/lease.json')+' hangup > '+q(base+'/hangup-result.json'))
 remote=json.loads(wait_file(nsftp,test_dir+'/result.json',end+3));note(json.dumps({'nas':remote}))
 command(dev,prefix+'call-status '+auth+' > '+q(base+'/final-status.json')+';grep -q '+q('"active_calls":0')+' '+q(base+'/final-status.json'))
 if not result.get('core_authorized') or not result.get('dtls_verified') or not result.get('closed') or result.get('transport_failed') or result.get('captured_frames',0)<75 or result.get('capture_peak',0)==0 or result.get('playback_peak',999)>4 or remote.get('received_frames',0)<75 or remote.get('decoded_peak',0)==0:raise RuntimeError('Carrier encrypted PCM acceptance failed')
 note('One 10010 dial, silence uplink, real downlink discarded after decoding; automatic hangup; no recording or SMS/data activation')
finally:
 if dev:
  try:
   rc,b=command(dev,('systemctl stop '+unit+' || true; ' if dev_started else '')+('systemctl stop '+core_unit+';systemctl stop '+deadline+'.timer '+deadline+'.service || true;' if core_started else '')+'for u in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$u";done;for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done',check=False);note(b)
   if rc:cleanup_ok=False
   if base:note(command(dev,core+' --inbox-verify-fixture '+q(base),check=False)[1])
  except Exception as e:note(type(e).__name__);cleanup_ok=False
 if nas and nas_started:
  try:
   rc,b=command(nas,'test "$(docker inspect '+container+' --format '+q('{{index .Config.Labels "org.rustsimhub.purpose"}}')+')" = rtc-carrier-acceptance;docker logs '+container+' > '+q(test_dir+'/container.log')+' 2>&1;docker stop -t 3 '+container+' >/dev/null;docker rm '+container+' >/dev/null',sudo=True,check=False);note(b)
   if rc:cleanup_ok=False
  except Exception as e:note(type(e).__name__);cleanup_ok=False
 for c in [nsftp,dsftp,dev,nas]:
  if c:c.close()
 args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text('\n'.join(logs),encoding='utf-8')
if not cleanup_ok:raise RuntimeError('Owned cleanup postconditions failed')
print('Encrypted carrier diagnostic passed; no recording, new SMS or cellular data')
