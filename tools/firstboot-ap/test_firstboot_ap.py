# SPDX-License-Identifier: AGPL-3.0-only
"""Parser/policy tests and Root filesystem fault injection, never a PID1/AP simulation claim."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest import mock

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import firstboot_ap as package
spec=importlib.util.spec_from_file_location('firstboot_runtime',package.MATERIAL/'runtime.py')
runtime=importlib.util.module_from_spec(spec);spec.loader.exec_module(runtime)

def encoded(v):return json.dumps(v,sort_keys=True,separators=(',',':')).encode()+b'\n'
def write(root,name,data=b'',uid=0,mode=0o600):
    p=root/name.lstrip('/');p.parent.mkdir(parents=True,exist_ok=True)
    p.write_bytes(data);os.chown(p,uid,uid);p.chmod(mode);return p

def directory(root,name,uid=0,gid=None,mode=0o700):
    p=root/name.lstrip('/');p.mkdir(parents=True,exist_ok=True);os.chown(p,uid,uid if gid is None else gid);p.chmod(mode);return p

class Policies(unittest.TestCase):
    def test_original_eight_targets_and_contract_unchanged(self):
        self.assertEqual(runtime.TARGETS,package.ap.FILES)
        self.assertEqual(runtime.AP_CONTRACT,package.ap.CONTRACT)
        self.assertEqual(runtime.GENERIC_CONTRACT,json.loads((package.MATERIAL/'contract.json').read_bytes()))
        self.assertEqual(runtime.LATE,{3,4,5})
    def test_duplicate_fields_rejected(self):
        with self.assertRaises(runtime.Refused):runtime.decode(b'{"format":1,"format":1}')
    def test_production_cli_has_no_root_or_command_override(self):
        source=(package.MATERIAL/'runtime.py').read_text()
        self.assertIn("sys.argv[1:] in ([],['--request'],['--inspect'])",source)
        self.assertNotIn('os.environ',source)
    def test_generic_request_precedes_original_identity_exec(self):
        source=(package.ROOT/'firmware/system/overlay/usr/local/libexec/rustsimhub-generic-install').read_text()
        self.assertLess(source.index('installation.complete'),source.index('rustsimhub-firstboot-ap --request'))
        self.assertLess(source.index('rustsimhub-firstboot-ap --request'),source.index('exec /usr/local/libexec/rustsimhub-install-initialize'))
    def test_optional_unit_does_not_depend_on_ap_success(self):
        text=(package.MATERIAL/'rustsimhub-firstboot-ap.service').read_text()
        self.assertIn('Before=rustsimhub-app-select.service',text)
        self.assertNotIn('Requires=rustsimhub-app.target',text)
        self.assertIn('After=rustsimhub-firstboot.service rustsimhub-generic-install.service',text)
        self.assertNotIn('CAP_NET_ADMIN',text)

class Fixture:
    """Synthetic program/material fixture; separate actual CLI coverage is in isolated_actual.py."""
    def __init__(self,root):
        self.root=root;self.f=runtime.Files(root);self.source='a'*64
        for p in ['var','var/lib','usr','usr/share','usr/share/rustsimhub','usr/share/rustsimhub/firstboot-ap','usr/share/rustsimhub/factory-app','usr/libexec','usr/libexec/rustsimhub','etc','etc/systemd','etc/systemd/system','etc/systemd/system/rustsimhub-core.service.d','etc/systemd/system/rustsimhub-app.target.d','etc/tmpfiles.d','etc/rustsimhub','run']:directory(root,'/'+p,mode=0o755)
        for p in [runtime.BASE,runtime.BASE+'/system']:directory(root,p)
        for p in [runtime.BASE+'/core',runtime.BASE+'/core/state',runtime.BASE+'/core/identity',runtime.BASE+'/core/setup']:directory(root,p,986)
        write(root,runtime.BASE+'/system/generic-provision.request',runtime.GENERIC)
        write(root,runtime.BASE+'/system/installation.complete',runtime.COMPLETE)
        write(root,runtime.BASE+'/system/device.identity.anchor',encoded({'device_public_key_hex':'01'*32}))
        write(root,runtime.BASE+'/core/identity/identity.seed',b'i'*72,986)
        write(root,runtime.BASE+'/core/setup/setup.code',b'c'*32,986)
        write(root,runtime.BASE+'/core/state/business.db',b'existing business data',986)
        self.info={name:{'component':name,'target_arch':'aarch64','development_fixtures':False,'source_archive_sha256':self.source,'setup_ap_installation_contract':runtime.AP_CONTRACT} for name in ['core','edge','setup-ap','core-launcher']}
        self.info['core'].update(database_schema=6,service_launch={'format':1,'modes':['device','forwarding_managed','setup_ap']})
        self.info['core-launcher']['launcher_protocol']=1
        files=[];builds={}
        for i,(name,target,mode) in enumerate(runtime.TARGETS):
            raw=(package.ap.MATERIAL/name).read_bytes() if name not in package.ap.BINARIES else ('synthetic program '+name).encode()
            write(root,runtime.SOURCE+'/'+name,raw,mode=0o644)
            if i not in runtime.LATE:write(root,target,raw,mode=mode)
            files.append({'name':name,'target':target,'mode':mode,'bytes':len(raw),'sha256':runtime.sha(raw)})
            if name in package.ap.BINARIES:builds[package.ap.BINARIES[name]]={'sha256':runtime.sha(raw),'info':self.info[package.ap.BINARIES[name]]}
        for name in ['core','edge']:
            raw=('synthetic program '+name).encode();write(root,'/usr/share/rustsimhub/factory-app/rustsimhub-'+name,raw,mode=0o555)
            builds[name]={'sha256':runtime.sha(raw),'info':self.info[name]}
        app=encoded({'format':2,'schema_min':6,'schema_max':6,'source_sha256':self.source});write(root,'/usr/share/rustsimhub/factory-app/manifest.json',app,mode=0o444)
        contract=(package.MATERIAL/'contract.json').read_bytes();write(root,runtime.SOURCE+'/generic-contract.json',contract,mode=0o644)
        self.m={'format':1,'kind':'generic-firstboot-ap','source_sha256':self.source,'factory_manifest_sha256':runtime.sha(app),'builds':builds,'files':files,'generic_contract_sha256':runtime.sha(contract),'installer_revision':'b'*40,'public_installable':False}
        self.save()
    def save(self):write(self.root,runtime.SOURCE+'/manifest.json',encoded(self.m),mode=0o644)
    def request(self):write(self.root,runtime.BASE+'/system/firstboot-ap.request',runtime.REQUEST)

class SyntheticCommands:
    def __init__(self,fixture):self.fixture=fixture;self.events=[];self.fail_at=None
    def stopped(self):
        self.events.append('stopped')
        if self.fail_at=='active':raise runtime.Refused('active fixture')
    def call(self,args,timeout=20):
        self.events.append(tuple(args));x=self.fixture
        if args[1:] and args[1]=='--build-info':return encoded(x.info[{'rustsimhub-core':'core','rustsimhub-edge':'edge','rustsimhub-setup-ap':'setup-ap','rustsimhub-core-launcher':'core-launcher'}[Path(args[0]).name]])
        if args[1]=='verify-package':return b'{}'
        if args[1]=='--prepare-from-core':
            if self.fail_at=='ap':raise runtime.Refused('injected interruption')
            directory(x.root,runtime.BASE+'/system/setup-ap/installation')
            for name in ['config.json','record.json','cert.der']:write(x.root,runtime.BASE+'/system/setup-ap/installation/'+name,('synthetic '+name).encode())
            return b'{}'
        raise AssertionError(args)
    def role(self,role,program,args):
        self.events.append(role);x=self.fixture
        if role=='core':
            write(x.root,runtime.BASE+'/core/setup/binding.json',encoded({'fixture':True}),986)
            return {'device_public_key_hex':'01'*32}
        directory(x.root,runtime.BASE+'/setup-edge/installation',985);directory(x.root,runtime.BASE+'/setup-edge/installation/tls',985)
        for name in ['config.json','record.json','tls/cert.der','tls/cert.pem','tls/key.der']:write(x.root,runtime.BASE+'/setup-edge/installation/'+name,('synthetic '+name).encode(),985)
        return {}

@unittest.skipUnless(os.geteuid()==0,'Root numeric ownership tests run only in an isolated container')
class RootFilesystem(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name);self.x=Fixture(self.root);self.c=SyntheticCommands(self.x)
    def tearDown(self):self.tmp.cleanup()
    def run_new(self):self.x.request();return runtime.run(self.root,self.c)
    def test_prepare_repeat_preserves_identity_code_db_and_private_material(self):
        first=self.run_new();before={p:p.read_bytes() for p in self.root.rglob('*') if p.is_file()}
        self.assertEqual(first,runtime.run(self.root,self.c))
        self.assertTrue(all(p.read_bytes()==raw for p,raw in before.items()))
        self.assertEqual(self.c.events.count('core'),1);self.assertFalse(first['authorization_verified']);self.assertFalse(first['services_started'])
        self.assertEqual(runtime.inspect(self.root)['state'],'prepared')
    def test_old_device_without_request_ignores_even_bad_new_material(self):
        write(self.root,runtime.SOURCE+'/manifest.json',b'bad',mode=0o644)
        self.assertEqual(runtime.run(self.root,self.c)['state'],'not_requested');self.assertEqual(self.c.events,[])
        self.assertFalse((self.root/(runtime.BASE+'/system/firstboot-ap').lstrip('/')).exists())
    def test_old_installed_device_cannot_create_new_request(self):
        with self.assertRaises(runtime.Refused):runtime.run(self.root,self.c,request=True)
        self.assertTrue(self.x.f.absent(runtime.BASE+'/system/firstboot-ap.request'))
    def test_request_is_durable_before_identity_complete(self):
        self.x.f.path(runtime.BASE+'/system/installation.complete').unlink()
        self.assertEqual(runtime.run(self.root,self.c,request=True)['state'],'requested')
        with self.assertRaises(runtime.Refused):runtime.run(self.root,self.c)
        self.assertEqual(self.c.events,[])
        self.assertTrue(self.x.f.absent(runtime.TARGETS[4][1]))
    def test_bad_source_and_contract_refused_before_programs(self):
        for change in ['source','contract']:
            with self.subTest(change=change):
                old=copy.deepcopy(self.x.m)
                if change=='source':self.x.m['source_sha256']='c'*64
                else:self.x.m['builds']['edge']['info']['setup_ap_installation_contract']={}
                self.x.save();self.x.request()
                with self.assertRaises(runtime.Refused):runtime.run(self.root,self.c)
                self.assertEqual(self.c.events,[]);self.x.m=old;self.x.save()
    def test_active_service_refused_without_activation(self):
        self.c.fail_at='active'
        with self.assertRaises(runtime.Refused):self.run_new()
        self.assertNotIn('core',self.c.events);self.assertTrue(self.x.f.absent(runtime.TARGETS[4][1]))
    def test_interruption_before_ap_reopens_without_reset(self):
        self.c.fail_at='ap'
        with self.assertRaises(runtime.Refused):self.run_new()
        code=self.x.f.read(runtime.BASE+'/core/setup/setup.code',986)
        self.assertTrue(self.x.f.absent(runtime.TARGETS[4][1]));self.c.fail_at=None
        self.assertEqual(runtime.run(self.root,self.c)['state'],'prepared')
        self.assertEqual(self.x.f.read(runtime.BASE+'/core/setup/setup.code',986),code)
    def test_interruption_after_first_late_publication_reopens(self):
        self.x.request();original=runtime.Files.put;counter=[0]
        def interrupted(files,path,raw,mode=0o600):
            original(files,path,raw,mode)
            if path==runtime.TARGETS[3][1]:counter[0]+=1;raise OSError('injected interruption')
        with mock.patch.object(runtime.Files,'put',interrupted):
            with self.assertRaises(OSError):runtime.run(self.root,self.c)
        self.assertEqual(counter[0],1);self.assertTrue(self.x.f.absent(runtime.TARGETS[4][1]))
        self.assertEqual(runtime.run(self.root,self.c)['state'],'prepared')
    def test_exact_link_publication_interruption_resumes_but_foreign_link_refuses(self):
        path=runtime.BASE+'/system/publish';pending=path+'.firstboot-pending';raw=b'own pending'
        write(self.root,pending,raw);os.link(self.x.f.path(pending),self.x.f.path(path));self.x.f.put(path,raw)
        self.assertEqual(self.x.f.read(path),raw);self.assertTrue(self.x.f.absent(pending))
        extra=self.root/'foreign';os.link(self.x.f.path(path),extra)
        with self.assertRaises(runtime.Refused):self.x.f.put(path,raw)
    def test_symlink_hardlink_wrong_owner_and_mode_refused(self):
        for attack in ['symlink','hardlink','owner','mode']:
            with self.subTest(attack=attack):
                p=self.x.f.path(runtime.SOURCE+'/generic-contract.json');raw=p.read_bytes();p.unlink()
                if attack=='symlink':other=self.root/'other';other.write_bytes(raw);p.symlink_to(other)
                else:
                    write(self.root,runtime.SOURCE+'/generic-contract.json',raw,mode=0o644)
                    if attack=='hardlink':os.link(p,self.root/'extra')
                    elif attack=='owner':os.chown(p,985,985)
                    else:p.chmod(0o666)
                self.x.request()
                with self.assertRaises((runtime.Refused,OSError)):runtime.run(self.root,self.c)
                p.unlink();write(self.root,runtime.SOURCE+'/generic-contract.json',raw,mode=0o644)
    def test_prepared_material_loss_is_refused_not_regenerated(self):
        self.run_new();self.x.f.path(runtime.BASE+'/setup-edge/installation/tls/key.der').unlink();count=self.c.events.count('edge')
        with self.assertRaises((runtime.Refused,OSError)):runtime.run(self.root,self.c)
        self.assertEqual(self.c.events.count('edge'),count)
    def test_existing_activation_is_never_overwritten(self):
        write(self.root,runtime.TARGETS[4][1],b'other service policy',mode=0o644)
        with self.assertRaises(runtime.Refused):self.run_new()
        self.assertEqual(self.x.f.read(runtime.TARGETS[4][1],mode=0o644),b'other service policy')
    def test_first_error_retained_and_status_contains_no_secret(self):
        self.x.request();runtime.record_failure('FirstError',self.root);runtime.record_failure('LaterError',self.root)
        failure=self.x.f.read(runtime.BASE+'/system/firstboot-ap/first-failure.json')
        self.assertIn(b'FirstError',failure);self.assertNotIn(b'LaterError',failure)
        self.assertEqual(runtime.inspect(self.root),{'state':'unknown','prior_failure_retained':True,'authorization_verified':False,'services_started':False})
    def test_unsafe_parent_never_repaired(self):
        p=self.x.f.path(runtime.BASE+'/core/setup');p.chmod(0o777);self.x.request()
        with self.assertRaises(runtime.Refused):runtime.run(self.root,self.c)
        self.assertEqual(stat.S_IMODE(p.stat().st_mode),0o777)


@unittest.skipUnless(os.geteuid()==0,'Root hook checks require an isolated container')
class RootBoardHook(unittest.TestCase):
    def setUp(self):
        spec=importlib.util.spec_from_file_location('original_firstboot_tests',package.ROOT/'tools/system/test_firstboot.py')
        old=importlib.util.module_from_spec(spec);spec.loader.exec_module(old)
        self.old=old;self.fixture=old.FirstbootTests('test_preparation_preserves_legacy_and_stable_serial_without_enrolment');self.fixture.setUp()
        f=self.fixture
        f.root.mkdir(mode=0o700);(f.root/'system').mkdir(mode=0o700)
        (f.root/'system/generic-provision.request').write_bytes(runtime.GENERIC)
        (f.root/'system/generic-provision.request').chmod(0o600)
        self.layer=f.base/'firstboot-material';self.layer.write_bytes(b'fixed marker only; not an authorization')
        self.run=f.base/'run';self.run.mkdir()
    def tearDown(self):self.fixture.tearDown()
    def execute(self):
        import subprocess
        f=self.fixture
        text=self.old.SCRIPT.read_text().replace('/var/lib/rustsimhub',str(f.state))
        for origin,replacement in [('/usr/share/rustsimhub/system-candidate.json',f.marker),('/usr/local/libexec/rustsimhub-state-check','true'),('/usr/share/rustsimhub/network/usb.nmconnection',f.template),('/usr/share/rustsimhub/firstboot-ap/manifest.json',self.layer),('/run/rustsimhub-setup-ap-core',self.run/'core'),('/run/rustsimhub-setup-ap-root',self.run/'root')]:text=text.replace(origin,str(replacement))
        return subprocess.run(['sh','-c',text],env=f.env,capture_output=True,text=True,timeout=10)
    def test_new_generic_creates_only_private_empty_ap_directories_and_repeats(self):
        self.assertEqual(self.execute().returncode,0)
        f=self.fixture
        for p,uid,gid,mode in [(f.root/'setup-edge',985,985,0o700),(f.root/'core/setup-ap-journal',986,986,0o700),(self.run/'core',986,0,0o2750),(self.run/'root',0,986,0o2750)]:
            m=p.stat();self.assertEqual((m.st_uid,m.st_gid,stat.S_IMODE(m.st_mode)),(uid,gid,mode));self.assertEqual(list(p.iterdir()),[])
        self.assertEqual(self.execute().returncode,0)
        self.assertFalse((f.root/'system/firstboot-ap.request').exists())
    def test_old_complete_without_request_creates_no_ap_directories(self):
        (self.fixture.root/'system/installation.complete').write_bytes(runtime.COMPLETE)
        self.assertEqual(self.execute().returncode,0)
        self.assertFalse((self.fixture.root/'setup-edge').exists());self.assertEqual(list(self.run.iterdir()),[])
    def test_unsafe_channel_link_refuses_without_following(self):
        untouched=self.fixture.base/'untouched';untouched.mkdir();(self.run/'core').symlink_to(untouched)
        self.assertNotEqual(self.execute().returncode,0);self.assertEqual(list(untouched.iterdir()),[])

if __name__=='__main__':unittest.main()
