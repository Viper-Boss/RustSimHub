#!/usr/bin/python3 -I
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed generic first-boot preparation. Never opens an AP, resets Owner or starts services."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys

BASE = '/var/lib/rustsimhub/rustsimhub-v1'
SOURCE = '/usr/share/rustsimhub/firstboot-ap'
REQUEST = b'RustSimHub.GenericFirstbootAp.v1\n'
GENERIC = b'RustSimHub.GenericProvision.v1\n'
COMPLETE = b'RustSimHub.InitialInstallation.v1\n'
# Exactly the original setup_ap_package.FILES write set. No new AP installation target.
TARGETS = (
 ('rustsimhub-setup-ap','/usr/libexec/rustsimhub/rustsimhub-setup-ap',0o755),
 ('rustsimhub-setup-ap.service','/etc/systemd/system/rustsimhub-setup-ap.service',0o644),
 ('rustsimhub-setup-edge.service','/etc/systemd/system/rustsimhub-setup-edge.service',0o644),
 ('rustsimhub-setup-ap.tmpfiles','/etc/tmpfiles.d/rustsimhub-setup-ap.conf',0o644),
 ('50-setup-ap-core.conf','/etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf',0o644),
 ('50-setup-ap-target.conf','/etc/systemd/system/rustsimhub-app.target.d/50-setup-ap.conf',0o644),
 ('contract.json','/etc/rustsimhub/setup-ap-contract.json',0o644),
 ('rustsimhub-core-launcher','/usr/libexec/rustsimhub/rustsimhub-core-launcher',0o755),
)
LATE = {3,4,5}
ACTIVATION = {4,5}
AP_CONTRACT = {'format': 1, 'auth_protocol': 2, 'database_schema': 6, 'application_format': 2, 'parent_channel': 1, 'station_trial_channel': 1, 'station_commit_channel': 1, 'material_format': 1, 'owner_adoption': 1, 'core_mode': '--serve-device-forwarding-managed-setup-ap', 'root_mode': '--serve-configurable', 'origin': 'https://192.168.100.1:8443', 'maximum_window_seconds': 3600}
GENERIC_CONTRACT = {'format': 1, 'kind': 'generic-firstboot-ap', 'request': 'RustSimHub.GenericFirstbootAp.v1', 'generic_request': 'RustSimHub.GenericProvision.v1', 'installation_marker': 'RustSimHub.InitialInstallation.v1', 'core_uid': 986, 'edge_uid': 985, 'database_schema': 6, 'application_format': 2, 'owner_authority': 'existing-core-device-window-and-ready-ap-only', 'network_authority': 'existing-core-lease-and-setup-ap-transaction-only', 'maximum_window_seconds': 3600, 'public_installable': False, 'activation': 'fixed-consumers-after-prepared-marker-and-manager-reload'}

class Refused(Exception):
    pass

def require(value, reason):
    if not value: raise Refused(reason)

def sha(data): return hashlib.sha256(data).hexdigest()
def hexadecimal(value,length):
    return isinstance(value,str) and len(value)==length and all(c in '0123456789abcdef' for c in value)
def same_json(value,expected):
    if type(value) is not type(expected):return False
    if isinstance(expected,dict):return set(value)==set(expected) and all(same_json(value[k],v) for k,v in expected.items())
    if isinstance(expected,list):return len(value)==len(expected) and all(same_json(a,b) for a,b in zip(value,expected))
    return value==expected
def stamp(m):
    return (m.st_dev,m.st_ino,m.st_mode,m.st_nlink,m.st_uid,m.st_gid,m.st_size,m.st_mtime_ns,m.st_ctime_ns)
def decode(raw):
    def unique(pairs):
        result={}
        for key,value in pairs:
            require(key not in result,'duplicate JSON field')
            result[key]=value
        return result
    try:return json.loads(raw,object_pairs_hook=unique)
    except (ValueError,UnicodeError):raise Refused('invalid JSON')

