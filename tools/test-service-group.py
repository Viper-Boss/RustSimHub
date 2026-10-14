#!/usr/bin/env python3
"""Private signed app group: non-root core/HTTPS and offline local health, crash rollback."""
import argparse,hashlib,json,pathlib,re,shlex,sys,tarfile,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['nas','device','artifacts','firstboot-report','ctl','report']:p.add_argument('--'+n,type=pathlib.Path,required=True)
a=p.parse_args();nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'));m=json.loads(a.artifacts.read_text(encoding='utf-8'));first=json.loads(a.firstboot_report.read_text(encoding='utf-8'))
directory=m['device_directory'];source=first['device_directory'];pin=first['device_pin'];public=first['owner_public_key'];owner=first['nas_owner_directory'];sha=m['source_archive_sha256'];q=shlex.quote
assert re.fullmatch(r'/opt/rustsimhub/dev-state-p1-[a-f0-9]{7,12}',directory)
assert re.fullmatch(r'/var/tmp/rustsimhub-firstboot-private-[A-Za-z0-9]{8}',source)
assert re.fullmatch(r'/mnt/docker-data/rustsimhub/p0/firstboot-client-private-[A-Za-z0-9]{8}',owner)
assert first['ready'] and not first['restart_duplicate_candidates_detected']
assert hashlib.sha256(a.ctl.read_bytes()).hexdigest()==m['simhubctl_sha256']
names=['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa','Opus-COPYING.txt']
nas=dev=ns=ds=None;started=False;base=None;setup='rustsimhub-service-group-setup-p6'
report={'new_calls':0,'new_sms':0,'network_mutations':False,'boot_units_installed':False,'development_signer':True,'source_archive_sha256':sha}
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(s,text,sudo=False,check=True):
 i,o,e=s.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=60);o.channel.set_combine_stderr(True)
 if sudo:i.write(nc['password']+'\n')
 i.write('set -eu\numask 077\n'+text+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if check and rc:report['failed_command_output']=raw;save();raise RuntimeError('Private group command failed; inspect private report, no automatic rerun')
 return rc,raw
def write(path,b,uid=0,gid=0,mode=0o600):
 with ds.open(path,'wx') as f:f.write(b)
 ds.chmod(path,mode);ds.chown(path,uid,gid)
try:
 nas=connect(nc);ch=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,ch);ns=nas.open_sftp();ds=dev.open_sftp()
 run(dev,'for u in '+setup+' rustsimhub-service-group-core-p6 rustsimhub-service-group-edge-p6;do if systemctl is-active --quiet "$u";then exit 10;fi;done;test ! -S '+q(source+'/runtime/core.sock'))
 # Publisher owns the development release secret on the NAS; only public packages leave it.
 text='''b=/mnt/docker-data/rustsimhub/p0
test "$(docker inspect rustsimhub-build-p0 --format '{{.State.Status}} {{.State.ExitCode}}')" = 'exited 0'
pub=$(mktemp -d "$b/group-publisher-private-XXXXXXXX")
mkdir -m 0700 "$pub/key" "$pub/input" "$pub/package1" "$pub/package2" "$pub/package3"
for n in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-update;do install -m 0755 "$b/src/target/production/$n" "$pub/input/$n";done
echo 'ad18aab86765db4699b2013e0b3cb48eea752e5bbdd11eeb8337b081a6a6bf85  /mnt/docker-data/rustsimhub/p0/audio-target/release/rustsimhub-audio-alsa' | sha256sum -c -
install -m 0755 "$b/audio-target/release/rustsimhub-audio-alsa" "$pub/input/rustsimhub-audio-alsa"
install -m 0644 "$b/src/licenses/Opus-COPYING.txt" "$pub/input/Opus-COPYING.txt"
image=$(cat "$b/audio-builder-image.txt")
docker run --rm --network none --read-only --cap-drop ALL --cap-add DAC_OVERRIDE --security-opt no-new-privileges --user 0:0 --memory 128m --pids-limit 16 --mount "type=bind,src=$pub,dst=/pub" "$image" /pub/input/rustsimhub-update keygen /pub/key
for epoch in 1 2 3;do
 docker run --rm --network none --read-only --cap-drop ALL --cap-add DAC_OVERRIDE --security-opt no-new-privileges --user 0:0 --memory 128m --pids-limit 16 --mount "type=bind,src=$pub,dst=/pub" "$image" /pub/input/rustsimhub-update make /pub/key/release.seed SOURCE "$epoch" /pub/input "/pub/package$epoch"
done
export=$(mktemp -d "$b/group-export-private-XXXXXXXX")
install -m 0600 "$pub/key/release.public" "$export/release.public"
for e in 1 2 3;do tar -C "$pub/package$e" -czf "$export/package$e.tar.gz" manifest.json manifest.sig rustsimhub-core simhubctl rustsimhub-edge rustsimhub-audio-alsa Opus-COPYING.txt;done
chown -R 1000:1000 "$export"
printf 'EXPORT=%s\\nIMAGE=%s\\n' "$export" "$image"
'''.replace('SOURCE',q(sha))
 verify_inputs='\n'.join('echo '+q(m[n+'_sha256']+'  /mnt/docker-data/rustsimhub/p0/src/target/production/'+n)+' | sha256sum -c -' for n in ['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-update'])
 _,raw=run(nas,verify_inputs+'\n'+text,True);match=re.search(r'EXPORT=(/mnt/docker-data/rustsimhub/p0/group-export-private-[A-Za-z0-9]{8})\nIMAGE=(sha256:[a-f0-9]{64})',raw);assert match;export,image=match.groups();report['nas_export']=export
 _,raw=run(nas,'d=$(mktemp -d /mnt/docker-data/rustsimhub/p0/group-signer-private-XXXXXXXX);printf "%s\\n" "$d"');signer=raw.splitlines()[-1];assert re.fullmatch(r'/mnt/docker-data/rustsimhub/p0/group-signer-private-[A-Za-z0-9]{8}',signer);ns.put(str(a.ctl),signer+'/simhubctl');ns.chmod(signer+'/simhubctl',0o755)
 _,raw=run(dev,'d=$(mktemp -d /var/tmp/rustsimhub-service-group-XXXXXXXX);chmod 0711 "$d";mkdir -m 0700 "$d/store";cp -a '+q(source+'/state')+' '+q(source+'/identity')+' "$d/";install -d -o rustsimhub-core -g rustsimhub-ipc -m 2750 "$d/runtime";install -d -o rustsimhub-edge -g rustsimhub-edge -m 0700 "$d/edge" "$d/health";printf "BASE=%s\\n" "$d";id -u rustsimhub-edge;id -g rustsimhub-edge;id -u rustsimhub-core')
 match=re.search(r'BASE=(/var/tmp/rustsimhub-service-group-[A-Za-z0-9]{8})\n([0-9]+)\n([0-9]+)\n([0-9]+)',raw);assert match;base,edgeuid,edgegid,coreuid=match.groups();report['device_directory']=base;save()
 write(base+'/group.marker',b'RustSimHub.private-service-group.v1\n');write(base+'/device.pin',pin.encode())
 for script in ['device-service-group.sh','device-group-health.sh']:write(base+'/'+script,(ROOT/'tools'/script).read_bytes(),mode=0o700);run(dev,'sh -n '+q(base+'/'+script))
 with ns.open(export+'/release.public','rb') as f:write(base+'/release.public',f.read(33))
 local=ROOT/'.private/group-packages';local.mkdir(parents=True,exist_ok=True)
 for epoch in [1,2,3]:
  path=local/('package'+str(epoch)+'-'+uuid.uuid4().hex+'.tar.gz');ns.get(export+'/package'+str(epoch)+'.tar.gz',str(path));package=base+'/package'+str(epoch);run(dev,'mkdir -m 0700 '+q(package))
  with tarfile.open(path,'r:gz') as tar:
   entries=tar.getmembers();assert len(entries)==7 and {e.name for e in entries}==set(names+['manifest.json','manifest.sig']);assert all(e.isfile() and 0<e.size<=24*1024*1024 for e in entries);assert sum(e.size for e in entries)<=24*1024*1024+8192+64
   blobs={e.name:tar.extractfile(e).read() for e in entries};manifest=json.loads(blobs['manifest.json']);assert manifest['source_sha256']==sha and manifest['epoch']==epoch and manifest['development_fixtures'] is False
   for item in manifest['files']:assert hashlib.sha256(blobs[item['name']]).hexdigest()==item['sha256']
   for name,blob in blobs.items():write(package+'/'+name,blob)
 socket=base+'/runtime/core.sock';core=directory+'/rustsimhub-core';ctl=directory+'/simhubctl';updater=directory+'/rustsimhub-update'
 run(dev,'systemd-run --unit '+setup+' --collect -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc -p CapabilityBoundingSet= -p NoNewPrivileges=yes -p PrivateDevices=yes -p ProtectSystem=strict -p '+q('ReadWritePaths='+base+'/state '+base+'/identity '+base+'/runtime')+' -p RuntimeMaxSec=120 -p TimeoutStopSec=6 -p MemoryMax=96M -p TasksMax=16 '+core+' --serve-call-observing '+q(socket)+' '+edgeuid+' '+q(base+'/state')+' '+q(base+'/identity'));started=True
 run(dev,'n=0;while ! test -S '+q(socket)+';do n=$((n+1));test "$n" -lt 15;sleep 1;done')
 def rpc(body):
  payload=json.dumps({'protocol':2,'id':str(uuid.uuid4()),'command':body},separators=(',',':'));delimiter='RSH_PRIVATE_GROUP_RPC_END';_,raw=run(dev,'runuser -u rustsimhub-edge -- '+core+' --request-auth '+q(socket)+" <<'"+delimiter+"'\n"+payload+'\n'+delimiter);return json.loads(raw.splitlines()[-1])['result']
 challenge=rpc({'op':'challenge','public_key':public});payload=json.dumps(challenge,separators=(',',':'));delimiter='RSH_PRIVATE_GROUP_SIGN_END'
 sign='docker run --rm -i --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 1000:1000 --memory 96m --pids-limit 16 --mount '+q('type=bind,src='+owner+',dst=/owner,readonly')+' --mount '+q('type=bind,src='+signer+',dst=/cli,readonly')+' '+q(image)+' /cli/simhubctl '
 _,raw=run(nas,sign+'sign /owner/key '+q(pin)+" login <<'"+delimiter+"'\n"+payload+'\n'+delimiter,True);token=rpc({'op':'authenticate','challenge_id':challenge['id'],'signature':json.loads(raw.splitlines()[-1])})['token']
 assert rpc({'op':'health','token':token})['ok'];assert rpc({'op':'open_pairing','token':token})['ok']
 _,raw=run(dev,'runuser -u rustsimhub-edge -- '+ctl+' keygen '+q(base+'/health/key'));key=json.loads(raw.splitlines()[-1])
 _,raw=run(dev,'runuser -u rustsimhub-edge -- '+ctl+' pair '+q(socket)+' '+q(base+'/health/key')+' '+q(pin)+' client status.read');pair=json.loads(raw.splitlines()[-1]);approved=rpc({'op':'approve','token':token,'pairing_id':pair['pairing_id'],'public_key':key['public_key'],'kind':'client','scopes':['status.read']});assert approved['scopes']==1
 run(dev,'systemctl stop '+setup);started=False
 run(dev,'runuser -u rustsimhub-edge -- '+directory+'/rustsimhub-edge --init-tls '+q(base+'/edge/tls')+' 10.55.0.2')
 config={'listen':'10.55.0.2:8447','origin':'https://10.55.0.2:8447','core_socket':socket,'core_uid':int(coreuid),'tls_directory':base+'/edge/tls'};write(base+'/edge/config.json',json.dumps(config).encode(),int(edgeuid),int(edgegid))
 def group(command):run(dev,'sh '+q(base+'/device-service-group.sh')+' '+command+' '+q(base)+' '+q(updater))
 def health():
  for unit,uid,name,blocked,user in [('rustsimhub-service-group-core-p6',coreuid,'rustsimhub-core',[base+'/edge/tls/key.der',base+'/health/key/client.seed'],'rustsimhub-core'),('rustsimhub-service-group-edge-p6',edgeuid,'rustsimhub-edge',[base+'/identity/identity.seed',base+'/health/key/client.seed'],'rustsimhub-edge')]:
   _,raw=run(dev,'pid=$(systemctl show '+unit+' -p MainPID --value);test "$pid" -gt 1;cat /proc/"$pid"/status;sha256sum /proc/"$pid"/root/opt/rustsimhub/selected/'+name)
   uidline=re.search(r'^Uid:\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)$',raw,re.M);assert uidline and all(x==uid for x in uidline.groups())
   for kind in ['CapEff','CapPrm','CapAmb','CapBnd']:assert re.search('^'+kind+r':\s+0+$',raw,re.M)
   assert m[name+'_sha256'] in raw
   for path in blocked:run(dev,'pid=$(systemctl show '+unit+' -p MainPID --value);test "$pid" -gt 1;nsenter -t "$pid" -m -- runuser -u '+user+' -- test ! -r '+q(path))
  _,raw=run(dev,'sh '+q(base+'/device-group-health.sh')+' '+q(base)+' '+q(ctl));assert '"local_signed_health":true' in raw
  request="printf 'GET / HTTP/1.1\\r\\nHost: 10.55.0.2:8447\\r\\nConnection: close\\r\\n\\r\\n'"
  _,raw=run(dev,request+' | timeout 10 openssl s_client -connect 10.55.0.2:8447 -CAfile '+q(base+'/edge/tls/cert.pem')+' -verify_ip 10.55.0.2 -verify_return_error -quiet -ignore_unexpected_eof 2>/dev/null');assert raw.startswith('HTTP/1.1 200') and 'RustSimHub' in raw
 def stage(epoch):run(dev,updater+' stage '+q(base+'/store')+' '+q(base+'/release.public')+' '+q(base+'/package'+str(epoch)))
 def selected():
  with ds.open(base+'/selected.hash','r') as f:value=f.read(65).decode().strip()
  assert re.fullmatch('[a-f0-9]{64}',value);return value
 def confirm():run(dev,updater+' confirm '+q(base+'/store')+' '+q(base+'/release.public')+' '+q(selected()))
 stage(1);group('start');health();factory=selected();confirm();group('stop');report['signed_nonroot_group_local_health']=True;save()
 stage(2);group('start');candidate=selected();assert candidate!=factory
 run(dev,'systemctl kill --kill-whom=main --signal=KILL rustsimhub-service-group-core-p6');time.sleep(1);group('stop');group('start');assert selected()==factory;health();group('stop');report['candidate_crash_group_fallback']=True;save()
 stage(3);group('start');health();third=selected();confirm();group('stop');group('start');assert selected()==third;health();group('stop')
 report.update({'confirmed_restart_preserved':True,'least_status_only_health_scope':True,'independent_of_nas_health':True,'actual_uid_empty_capabilities_and_mount_isolation_verified':True,'passed':True});save();print('Private signed non-root core/HTTPS group, local scoped health, crash fallback and confirmed restart passed')
finally:
 if dev:
  if started:run(dev,'systemctl stop '+setup,check=False)
  if base:run(dev,'sh '+q(base+'/device-service-group.sh')+' stop '+q(base)+' '+q(directory+'/rustsimhub-update'),check=False)
  rc,_=run(dev,'for n in nassimhub-agent ModemManager NetworkManager rmtfs;do systemctl is-active "$n";done;for s in /proc/asound/card0/pcm*/sub0/status;do test "$(cat "$s")" = closed;done',check=False);report['cleanup_verified']=rc==0
 save()
 for s in [ds,ns,dev,nas]:
  if s:s.close()
