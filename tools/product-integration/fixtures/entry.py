#!/usr/bin/python3
# SPDX-License-Identifier: AGPL-3.0-only
# Disposable test data and synthetic MM/PID1 only. Production authority stays
# in the original formal programs, signatures, inode guards, Auth and Store.
import os,subprocess,json,time,platform,socket,struct,secrets,hashlib,fcntl
from pathlib import Path
os.environ['RSH_WEB4_PRIVATE_CONTAINER']='yes'
# Require the formal producer to name this frozen archive before data setup.
source=Path('/fixture/source-archive').read_text().strip()
scenario=Path('/fixture/scenario').read_text().strip()
default=scenario=='default'
core='/fixture/programs/rustsimhub-core'
worker='/fixture/programs/rustsimhub-forwarder'
for program in [core,worker]:
 info=json.loads(subprocess.check_output([program,'--build-info'],timeout=5))
 assert info['source_archive_sha256']==source and info['development_fixtures'] is False
mode=json.loads(subprocess.check_output([core,'--product-runtime-compatibility','legacy-v1' if default else ('product-email-v1' if scenario=='email' else 'product-recovery-health-v1')],timeout=5))
assert mode['source_archive_sha256']==source and mode['opens_database'] is False and mode['starts_services'] is False

def mkdir(p,uid=0,gid=0,mode=0o755):
 p=Path(p);p.mkdir(parents=True,exist_ok=True);p.chmod(mode);os.chown(p,uid,gid)
def put(p,b,uid=0,gid=0,mode=0o600):
 p=Path(p);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b);p.chmod(mode);os.chown(p,uid,gid)
# Match the normal system-bus alias used by the formal zbus system builder.
var_run=Path('/var/run')
var_run.parent.mkdir(parents=True,exist_ok=True)
if not var_run.exists() and not var_run.is_symlink():var_run.symlink_to('/run')
assert var_run.resolve()==Path('/run'), 'private system bus alias required'
for p in ['/var/lib/rustsimhub/rustsimhub-v1/system/app','/opt/rustsimhub','/run/dbus','/etc/rustsimhub']:mkdir(p)
for p,uid in [('/var/lib/rustsimhub-edge',985),('/var/lib/rustsimhub-media',984)]:mkdir(p,uid,uid,0o700)
put('/var/lib/rustsimhub-edge/config.json',json.dumps({'listen':'127.0.0.1:8443','origin':'https://localhost:8443','core_socket':'/run/rustsimhub/core/core.sock','core_uid':986,'tls_directory':'/var/lib/rustsimhub-edge/tls','media_worker_socket':'/run/rustsimhub/media/control.sock'}).encode(),985,985)
put('/var/lib/rustsimhub-media/config.json',json.dumps({'control_socket':'/run/rustsimhub/media/control.sock','core_socket':'/run/rustsimhub/core/media.sock','bind_ip':'127.0.0.1'}).encode(),984,984)
put('/etc/rustsimhub/product-roles.json',b'{"format":1,"roles":{"nkn":{"uid":1201,"gid":1201},"ble":{"uid":1202,"gid":1202}}}')
put('/etc/nsswitch.conf',b'passwd: files\ngroup: files\n',mode=0o644)
users=['root:x:0:0:root:/root:/bin/false'];groups=['root:x:0:','audio:x:29:rustsimhub-media','rustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder']
for role,uid in [('core',986),('edge',985),('media',984),('forwarder',983),('nkn',1201),('ble',1202)]:
 users.append(f'rustsimhub-{role}:x:{uid}:{uid}:fixture:/nonexistent:/usr/sbin/nologin');groups.append(f'rustsimhub-{role}:x:{uid}:')
put('/etc/passwd',('\n'.join(users)+'\n').encode(),mode=0o644);put('/etc/group',('\n'.join(groups)+'\n').encode(),mode=0o644)
put('/etc/machine-id',b'0123456789abcdef0123456789abcdef\n',mode=0o644)
put('/fixture/dbus.conf',b'<busconfig><type>system</type><listen>unix:path=/run/dbus/system_bus_socket</listen><auth>EXTERNAL</auth><policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>')
log=open('/fixture/dbus.log','wb')
bus=subprocess.Popen(['/usr/bin/dbus-daemon','--nofork','--nopidfile','--config-file=/fixture/dbus.conf'],stdout=log,stderr=log)
for _ in range(500):
 if Path('/run/dbus/system_bus_socket').exists():break
 time.sleep(.01)
