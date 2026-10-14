#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Verify local ARM binaries on NAS and an isolated stick without hardware operations."""
import argparse,hashlib,json,pathlib,re,shlex,sys,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--image',required=True,help='Pinned Debian 13 ARM64 runtime image')
for name in ['nas','device','artifacts','report']:p.add_argument('--'+name,type=pathlib.Path,required=True)
a=p.parse_args();assert not a.report.exists()
m=json.loads((a.artifacts/'build.json').read_text());names=['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-update','rustsimhub-recovery','rustsimhub-migrate']
assert m['target']=='aarch64-unknown-linux-gnu' and re.fullmatch('[0-9a-f]{64}',m['source_archive_sha256'])
for name in names:assert hashlib.sha256((a.artifacts/name).read_bytes()).hexdigest()==m['artifact_sha256'][name]
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'));q=shlex.quote
report={'revision':m['revision'],'source_archive_sha256':m['source_archive_sha256'],'artifact_sha256':m['artifact_sha256'],'new_calls':0,'new_sms':0,'cellular_tests':False,'passed':False}
nas=dev=None;unit='rustsimhub-crosscheck-'+uuid.uuid4().hex[:12]
container=unit;image=a.image
assert re.fullmatch('debian@sha256:[0-9a-f]{64}',image)
def save():a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2),encoding='utf-8')
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy());s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(s,body,sudo=False):
 i,o,e=s.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=90);o.channel.set_combine_stderr(True)
 if sudo:i.write(nc['password']+'\n')
 i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();raw=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc:report['failure_output']=raw;save();raise RuntimeError('Private ARM runtime check failed')
 return raw
def put(s,base):
 with s.open_sftp() as sf:
  for name in names:sf.put(str(a.artifacts/name),base+'/'+name);sf.chmod(base+'/'+name,0o555)
 run(s,'\n'.join('echo '+q(m['artifact_sha256'][name]+'  '+base+'/'+name)+' | sha256sum -c -' for name in names))
def check(raw):
 infos=[json.loads(line) for line in raw.splitlines() if line.startswith('{')]
 assert len(infos)==7
 for info in infos[:6]:assert info['source_archive_sha256']==m['source_archive_sha256']
 assert infos[0]['development_fixtures'] is False
 assert infos[-1]['ok'] and infos[-1]['hardware_mutations'] is False
 return infos
try:
 nas=connect(nc)
 assert run(nas,'docker image inspect '+q(image)+" --format '{{.Architecture}}'",True).splitlines()[-1]=='arm64'
 base=run(nas,'test "$(id -u)" = 1000; mktemp -d /mnt/docker-data/rustsimhub/p0/local-cross-XXXXXXXX').splitlines()[-1]
 assert re.fullmatch('/mnt/docker-data/rustsimhub/p0/local-cross-[A-Za-z0-9]{8}',base)
 put(nas,base);report['nas_directory']=base
 program='; '.join(q('/payload/'+name)+' --build-info' for name in names)+'; /payload/rustsimhub-core --self-check; /payload/simhubctl --help >/dev/null'
 command=['docker','run','--rm','--name',container,'--label','org.rustsimhub.purpose=local-cross-runtime-check','--network','none','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--user','1000:1000','--cpus','1','--memory','128m','--pids-limit','16','--mount','type=bind,src='+base+',dst=/payload,readonly',image,'sh','-ec',program]
 raw=run(nas,shlex.join(command),True);report['nas_runtime']=check(raw)
 sock=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,sock)
 base=run(dev,'mktemp -d /var/tmp/rustsimhub-crosscheck-XXXXXXXX').splitlines()[-1]
 assert re.fullmatch('/var/tmp/rustsimhub-crosscheck-[A-Za-z0-9]{8}',base)
 put(dev,base);run(dev,'chmod 0711 '+q(base));report['device_directory']=base
 program='; '.join(q(base+'/'+name)+' --build-info' for name in names)+'; '+q(base+'/rustsimhub-core')+' --self-check; '+q(base+'/simhubctl')+' --help >/dev/null'
 command=['systemd-run','--quiet','--wait','--pipe','--collect','--unit='+unit,'-p','User=rustsimhub-core','-p','Group=rustsimhub-core','-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes','-p','ProtectSystem=strict','-p','ProtectHome=yes','-p','PrivateDevices=yes','-p','PrivateNetwork=yes','-p','InaccessiblePaths=/run/dbus','-p','RuntimeMaxSec=20','-p','MemoryMax=128M','-p','TasksMax=16','-p','LimitCORE=0','/bin/sh','-ec',program]
 raw=run(dev,shlex.join(command));report['device_runtime']=check(raw)
 run(nas,'if docker inspect '+q(container)+' >/dev/null 2>&1;then exit 10;fi',True)
 raw=run(dev,'if systemctl is-active --quiet '+q(unit)+';then exit 10;fi\nsystemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nfor f in /proc/asound/card*/pcm*/sub*/status;do test "$(sed -n \'1p\' "$f")" = closed;done')
 assert raw.splitlines()==['active']*4
 report['passed']=True;report['cleanup_verified']=True
finally:
 save()
 if dev:dev.close()
 if nas:nas.close()
print('Six local ARM binaries verified on NAS and isolated non-root stick; source hashes and production mode match; no hardware operations')
