#!/usr/bin/env python3
"""Download a completed production build and install in a fresh development namespace."""
import argparse,hashlib,json,pathlib,re,shlex,sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
import paramiko
p=argparse.ArgumentParser(description=__doc__)
for n in ['nas','device','archive','output','metadata']:p.add_argument('--'+n,type=pathlib.Path,required=True)
p.add_argument('--revision',required=True);a=p.parse_args()
assert re.fullmatch('[a-f0-9]{7,12}',a.revision)
names=['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-update','rustsimhub-recovery']
nc=json.loads(a.nas.read_text(encoding='utf-8-sig'));dc=json.loads(a.device.read_text(encoding='utf-8-sig'))
source_hash=hashlib.sha256(a.archive.read_bytes()).hexdigest();q=shlex.quote
def connect(c,sock=None):
 s=paramiko.SSHClient();s.load_host_keys(c.get('known_hosts',str(pathlib.Path.home()/'.ssh/known_hosts')));s.set_missing_host_key_policy(paramiko.RejectPolicy())
 s.connect(c['host'],port=c.get('port',22),username=c['username'],password=c.get('password'),key_filename=c.get('key_filename'),sock=sock,timeout=12,look_for_keys=False,allow_agent=False);return s
def run(s,body,sudo=False):
 i,o,e=s.exec_command("sudo -S -p '' sh -s" if sudo else 'sh -s',timeout=60);o.channel.set_combine_stderr(True)
 if sudo:i.write(nc['password']+'\n')
 i.write('set -eu\numask 077\n'+body+'\n');i.channel.shutdown_write();b=o.read().decode(errors='replace');rc=o.channel.recv_exit_status()
 if rc:raise RuntimeError('Artifact verification/installation failed; do not overwrite or retry blindly')
 return b
nas=dev=None
try:
 nas=connect(nc)
 script='''test "$(docker inspect rustsimhub-build-p0 --format '{{.State.Status}} {{.State.ExitCode}}')" = 'exited 0'
python3 - <<'PY'
import hashlib,json,pathlib,subprocess
base=pathlib.Path('/mnt/docker-data/rustsimhub/p0')
report={'source_archive_sha256':hashlib.sha256((base/'source.tar.gz').read_bytes()).hexdigest()}
for name in NAMES:
 path=base/'src/target/production'/name
 info=json.loads(subprocess.check_output([str(path),'--build-info']))
 assert info['source_archive_sha256']==report['source_archive_sha256']
 if name=='rustsimhub-core':assert info['development_fixtures'] is False
 report[name+'_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
print(json.dumps(report))
PY'''.replace('NAMES',repr(names))
 report=json.loads(run(nas,script,True).splitlines()[-1]);assert report['source_archive_sha256']==source_hash
 a.output.mkdir(parents=True,exist_ok=True)
 with nas.open_sftp() as sf:
  for name in names:
   f=a.output/name
   if f.exists():assert hashlib.sha256(f.read_bytes()).hexdigest()==report[name+'_sha256']
   else:sf.get('/mnt/docker-data/rustsimhub/p0/src/target/production/'+name,str(f))
   assert hashlib.sha256(f.read_bytes()).hexdigest()==report[name+'_sha256']
 channel=nas.get_transport().open_channel('direct-tcpip',(dc['host'],dc.get('port',22)),('127.0.0.1',0),timeout=12);dev=connect(dc,channel)
 target='/opt/rustsimhub/dev-state-p1-'+a.revision
 raw=run(dev,'test ! -e '+q(target)+';d=$(mktemp -d /var/tmp/rustsimhub-artifact-upload-XXXXXXXX);printf "%s\\n" "$d"')
 upload=raw.splitlines()[-1];assert re.fullmatch(r'/var/tmp/rustsimhub-artifact-upload-[A-Za-z0-9]{8}',upload)
 with dev.open_sftp() as sf:
  for name in names:sf.put(str(a.output/name),upload+'/'+name);sf.chmod(upload+'/'+name,0o600)
 lines=['install -d -m 0755 '+q(target)]
 for name in names:
  lines+=['echo '+q(report[name+'_sha256']+'  '+upload+'/'+name)+' | sha256sum -c -',
          'install -m 0755 '+q(upload+'/'+name)+' '+q(target+'/'+name),
          q(target+'/'+name)+' --build-info | grep -Fq '+q('"source_archive_sha256":"'+source_hash+'"')]
 lines+=[q(target+'/rustsimhub-core')+' --build-info | grep -Fq \'"development_fixtures":false\'']
 run(dev,'\n'.join(lines));report['device_directory']=target;report['verified_production']=True
 a.metadata.parent.mkdir(parents=True,exist_ok=True);a.metadata.write_text(json.dumps(report,indent=2),encoding='utf-8')
 print('Completed production build verified, downloaded and installed in a new isolated namespace')
finally:
 if dev:dev.close()
 if nas:nas.close()