class Files:
    """Private test roots use the same checked relative paths; production CLI always uses /."""
    def __init__(self, root):
        self.root=Path(root)
    def path(self, absolute):
        require(absolute.startswith('/') and '..' not in Path(absolute).parts,'fixed absolute path')
        return self.root/absolute.lstrip('/')
    def parents(self, absolute, owners=(0,)):
        relative=Path(absolute.lstrip('/'));current=self.root
        for part in relative.parts[:-1]:
            current=current/part;m=current.lstat()
            require(stat.S_ISDIR(m.st_mode) and m.st_uid in owners and not m.st_mode & 0o022,'unsafe parent')
    def read(self, absolute, uid=0, mode=0o600, maximum=32768, absent=False, links=(1,)):
        try:self.parents(absolute,(0,uid))
        except FileNotFoundError:
            if absent:return None
            raise Refused('missing required parent')
        try:fd=os.open(self.path(absolute),os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK|os.O_CLOEXEC)
        except FileNotFoundError:
            if absent:return None
            raise Refused('missing required file')
        with os.fdopen(fd,'rb') as f:
            a=os.fstat(f.fileno());require(stat.S_ISREG(a.st_mode) and (a.st_uid,a.st_gid,stat.S_IMODE(a.st_mode))==(uid,uid,mode) and a.st_nlink in links and a.st_size<=maximum,'unsafe file')
            raw=f.read(maximum+1);b=os.fstat(f.fileno());require(stamp(a)==stamp(b) and len(raw)<=maximum and stamp(self.path(absolute).lstat())==stamp(a),'file changed')
            return raw
    def absent(self, absolute):
        try:self.path(absolute).lstat()
        except FileNotFoundError:return True
        return False
    def directory(self, absolute, uid=0, gid=None, mode=0o700):
        gid=uid if gid is None else gid;self.parents(absolute,(0,uid));p=self.path(absolute)
        try:p.mkdir(mode=mode);os.chown(p,uid,gid);p.chmod(mode)
        except FileExistsError:pass
        m=p.lstat();require(stat.S_ISDIR(m.st_mode) and (m.st_uid,m.st_gid,stat.S_IMODE(m.st_mode))==(uid,gid,mode),'unsafe directory')
    def put(self, absolute, raw, mode=0o600):
        pending=absolute+'.firstboot-pending'
        if not self.absent(absolute) and not self.absent(pending):
            a=self.path(absolute).lstat();b=self.path(pending).lstat()
            require((a.st_dev,a.st_ino,a.st_nlink)==(b.st_dev,b.st_ino,2),'unproven publication link')
            require(self.read(absolute,mode=mode,maximum=max(32768,len(raw)),links=(2,))==raw,'published bytes differ')
            self.path(pending).unlink()
            fd=os.open(self.path(absolute).parent,os.O_RDONLY|os.O_DIRECTORY);os.fsync(fd);os.close(fd)
        old=self.read(absolute,mode=mode,maximum=max(32768,len(raw)),absent=True)
        if old is not None:require(old==raw,'existing target differs');return
        pending=absolute+'.firstboot-pending'
        saved=self.read(pending,mode=mode,maximum=max(32768,len(raw)),absent=True)
        if saved is None:
            fd=os.open(self.path(pending),os.O_CREAT|os.O_EXCL|os.O_WRONLY|os.O_NOFOLLOW|os.O_CLOEXEC,mode)
            with os.fdopen(fd,'wb') as f:
                os.fchmod(f.fileno(),mode);f.write(raw);f.flush();os.fsync(f.fileno())
        else:require(saved==raw,'pending target differs')
        # Hard-link publication is NOREPLACE; a racing later writer is never overwritten.
        os.link(self.path(pending),self.path(absolute),follow_symlinks=False);self.path(pending).unlink()
        fd=os.open(self.path(absolute).parent,os.O_RDONLY|os.O_DIRECTORY);os.fsync(fd);os.close(fd)

