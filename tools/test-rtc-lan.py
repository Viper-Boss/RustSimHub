#!/usr/bin/env python3
"""Synthetic-only encrypted LAN acceptance. Never dials, opens ALSA or touches old services."""
import argparse,ipaddress,json,pathlib,re,shlex,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--nas',type=pathlib.Path,required=True);p.add_argument('--device',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--expected-source-sha256',required=True);args=p.parse_args()
if not re.fullmatch(r'[a-f0-9]{64}',args.expected_source_sha256):raise SystemExit('Invalid expected source digest')
nas_config=json.loads(args.nas.read_text(encoding='utf-8-sig'));dev_config=json.loads(args.device.read_text(encoding='utf-8-sig'))
def connect(config,sock=None):
 c=paramiko.SSHClient();c.load_host_keys(config.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));c.set_missing_host_key_policy(paramiko.RejectPolicy());c.connect(config['host'],port=config.get('port',22),username=config['username'],password=config.get('password'),key_filename=config.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return c
def command(c,text,sudo=False,check=True):
 inp,out,err=c.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=20);out.channel.set_combine_stderr(True)
 if sudo:inp.write(nas_config['password']+'\n')
 inp.write('set -eu\n'+text+'\n');inp.channel.shutdown_write();b=out.read().decode(errors='replace');rc=out.channel.recv_exit_status()
 if check and rc:raise RuntimeError('Owned RTC test command failed; inspect private report')
 return rc,b
