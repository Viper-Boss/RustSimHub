#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual existing ARM CLI + new Python runtime in an owned, offline Root container.

Only PID1 service status and role bind namespaces are adapted. This never starts a
listener/NetworkManager/Core server and is not a physical device/PID1/Owner claim test.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from unittest import mock

sys.path.insert(0,str(Path(__file__).resolve().parent))
import firstboot_ap as package
from test_firstboot_ap import runtime,encoded,write,directory

def command(args,uid=0,maximum=32768):
    if uid:args=['/usr/bin/setpriv','--reuid='+str(uid),'--regid='+str(uid),'--clear-groups','--no-new-privs',*args]
    result=subprocess.run(args,stdin=subprocess.DEVNULL,capture_output=True,env={},cwd='/',timeout=35)
    if result.returncode or len(result.stdout)>maximum:
        # Never include key/code/certificate/identity/subprocess output in failure reports.
        raise RuntimeError('isolated actual command refused: '+Path(args[-1]).name+' exit='+str(result.returncode))
    return result.stdout

class ActualCommands:
    def __init__(self,root,qemu,sysroot,info):self.root=root;self.qemu=qemu;self.sysroot=sysroot;self.info=info;self.events=[];self.fail_ap=False
    def real(self,program,args,uid=0):return command([str(self.qemu),'-L',str(self.sysroot),str(program),*map(str,args)],uid)
    def path(self,path):return self.root/path.lstrip('/')
    def call(self,args,timeout=20):
        self.events.append(Path(args[0]).name+':'+args[1])
        if args[1]=='--build-info':return self.real(self.path(args[0]),args[1:])
        if args[1]=='verify-package':return self.real(self.path(args[0]),[args[1],self.path(args[2]),self.path(args[3])])
        if args[1]=='--prepare-from-core':
            if self.fail_ap:raise runtime.Refused('injected before actual AP preparation')
            return self.real(self.path(args[0]),[args[1],*[self.path(p) for p in args[2:5]],*args[5:]])
        raise AssertionError('unexpected command boundary')
    def stopped(self):
        # Explicit synthetic PID1 boundary; no host/session bus is accessed.
        self.events.append('synthetic_pid1_inactive_no_jobs')
    def role(self,role,program,args):
        self.events.append(role)
        source=self.path(runtime.BASE+('/setup-edge' if role=='edge' else '/core'))
        alias=Path('/var/lib/rustsimhub-setup-edge' if role=='edge' else '/var/lib/rustsimhub-core')
        alias.mkdir(parents=True,exist_ok=True)
        command(['/usr/bin/mount','--bind',str(source),str(alias)])
        try:return json.loads(self.real(self.path(program),args,985 if role=='edge' else 986))
        finally:command(['/usr/bin/umount',str(alias)])