class Commands:
    def call(self, args, timeout=20):
        p=subprocess.run(args,stdin=subprocess.DEVNULL,capture_output=True,env={},cwd='/',timeout=timeout)
        require(p.returncode==0 and len(p.stdout)<=32768,'bounded preparation command failed')
        return p.stdout
    def role(self, role, program, args):
        source=BASE+'/'+('setup-edge' if role=='edge' else 'core')
        alias='/var/lib/rustsimhub-setup-edge' if role=='edge' else '/var/lib/rustsimhub-core'
        command=['/usr/bin/systemd-run','--quiet','--wait','--pipe','--collect','--expand-environment=no',
          '-p','Type=exec','-p','User=rustsimhub-'+role,'-p','Group=rustsimhub-'+role,
          '-p','CapabilityBoundingSet=','-p','AmbientCapabilities=','-p','NoNewPrivileges=yes',
          '-p','PrivateNetwork=yes','-p','PrivateDevices=yes','-p','PrivateTmp=yes','-p','ProtectSystem=strict',
          '-p','ProtectHome=yes','-p','UMask=0077','-p','TemporaryFileSystem=/var/lib:mode=0755',
          '-p','BindPaths='+source+':'+alias,'-p','ReadWritePaths='+alias,
          '-p','RuntimeMaxSec=20','-p','MemoryMax=96M','-p','TasksMax=8','--',program,*args]
        return decode(self.call(command,25))
    def stopped(self):
        for unit in ['rustsimhub-app.target','rustsimhub-app-select.service','rustsimhub-core.service','rustsimhub-edge.service','rustsimhub-media.service','rustsimhub-setup-ap.service','rustsimhub-setup-edge.service']:
            raw=self.call(['/usr/bin/systemctl','show','--value','-p','ActiveState',unit],5).strip()
            require(raw in (b'inactive',b'failed'),'business or AP service already active')
            # Start jobs for these explicitly ordered boot units are expected.
            # Active processes remain forbidden; requiring Job=0 deadlocks first boot.
            for prop in ['MainPID','ControlPID']:
                require(self.call(['/usr/bin/systemctl','show','--value','-p',prop,unit],5).strip() in (b'',b'0'),'service job or process exists')

    def reload(self):
        self.call(['/usr/bin/systemctl','daemon-reload'],10)

def manifest(files):
    raw=files.read(SOURCE+'/manifest.json',mode=0o644)
    value=decode(raw);require(set(value)=={'format','kind','source_sha256','factory_manifest_sha256','builds','files','generic_contract_sha256','installer_revision','public_installable'},'manifest fields')
    require(type(value['format']) is int and value['format']==1 and value['kind']=='generic-firstboot-ap' and value['public_installable'] is False,'explicit firstboot contract')
    require(hexadecimal(value['source_sha256'],64) and hexadecimal(value['factory_manifest_sha256'],64) and hexadecimal(value['installer_revision'],40),'source digest')
    require(set(value['builds'])=={'core','edge','setup-ap','core-launcher'},'exact components')
    for component,record in value['builds'].items():
        require(set(record)=={'sha256','info'} and hexadecimal(record['sha256'],64),'component digest')
        info=record['info'];require(info['component']==component and info['target_arch']=='aarch64' and info['development_fixtures'] is False and info['source_archive_sha256']==value['source_sha256'] and same_json(info['setup_ap_installation_contract'],AP_CONTRACT),'joint program contract')
    require(same_json(value['builds']['core']['info']['database_schema'],6) and same_json(value['builds']['core']['info']['service_launch'],{'format':1,'modes':['device','forwarding_managed','setup_ap']}),'Core capabilities')
    require(same_json(value['builds']['core-launcher']['info']['launcher_protocol'],1),'launcher protocol')
    require(len(value['files'])==8,'exact AP target count')
    for i,(name,target,mode) in enumerate(TARGETS):
        r=value['files'][i];require(set(r)=={'name','target','mode','bytes','sha256'} and (r['name'],r['target'],r['mode'])==(name,target,mode) and type(r['mode']) is int and type(r['bytes']) is int,'fixed AP target')
        data=files.read(SOURCE+'/'+name,mode=0o644,maximum=32*1024*1024)
        require(r['bytes']==len(data) and r['sha256']==sha(data),'material bytes changed')
        if i not in LATE:require(files.read(target,mode=mode,maximum=32*1024*1024)==data,'installed immutable material differs')
    contract=files.read(SOURCE+'/generic-contract.json',mode=0o644)
    require(sha(contract)==value['generic_contract_sha256'] and same_json(decode(contract),GENERIC_CONTRACT),'generic contract changed')
    return value,raw