q=shlex.quote;nas=None;dev=None;nsftp=None;dsftp=None;test_dir=None;dev_dir=None;container='rustsimhub-rtc-peer-p3';unit='rustsimhub-rtc-lan-p3';logs=[];nas_started=False;dev_started=False;cleanup_ok=True
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
try:
 nas=connect(nas_config);channel=nas.get_transport().open_channel('direct-tcpip',(dev_config['host'],dev_config.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dev_config,channel)
 dev_ip=str(ipaddress.IPv4Address(dev_config['host']));_,route=command(nas,'ip -4 route get '+q(dev_ip));tokens=route.split();nas_ip=str(ipaddress.IPv4Address(tokens[tokens.index('src')+1]))
 if not ipaddress.ip_address(nas_ip).is_private:raise RuntimeError('LAN test requires private interface')
 # Do not reuse or remove a live container/unit from another invocation.
 rc,_=command(nas,'if docker inspect '+container+' >/dev/null 2>&1;then exit 10;fi',sudo=True,check=False)
 if rc:raise RuntimeError('RTC container already present; inspect it first')
 rc,_=command(dev,'if systemctl is-active --quiet '+unit+';then exit 10;fi',check=False)
 if rc:raise RuntimeError('RTC unit already active; inspect it first')
 _,b=command(nas,'base=/mnt/docker-data/rustsimhub/p0; d=$(mktemp -d "$base/rtc-lan-private-XXXXXXXX"); chown 1000:1000 "$d"; chmod 0700 "$d"; install -o 1000 -g 1000 -m 0755 "$base/audio-target/release/rustsimhub-audio-alsa" "$d/rustsimhub-audio-alsa"; printf "TEST_DIR=%s\\n" "$d"; cat "$base/audio-builder-image.txt"',sudo=True)
 match=re.search(r'TEST_DIR=(/mnt/docker-data/rustsimhub/p0/rtc-lan-private-[A-Za-z0-9]{8})\n(sha256:[a-f0-9]{64})',b)
 if not match:raise RuntimeError('Unexpected owned RTC namespace')
 test_dir,image=match.groups()
 # Host may lack libasound; validate inside the pinned image without hardware instead.
 _,info=command(nas,'docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 64m --pids-limit 16 --mount '+q('type=bind,src='+test_dir+',dst=/test,readonly')+' '+q(image)+' /test/rustsimhub-audio-alsa --build-info',sudo=True)
 if json.loads(info.splitlines()[-1]).get('source_archive_sha256')!=args.expected_source_sha256:raise RuntimeError('NAS artifact source mismatch')
 _,info=command(dev,'/opt/rustsimhub/dev-rtc-p3/rustsimhub-audio-alsa --build-info')
 if json.loads(info.splitlines()[-1]).get('source_archive_sha256')!=args.expected_source_sha256:raise RuntimeError('Device artifact source mismatch')
 _,b=command(dev,'d=$(mktemp -d /var/tmp/rustsimhub-rtc-lan-XXXXXXXX); chown rustsimhub-edge:rustsimhub-edge "$d"; chmod 0700 "$d"; printf "TEST_DIR=%s\\n" "$d"; id -u rustsimhub-edge; id -g rustsimhub-edge')
 match=re.search(r'TEST_DIR=(/var/tmp/rustsimhub-rtc-lan-[A-Za-z0-9]{8})\n([0-9]+)\n([0-9]+)',b)
 if not match:raise RuntimeError('Unexpected device RTC namespace')
 dev_dir,uid,gid=match.groups();note(json.dumps({'nas_fixture':test_dir,'device_fixture':dev_dir,'source_archive_sha256':args.expected_source_sha256}));uid=int(uid);gid=int(gid);nsftp=nas.open_sftp();dsftp=dev.open_sftp()
 put(nsftp,test_dir+'/config.json',json.dumps({'mode':'offer_synthetic','bind':nas_ip+':0','peer_ip':dev_ip,'seconds':5,'core_socket':None,'core_uid':None}).encode(),1000,1000)
 put(dsftp,dev_dir+'/config.json',json.dumps({'mode':'answer_synthetic','bind':dev_ip+':0','peer_ip':nas_ip,'seconds':5,'core_socket':None,'core_uid':None}).encode(),uid,gid)
 _,b=command(nas,'docker run -d --name '+container+' --label org.rustsimhub.purpose=rtc-synthetic-acceptance --network host --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --cpus 1 --memory 128m --pids-limit 16 --mount '+q('type=bind,src='+test_dir+',dst=/test')+' '+q(image)+' /test/rustsimhub-audio-alsa --rtc-lan-peer /test/config.json',sudo=True);nas_started=True;note(b)
 end=time.monotonic()+35;offer=wait_file(nsftp,test_dir+'/offer.sdp',end);put(dsftp,dev_dir+'/offer.sdp',offer,uid,gid)
 binary='/opt/rustsimhub/dev-rtc-p3/rustsimhub-audio-alsa'
 _,b=command(dev,'systemd-run --unit='+unit+' --collect -p User=rustsimhub-edge -p Group=rustsimhub-edge -p PrivateDevices=yes -p PrivateTmp=yes -p ProtectSystem=strict -p NoNewPrivileges=yes -p InaccessiblePaths=/run/dbus -p '+q('BindPaths='+dev_dir)+' -p MemoryMax=128M -p TasksMax=8 -p RuntimeMaxSec=40 '+q(binary)+' --rtc-lan-peer '+q(dev_dir+'/config.json'));dev_started=True;note(b)
 answer=wait_file(dsftp,dev_dir+'/answer.sdp',end);put(nsftp,test_dir+'/answer.sdp',answer,1000,1000)
 a=json.loads(wait_file(nsftp,test_dir+'/result.json',end));b=json.loads(wait_file(dsftp,dev_dir+'/result.json',end))
 note(json.dumps({'nas':a,'device':b,'synthetic_only':True,'hardware_mutations':False},sort_keys=True))
 for result in [a,b]:
  if not result.get('dtls_verified') or result.get('pcm_opened') or not result.get('revocation_closed') or result.get('received_frames',0)<200 or not (0<=result.get('decoded_peak',65536)<=4):raise RuntimeError('Encrypted LAN acceptance failed')
 note(json.dumps({'synthetic_only':True,'nas':a,'device':b,'hardware_mutations':False,'new_calls':0,'new_sms':0},sort_keys=True))
finally:
 if dev:
  try:
   rc,b=command(dev,('systemctl stop '+unit+' || true; ' if dev_started else '')+' for n in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$n";done; for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done; if systemctl is-active --quiet '+unit+';then exit 10;fi',check=False);note(b)
   if rc:cleanup_ok=False
  except Exception as e:note(type(e).__name__);cleanup_ok=False
 if nas and test_dir and nas_started:
  try:
   rc,b=command(nas,'if test "$(docker inspect '+container+' --format \'{{index .Config.Labels "org.rustsimhub.purpose"}}\' 2>/dev/null)" = rtc-synthetic-acceptance;then docker logs '+container+' > '+q(test_dir+'/container.log')+' 2>&1; docker stop -t 3 '+container+' >/dev/null; docker rm '+container+' >/dev/null;fi',sudo=True,check=False);note(b)
   if rc:cleanup_ok=False
  except Exception as e:note(type(e).__name__);cleanup_ok=False
 for c in [nsftp,dsftp,dev,nas]:
  if c:c.close()
 args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text('\n'.join(logs),encoding='utf-8')
if not cleanup_ok:raise RuntimeError('RTC cleanup postconditions failed; inspect private report')
print('Synthetic encrypted NAS/device LAN acceptance passed; owned services stopped; no hardware actions')