def fixture(root,bundle,application,public,programs,source,qemu,sysroot,info):
    root.mkdir(mode=0o755);root.chmod(0o755)
    directory(root,'/var/lib/rustsimhub',mode=0o700)
    for p in ['/var','/var/lib','/usr','/usr/share','/usr/share/rustsimhub','/usr/libexec','/usr/libexec/rustsimhub','/etc','/etc/systemd','/etc/systemd/system','/etc/tmpfiles.d','/etc/rustsimhub','/run']:directory(root,p,mode=0o755)
    factory=root/'usr/share/rustsimhub/factory-app';factory.mkdir(parents=True)
    for p in application.iterdir():
        target=factory/p.name;shutil.copyfile(p,target);target.chmod(0o444 if p.name in ['manifest.json','manifest.sig','Opus-COPYING.txt'] else 0o555)
    write(root,'/usr/share/rustsimhub/release.public',public.read_bytes(),mode=0o444)
    write(root,'/usr/libexec/rustsimhub/rustsimhub-update',(programs/'rustsimhub-update').read_bytes(),mode=0o555)
    layer=package.install(root,bundle,source,package.sha((factory/'manifest.json').read_bytes()))
    package.audit_tree(root,layer)
    # Original firstboot numeric layout, not any real account or mounted device.
    for p in [runtime.BASE,runtime.BASE+'/system']:directory(root,p)
    for p in [runtime.BASE+'/core',runtime.BASE+'/core/state',runtime.BASE+'/core/identity',runtime.BASE+'/core/setup']:directory(root,p,986)
    directory(root,runtime.BASE+'/setup-edge',985)
    directory(root,runtime.BASE+'/core/setup-ap-journal',986)
    directory(root,'/run/rustsimhub-setup-ap-core',986,0,0o2750)
    directory(root,'/run/rustsimhub-setup-ap-root',0,986,0o2750)
    write(root,runtime.BASE+'/system/generic-provision.request',runtime.GENERIC)
    actual=ActualCommands(root,qemu,sysroot,info)
    # Request must precede the original identity installation, not promote old devices.
    assert runtime.run(root,actual,request=True)['state']=='requested'
    try:runtime.run(root,actual)
    except runtime.Refused:pass
    else:raise AssertionError('identity-not-ready unexpectedly accepted')
    assert not (root/runtime.TARGETS[4][1].lstrip('/')).exists()
    value=actual.role('core','/usr/share/rustsimhub/factory-app/rustsimhub-core',['--prepare-device-setup','/var/lib/rustsimhub-core/state','/var/lib/rustsimhub-core/identity','/var/lib/rustsimhub-core/setup'])
    write(root,runtime.BASE+'/system/device.identity.anchor',encoded(value))
    write(root,runtime.BASE+'/system/installation.complete',runtime.COMPLETE)
    return actual,layer