def run(root=Path('/'),commands=None,request=False):
    require(os.geteuid()==0,'Root required');f=Files(root);c=commands or Commands()
    generic=f.read(BASE+'/system/generic-provision.request',absent=True)
    if generic is None:return {'state':'not_requested','changed':False,'services_started':False}
    require(generic==GENERIC,'generic request differs')
    request_path=BASE+'/system/firstboot-ap.request'
    if not request and f.read(request_path,absent=True) is None:return {'state':'not_requested','changed':False,'services_started':False}
    m,raw=manifest(f)
    if request:
        # No request may convert an already initialized old device into a new enrolment.
        require(f.absent(BASE+'/system/installation.complete'),'already installed device')
        f.put(request_path,REQUEST)
        return {'state':'requested','services_started':False}
    if f.read(request_path,absent=True) is None:return {'state':'not_requested','changed':False,'services_started':False}
    require(f.read(request_path)==REQUEST,'firstboot request differs')
    f.directory(BASE+'/system/firstboot-ap');lock=os.open(f.path(BASE+'/system/firstboot-ap'),os.O_RDONLY|os.O_DIRECTORY|os.O_CLOEXEC);fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    try:return prepare(f,c,m,raw)
    finally:os.close(lock)

def generated(f):
    result={}
    entries=[('/core/identity/identity.seed',986),('/core/setup/setup.code',986),('/core/setup/binding.json',986)]
    entries += [('/setup-edge/installation/'+n,985) for n in ['config.json','record.json','tls/cert.der','tls/cert.pem','tls/key.der']]
    entries += [('/system/setup-ap/installation/'+n,0) for n in ['config.json','record.json','cert.der']]
    for name,uid in entries:
        raw=f.read(BASE+name,uid,maximum=32768);require(raw,'empty generated file');result[name]=sha(raw)
    return result

