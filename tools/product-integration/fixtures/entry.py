#!/usr/bin/python3
# SPDX-License-Identifier: AGPL-3.0-only
# Disposable test data and synthetic MM/PID1 only. Production authority stays
# in the original formal programs, signatures, inode guards, Auth and Store.
import os,subprocess,json,time,platform,socket,struct,secrets,hashlib
from pathlib import Path
os.environ['RSH_WEB4_PRIVATE_CONTAINER']='yes'
# Require the formal producer to name this frozen archive before data setup.
source=Path('/fixture/source-archive').read_text().strip()
core='/fixture/programs/rustsimhub-core'
worker='/fixture/programs/rustsimhub-forwarder'
for program in [core,worker]:
 info=json.loads(subprocess.check_output([program,'--build-info'],timeout=5))
 assert info['source_archive_sha256']==source and info['development_fixtures'] is False
mode=json.loads(subprocess.check_output([core,'--product-runtime-compatibility','product-recovery-health-v1'],timeout=5))
assert mode['source_archive_sha256']==source and mode['opens_database'] is False and mode['starts_services'] is False

def mkdir(p,uid=0,gid=0,mode=0o755):
 p=Path(p);p.mkdir(parents=True,exist_ok=True);p.chmod(mode);os.chown(p,uid,gid)
def put(p,b,uid=0,gid=0,mode=0o600):
 p=Path(p);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(b);p.chmod(mode);os.chown(p,uid,gid)
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
if formal:
 assert platform.machine()=='aarch64', 'formal ARM path has no native verification switch'
 generator=subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::prepare_packages','--ignored','--nocapture','--test-threads=1'])
 assert generator.returncode==0
 updater='/fixture/programs/rustsimhub-update'
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
 print('FORMAL_ARM_PRODUCTION_CHAIN_PASSED',flush=True)
 r=type('Result',(),{'returncode':0})()
else:
 r=subprocess.run(['/fixture/update-tests','--exact','admission::product::tests::production_chain','--ignored','--nocapture','--test-threads=1'])
if not formal and r.returncode==0:
 assert Path('/fixture/production-chain-passed').read_bytes()==b'actual-core-launcher-auth-store-migration-health\n'
 print('NATIVE_PRODUCTION_CHAIN_PASSED',flush=True)
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