def execute(args):
    assert os.geteuid()==0,'isolated Root container required'
    assert args.work.is_dir() and not any(args.work.iterdir()),'fresh owned work directory required'
    os.umask(0o077)
    # Only this run's disposable Ed25519 application signer, never a release key.
    key=args.work/'synthetic.pem';public_der=args.work/'synthetic.der';public=args.work/'synthetic.public'
    command(['/usr/bin/openssl','genpkey','-algorithm','ED25519','-out',str(key)])
    command(['/usr/bin/openssl','pkey','-in',str(key),'-pubout','-outform','DER','-out',str(public_der)])
    public.write_bytes(public_der.read_bytes()[-32:]);public.chmod(0o600)
    infos={name:json.loads(command([str(args.qemu),'-L',str(args.sysroot),str(args.programs/('rustsimhub-'+name)),'--build-info'])) for name in ['core','edge','setup-ap','core-launcher']}
    source=infos['core']['source_archive_sha256'];application=args.work/'application';application.mkdir()
    from application_package import application_files
    files=[]
    for name in application_files(2):
        raw=args.license.read_bytes() if name=='Opus-COPYING.txt' else (args.programs/name).read_bytes()
        p=application/name;p.write_bytes(raw);p.chmod(0o555 if name!='Opus-COPYING.txt' else 0o444)
        files.append({'name':name,'bytes':len(raw),'sha256':package.sha(raw)})
    app={'format':2,'kind':'application','board':'ufi003-v02','arch':'aarch64-linux','epoch':1,'schema_min':6,'schema_max':9,'source_sha256':source,'development_fixtures':False,'files':files}
    raw=encoded(app);(application/'manifest.json').write_bytes(raw)
    signed=args.work/'synthetic-message';signed.write_bytes(b'RustSimHub.AppManifest.v1\0'+raw)
    command(['/usr/bin/openssl','pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(signed),'-out',str(application/'manifest.sig')])
    bundle=args.work/'bundle'
    manifest=package.build(args.programs/'rustsimhub-core',args.programs/'rustsimhub-edge',args.programs/'rustsimhub-setup-ap',args.programs/'rustsimhub-core-launcher',application,public,bundle,[str(args.qemu),'-L',str(args.sysroot)])
    package.verify(bundle,source,package.sha(raw))
    checks=[];materials=[]
    for number in [1,2]:
        root=args.work/('device-'+str(number));c,layer=fixture(root,bundle,application,public,args.programs,source,args.qemu,args.sysroot,infos)
        f=runtime.Files(root)
        before={name:f.read(runtime.BASE+name,986,maximum=2*1024*1024) for name in ['/core/identity/identity.seed','/core/setup/setup.code','/core/state/store.sqlite3']}
        if number==1:
            c.fail_ap=True
            try:runtime.run(root,c)
            except runtime.Refused:runtime.record_failure('InjectedBeforeAp',root)
            else:raise AssertionError('fault injection did not fail')
            assert f.absent(runtime.TARGETS[4][1]);c.fail_ap=False
            checks.append('actual_core_edge_interruption_before_ap_reopens')
        else:
            original=runtime.Files.put
            def interrupted(files,path,data,mode=0o600):
                original(files,path,data,mode)
                if path==runtime.TARGETS[4][1]:raise OSError('injected after first activation')
            try:
                with mock.patch.object(runtime.Files,'put',interrupted):runtime.run(root,c)
            except OSError:pass
            else:raise AssertionError('fault injection did not fail')
            assert f.absent(runtime.TARGETS[5][1])
            checks.append('actual_prepared_material_interruption_between_activation_dropins_reopens')
        first=runtime.run(root,c);assert first['state']=='prepared' and first['authorization_verified'] is False and first['services_started'] is False
        assert before['/core/identity/identity.seed']==f.read(runtime.BASE+'/core/identity/identity.seed',986)
        assert before['/core/setup/setup.code']==f.read(runtime.BASE+'/core/setup/setup.code',986)
        # Binding preparation may persist the Core durable realm in this owned business DB.
        # Repeat must preserve all bytes, including that legitimate Core-owned update.
        digest=runtime.generated(f);db=f.read(runtime.BASE+'/core/state/store.sqlite3',986,maximum=2*1024*1024)
        repeated=runtime.run(root,c);assert repeated==first and digest==runtime.generated(f)
        assert db==f.read(runtime.BASE+'/core/state/store.sqlite3',986,maximum=2*1024*1024)
        assert 'key.der' not in json.dumps(runtime.inspect(root))
        materials.append([f.read(runtime.BASE+'/core/identity/identity.seed',986),f.read(runtime.BASE+'/setup-edge/installation/tls/cert.der',985),f.read(runtime.BASE+'/setup-edge/installation/tls/key.der',985),f.read(runtime.BASE+'/system/setup-ap/installation/config.json')])
        checks.append('actual_generation_repeat_private_permissions_'+str(number))
        # Existing installation plus historical generic marker cannot create a new request.
        request=f.path(runtime.BASE+'/system/firstboot-ap.request');saved=request.read_bytes();request.unlink()
        old=f.path(runtime.SOURCE+'/manifest.json');saved_manifest=old.read_bytes();old.chmod(0o644);old.write_bytes(b'bad materials on old device')
        assert runtime.run(root,c)['state']=='not_requested'
        old.write_bytes(saved_manifest)
        try:runtime.run(root,c,request=True)
        except runtime.Refused:pass
        else:raise AssertionError('old installed device accepted new request')
        request.write_bytes(saved);request.chmod(0o600)
        # Actual program/material bytes, owner and path safety all rechecked on repeat.
        cert=f.path(runtime.BASE+'/setup-edge/installation/tls/cert.der');cert.chmod(0o644)
        try:runtime.run(root,c)
        except runtime.Refused:pass
        else:raise AssertionError('unsafe generated cert mode accepted')
        cert.chmod(0o600)
        checks.append('old_device_unchanged_and_wrong_private_mode_refused_'+str(number))
    assert all(a!=b for a,b in zip(materials[0],materials[1])),'new devices reused identity/certificate/key/AP credentials'
    checks.append('two_new_devices_have_independent_random_identity_tls_ap_material')
    return {'format':1,'passed':True,'checks':checks,'actual_program_source_sha256':source,'installer_revision':manifest['installer_revision'],'bundle_sha256':package.sha((bundle/'manifest.json').read_bytes()),'actual_arm_cli':True,'synthetic_pid1_service_status':True,'real_pid1_tested':False,'listeners_started':False,'network_actions':False,'owner_claim_tested':False,'public_installable':False}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['programs','qemu','sysroot','license','work']:parser.add_argument('--'+name,type=Path,required=True)
    print(json.dumps(execute(parser.parse_args()),sort_keys=True))