assert Path('/run/dbus/system_bus_socket').exists() and bus.poll() is None
mm=subprocess.Popen(['/fixture/update-tests','--exact','admission::product::tests::private_mm_bus','--ignored','--nocapture','--test-threads=1'],stdout=open('/fixture/mm.log','wb'),stderr=subprocess.STDOUT)
for _ in range(500):
 if Path('/fixture/mm-ready').exists():break
 time.sleep(.01)
assert Path('/fixture/mm-ready').exists() and mm.poll() is None
formal=os.environ.get('RSH_WEB4_FORMAL_CLI')=='yes'
assert not (formal and scenario=='email'), 'email API chain currently uses the native isolated driver'
if formal and default:
 assert platform.machine()=='aarch64'
 assert subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::prepare_default','--ignored','--nocapture','--test-threads=1']).returncode==0
 app='/var/lib/rustsimhub/rustsimhub-v1/system/app'
 state=json.loads(Path(app+'/state.json').read_bytes());digest=state['confirmed']['hash']
 assert len(digest)==64 and all(c in '0123456789abcdef' for c in digest)
 updater='/fixture/programs/rustsimhub-update'
 subprocess.run([updater,'boot-select',app,'/usr/share/rustsimhub/release.public',app+'/versions/'+digest,'/run/rustsimhub-app','/opt/rustsimhub/versions'],check=True,timeout=30)
 subprocess.run(['/usr/bin/systemctl','--system','start','rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-media.service'],check=True,timeout=10)
 subprocess.run(['/fixture/setpriv','--bounding-set=-all','--no-new-privs','--reuid=985','--regid=985','--groups=987','/fixture/update-tests','--exact','admission::product::tests::inspect_business_readiness','--ignored','--nocapture','--test-threads=1'],check=True,timeout=20)
 assert not Path('/etc/rustsimhub/product-profile.json').exists() and not Path('/run/rustsimhub/update-health/core.sock').exists()
 assert int.from_bytes(Path('/var/lib/rustsimhub-core/state/store.sqlite3').read_bytes()[60:64],'big')==6
 state=json.loads(subprocess.check_output([updater,'inspect-state'],timeout=5))['state']
 assert state['floor']==1 and state['candidate'] is None and state['attempted'] is False
 print('FORMAL_ARM_DEFAULT_CHAIN_PASSED',flush=True)
 r=type('Result',(),{'returncode':0})()