def prepare(f,c,m,raw):
    require(f.read(BASE+'/system/installation.complete')==COMPLETE,'identity installation incomplete')
    anchor=f.read(BASE+'/system/device.identity.anchor');require(f.read(BASE+'/core/identity/identity.seed',986,maximum=72) is not None,'identity missing')
    require(len(f.read(BASE+'/core/identity/identity.seed',986,maximum=72))==72,'identity shape')
    require(len(f.read(BASE+'/core/setup/setup.code',986,maximum=32))==32,'setup code missing')
    factory='/usr/share/rustsimhub/factory-app';factory_raw=f.read(factory+'/manifest.json',mode=0o444,maximum=131072)
    require(sha(factory_raw)==m['factory_manifest_sha256'],'factory manifest changed')
    app=decode(factory_raw);require(app['source_sha256']==m['source_sha256'] and app['format']==2 and app['schema_min']<=6<=app['schema_max'],'factory source/format/schema')
    c.call(['/usr/libexec/rustsimhub/rustsimhub-update','verify-package','/usr/share/rustsimhub/release.public',factory],20)
    for component,path in [('core',factory+'/rustsimhub-core'),('edge',factory+'/rustsimhub-edge'),('setup-ap',TARGETS[0][1]),('core-launcher',TARGETS[7][1])]:
        record=m['builds'][component];data=f.read(path,mode=0o555 if component in ('core','edge') else 0o755,maximum=32*1024*1024)
        require(sha(data)==record['sha256'],'program changed');info=decode(c.call([path,'--build-info'],10));require(info==record['info'],'actual metadata differs')
    state_path=BASE+'/system/firstboot-ap/state.json';prior=f.read(state_path,absent=True)
    state=decode(prior) if prior else None
    expected={'format':1,'state':'prepared','source_sha256':m['source_sha256'],'manifest_sha256':sha(raw),'anchor_sha256':sha(anchor),'authorization_verified':False,'services_started':False}
    if state:
        expected['generated']=generated(f)
        require(state==expected,'preparation state changed')
        for i in LATE:
            name,target,mode=TARGETS[i];require(f.read(target,mode=mode)==f.read(SOURCE+'/'+name,mode=0o644),'activation changed')
        return expected
    c.stopped()
    for path,uid,gid,mode in [(BASE+'/setup-edge',985,985,0o700),(BASE+'/core/setup-ap-journal',986,986,0o700),(BASE+'/system/setup-ap',0,0,0o700),(BASE+'/system/setup-ap/journal',0,0,0o700),('/run/rustsimhub-setup-ap-core',986,0,0o2750),('/run/rustsimhub-setup-ap-root',0,986,0o2750)]:f.directory(path,uid,gid,mode)
    core=c.role('core',factory+'/rustsimhub-core',['--prepare-device-ap-binding','/var/lib/rustsimhub-core/state','/var/lib/rustsimhub-core/identity','/var/lib/rustsimhub-core/setup'])
    require(core['device_public_key_hex']==decode(anchor)['device_public_key_hex'],'identity changed during binding')
    c.role('edge',factory+'/rustsimhub-edge',['--prepare-setup-local','/var/lib/rustsimhub-setup-edge'])
    c.call([TARGETS[0][1],'--prepare-from-core',BASE+'/system/setup-ap',BASE+'/core/setup',BASE+'/setup-edge/installation','985','986'],20)
    require(f.read(BASE+'/system/device.identity.anchor')==anchor,'identity anchor changed')
    manifest(f);c.stopped()
    # Original tmpfiles and the two activation drop-ins are published only after preparation.
    expected['generated']=generated(f)
    for i in sorted(LATE):
        name,target,mode=TARGETS[i];f.put(target,f.read(SOURCE+'/'+name,mode=0o644),mode)
    c.reload() # PID1 must use the just-published Core ExecStart drop-in before queued boot jobs.
    f.put(state_path,json.dumps(expected,sort_keys=True,separators=(',',':')).encode()+b'\n')
    return expected

def inspect(root=Path('/')):
    require(os.geteuid()==0,'Root required');f=Files(root)
    if f.absent(BASE+'/system/firstboot-ap.request'):return {'state':'not_requested','authorization_verified':False,'services_started':False}
    state=f.read(BASE+'/system/firstboot-ap/state.json',absent=True) if not f.absent(BASE+'/system/firstboot-ap') else None
    failed=not f.absent(BASE+'/system/firstboot-ap/first-failure.json')
    return {'state':'prepared' if state and decode(state).get('state')=='prepared' else ('unknown' if failed else 'requested'),'prior_failure_retained':failed,'authorization_verified':False,'services_started':False}

def record_failure(kind,root=Path('/')):
    try:
        f=Files(root)
        if f.absent(BASE+'/system/firstboot-ap.request'):return
        f.directory(BASE+'/system/firstboot-ap')
        raw=json.dumps({'format':1,'state':'unknown','error':kind,'authorization_verified':False,'services_started':False},sort_keys=True).encode()+b'\n'
        f.put(BASE+'/system/firstboot-ap/first-failure.json',raw)
    except (OSError,Refused):pass # Never repair unsafe state merely to log a failure.

def main():
    try:
        require(sys.argv[1:] in ([],['--request'],['--inspect']),'fixed arguments only')
        result=inspect() if sys.argv[1:]==['--inspect'] else run(request=sys.argv[1:]==['--request']);print(json.dumps({k:result[k] for k in ['state','changed','prior_failure_retained','authorization_verified','services_started'] if k in result},sort_keys=True))
    except (Refused,OSError,subprocess.SubprocessError,KeyError,TypeError) as e:
        # Error codes only: no credential bodies, subprocess output or identity values.
        record_failure(type(e).__name__)
        print(json.dumps({'state':'refused','error':type(e).__name__,'authorization_verified':False,'services_started':False}),file=sys.stderr);raise SystemExit(1)

if __name__=='__main__':main()
