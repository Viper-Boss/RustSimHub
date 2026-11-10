#!/usr/bin/python3
# Synthetic PID1 only, private container. Fixed units launch real programs.
# SPDX-License-Identifier: AGPL-3.0-only
# Disposable test controller only; never installed in a product image.
import sys,os,json,subprocess,time,signal
from pathlib import Path
UNITS=['rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-media.service','rustsimhub-forwarder.service','rustsimhub-nkn-root.service','rustsimhub-ble-recovery.service']
AUX=['rustsimhub-forwarder.socket','rustsimhub-forwarder.timer','rustsimhub-forwarder.path']
PID=Path('/fixture/pids.json')
a=sys.argv[1:]
def pids():
 try:return json.loads(PID.read_text())
 except FileNotFoundError:return {}
def running(pid):
 try:
  state=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[0]
  return state!='Z'
 except FileNotFoundError:return False
def spawn(uid,exe,args,log):
 command=['/fixture/setpriv','--bounding-set=-all','--no-new-privs',f'--reuid={uid}',f'--regid={uid}','--groups=987',exe,*args]
 stream=open('/fixture/'+log,'ab',buffering=0)
 process=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=stream,stderr=stream,close_fds=True)
 if os.environ.get('RSH_WEB4_FORMAL_CLI')=='yes':
  unit={986:UNITS[0],985:UNITS[1],984:UNITS[2]}[uid]
  group=Path('/sys/fs/cgroup/system.slice')/unit
  group.mkdir(exist_ok=True)
  (group/'cgroup.procs').write_text(str(process.pid))
 return process.pid
if 'daemon-reload' in a:raise SystemExit(0)
if 'show' in a:
 unit=a[-1]
 if unit not in UNITS+AUX:raise SystemExit(2)
 props=[a[i+1] for i,v in enumerate(a[:-1]) if v=='-p']
 for v in a:
  if v.startswith('--property='):props.extend(v.removeprefix('--property=').split(','))
 current=pids();pid=current.get(unit,0)
 active=bool(pid and running(pid));pid=pid if active else 0
 drop=Path('/etc/systemd/system/rustsimhub-core.service.d')
 drops=' '.join(str(p) for p in sorted(drop.glob('*.conf'))) if drop.exists() else ''
 role=unit.removeprefix('rustsimhub-').removesuffix('.service')
 values={'Id':unit,'LoadState':'loaded','ActiveState':'active' if active else 'inactive','SubState':'running' if active else 'dead','MainPID':str(pid),'User':'rustsimhub-'+role,'ControlGroup':'/system.slice/'+unit if active else '', 'NeedDaemonReload':'no','InvocationID':f'{pid:032x}' if active else '', 'FragmentPath':'/etc/systemd/system/'+unit,'DropInPaths':drops}
 values.update({'ControlPID':'0','UnitFileState':'static','TriggeredBy':'','Triggers':'','KillMode':'control-group','Delegate':'no','Restart':'no','Job':'0'})
 for prop in props:
  if prop not in values:raise SystemExit(2)
  print(prop+'='+values[prop])
 raise SystemExit(0)
if 'stop' in a:
 current=pids();stopping=[]
 for unit in a[a.index('stop')+1:]:
  if unit not in UNITS:raise SystemExit(2)
  pid=current.pop(unit,None)
  if pid and running(pid):
   try:os.kill(pid,signal.SIGTERM);stopping.append(pid)
   except ProcessLookupError:pass
 deadline=time.monotonic()+3
 while any(running(pid) for pid in stopping):
  if time.monotonic()>deadline:raise SystemExit(3)
  time.sleep(.01)
 PID.write_text(json.dumps(current));raise SystemExit(0)
if 'start' in a:
 if a!=['--system','start',*UNITS[:3]]:raise SystemExit(2)
 current=pids()
 watched=['/','/var','/var/lib','/var/lib/rustsimhub','/var/lib/rustsimhub/rustsimhub-v1','/var/lib/rustsimhub/rustsimhub-v1/core','/var/lib/rustsimhub/rustsimhub-v1/core/state','/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding','/run','/etc','/etc/systemd','/etc/systemd/system','/etc/systemd/system/rustsimhub-core.service.d','/usr','/usr/libexec','/usr/libexec/rustsimhub','/usr/libexec/rustsimhub/product','/etc/rustsimhub','/etc/tmpfiles.d','/etc/polkit-1','/etc/polkit-1/rules.d','/etc/dbus-1/system.d']
 def stamps():
  result={}
  for path in watched:
   try:
    m=os.lstat(path);result[path]=[m.st_dev,m.st_ino,m.st_mode,m.st_uid,m.st_gid,m.st_size,m.st_nlink,m.st_mtime_ns,m.st_ctime_ns]
   except FileNotFoundError:result[path]=None
  return result
 before=stamps()
 # Actual Edge creates a private test TLS identity. Never log key material.
 tls=Path('/var/lib/rustsimhub-edge/tls')
 if not tls.exists():
  r=subprocess.run(['/fixture/setpriv','--bounding-set=-all','--no-new-privs','--reuid=985','--regid=985','--groups=987','/opt/rustsimhub/selected/rustsimhub-edge','--init-tls',str(tls),'localhost'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=2)
  if r.returncode:raise SystemExit(3)
 history=Path('/var/lib/rustsimhub-core/archive/history.sqlite')
 operation='inspect' if history.exists() else 'init'
 prepared=subprocess.run(['/fixture/setpriv','--bounding-set=-all','--no-new-privs','--reuid=986','--regid=986','--groups=987','/fixture/programs/rustsimhub-migrate',operation,str(history.parent)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=2)
 if prepared.returncode:raise SystemExit(3)
 current[UNITS[0]]=spawn(986,'/usr/libexec/rustsimhub/rustsimhub-core-launcher',[],'core.log')
 deadline=time.monotonic()+3
 while not Path('/run/rustsimhub/core/core.sock').exists():
  if time.monotonic()>deadline or not running(current[UNITS[0]]):raise SystemExit(3)
  time.sleep(.01)
 current[UNITS[2]]=spawn(984,'/opt/rustsimhub/selected/rustsimhub-audio-alsa',['--serve-worker','/var/lib/rustsimhub-media/config.json'],'media.log')
 deadline=time.monotonic()+3
 while not Path('/run/rustsimhub/media/control.sock').exists():
  if time.monotonic()>deadline or not running(current[UNITS[2]]):raise SystemExit(3)
  time.sleep(.01)
 current[UNITS[1]]=spawn(985,'/opt/rustsimhub/selected/rustsimhub-edge',['--serve','/var/lib/rustsimhub-edge/config.json'],'edge.log')
 PID.write_text(json.dumps(current))
 after=stamps()
 Path('/fixture/start-deltas.json').write_text(json.dumps({p:[before[p],after[p]] for p in watched if before[p]!=after[p]}))
 raise SystemExit(0)
raise SystemExit(2)