elif formal:
 assert platform.machine()=='aarch64', 'formal ARM path has no native verification switch'
 generator=subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::prepare_packages','--ignored','--nocapture','--test-threads=1'])
 assert generator.returncode==0
 updater='/fixture/programs/rustsimhub-update'
 # Exercise real rejection before admitting a plan. Each attempt must leave
 # actual business bytes and the original candidate ledger unchanged.
 database=Path('/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3')
 state=Path('/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json')
 original_data=database.read_bytes();original_state=state.read_bytes()
 def refused(name):
  completed=subprocess.run([updater,'product-prepare'],capture_output=True,timeout=30)
  assert completed.returncode!=0, name
  assert database.read_bytes()==original_data and state.read_bytes()==original_state, name
  print('FORMAL REJECTION',name,'EXIT',completed.returncode,flush=True)
 for name,path in [('application-signature','/var/lib/rustsimhub/rustsimhub-v1/system/app-inbox/manifest.sig'),
                   ('signed-program-bytes','/var/lib/rustsimhub/rustsimhub-v1/system/app-inbox/rustsimhub-core'),
                   ('installed-trust','/usr/share/rustsimhub/release.public')]:
  target=Path(path);original=target.read_bytes()
  try:
   changed=bytearray(original);changed[-1]^=1;target.write_bytes(changed);refused(name)
  finally:target.write_bytes(original)
 group=Path('/etc/group');original=group.read_bytes()
 try:
  group.write_bytes(original.replace(b'rustsimhub-nkn:x:1201:',b'rustsimhub-nkn:x:1201:root'))
  refused('actual-NSS-supplementary-alias')
 finally:group.write_bytes(original)
 for name,path in [('competing-maintenance','/var/lib/rustsimhub/rustsimhub-v1/system/app/.update.lock'),
                   ('competing-business-writer','/var/lib/rustsimhub/rustsimhub-v1/core/state/.writer.lock')]:
  with open(path,'rb') as held:
   fcntl.flock(held,fcntl.LOCK_EX|fcntl.LOCK_NB);refused(name)
 for command in ['product-prepare','product-apply','product-inspect','product-activate']:
  completed=subprocess.run([updater,command],capture_output=True,timeout=300)
  print('FORMAL',command,'EXIT',completed.returncode,flush=True)
  if completed.returncode:
   print(completed.stderr.decode(errors='replace'));raise SystemExit(completed.returncode)
  report=json.loads(completed.stdout)
  assert report.get('health_verified') is False and report.get('may_confirm') is False
 assert subprocess.run([updater,'product-activate'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
 # Observe real business readiness without producing an updater capability.
 # Close this separate connection, then obtain all fresh guards for exactly
 # one formal confirmation attempt. Never retry an uncertain confirmation.
 deadline=time.monotonic()+20
 ready=False
 while time.monotonic()<deadline:
  try:
   with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
    client.settimeout(2);client.connect('/run/rustsimhub/update-health/core.sock')
    body=json.dumps({'protocol':1,'step':'observe','challenge':secrets.token_hex(32),'connection':None},separators=(',',':')).encode()
    client.sendall(struct.pack('>I',len(body))+body)
    def read_exact(n):
     result=b''
     while len(result)<n:
      part=client.recv(n-len(result))
      if not part:raise RuntimeError('closed health source')
      result+=part
     return result
    size=struct.unpack('>I',read_exact(4))[0];assert 0<size<=16384
    snapshot=json.loads(read_exact(size));ready=snapshot.get('business_ready') is True
  except (OSError,RuntimeError):pass
  if ready:break
  time.sleep(.1)
 assert ready, 'actual Core never became ready'
 confirmed=subprocess.run([updater,'confirm-running-candidate'],capture_output=True,timeout=10)
 print('FORMAL confirm-running-candidate EXIT',confirmed.returncode,confirmed.stdout.decode(errors='replace'),confirmed.stderr.decode(errors='replace'),flush=True)
 assert confirmed.returncode==0
 assert json.loads(confirmed.stdout)['result']=='confirmed'
 assert subprocess.run([updater,'confirm-running-candidate'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0
 state=json.loads(subprocess.check_output([updater,'inspect-state'],timeout=5))
 assert state['state']['floor']==2 and state['state']['candidate'] is None and state['state']['attempted'] is False
 processes=list(Path('/proc').iterdir());assert len(processes)<=4096
 for process in processes:
  if not process.name.isdecimal():continue
  try:status=(process/'status').read_text()
  except FileNotFoundError:continue
  line=next(row for row in status.splitlines() if row.startswith('Uid:'))
  assert not ({983,1201,1202}&set(map(int,line.split()[1:]))), 'an OFF worker is running' 
 subprocess.run(['/usr/bin/systemctl','--system','stop','rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-media.service'],check=True,timeout=5)
 time.sleep(.2)
 stopped=subprocess.run([updater,'product-inspect'],capture_output=True,timeout=30)
 assert stopped.returncode==0, stopped.stderr.decode(errors='replace')
 inspect=json.loads(stopped.stdout)
 assert inspect['data_admission']['comparison']['actual_data']['business_schema']==9
 print('FORMAL post-confirm passive product-inspect EXIT',stopped.returncode,flush=True)
 subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::schema9_backup','--ignored','--nocapture','--test-threads=1'],check=True,timeout=15)
 print('FORMAL_ARM_PRODUCTION_CHAIN_PASSED',flush=True)
 r=type('Result',(),{'returncode':0})()
else:
 r=subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::default_chain' if default else ('admission::product::tests::email_chain' if scenario=='email' else 'admission::product::tests::production_chain'),'--ignored','--nocapture','--test-threads=1'])
if not formal and r.returncode==0:
 marker='default-chain-passed' if default else ('email-chain-passed' if scenario=='email' else 'production-chain-passed')
 assert Path('/fixture/'+marker).exists()
 print('NATIVE_DEFAULT_CHAIN_PASSED' if default else ('NATIVE_EMAIL_CHAIN_PASSED' if scenario=='email' else 'NATIVE_PRODUCTION_CHAIN_PASSED'),flush=True)
for relative in ['core/state/product-upgrade/state.json','system/product-upgrade/recovery9/state.json','system/product-install/active.json']:
 p=Path('/var/lib/rustsimhub/rustsimhub-v1')/relative
 if p.exists():print('PRIVATE JOURNAL',relative,p.read_text())
try:
 raw=Path('/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3').read_bytes()
 print('PRIVATE ACTUAL SCHEMA',int.from_bytes(raw[60:64],'big'))
except FileNotFoundError:pass
if Path('/fixture/start-deltas.json').exists():print('PRIVATE START DELTAS',Path('/fixture/start-deltas.json').read_text())
for name in ['core','edge','media','mm','dbus']:
 p=Path('/fixture/'+name+'.log')
 if p.exists():print(f'PRIVATE {name} LOG:\n'+p.read_text(errors='replace'))
raise SystemExit(r.returncode)
