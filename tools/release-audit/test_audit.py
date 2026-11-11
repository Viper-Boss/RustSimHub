# SPDX-License-Identifier: AGPL-3.0-only
"""Owned synthetic inputs only. No device, candidate program or private directory is opened."""
import copy
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock
import audit
import contracts
import public
from safeio import Refused,Tree,parse,sha,read_file

class AuditTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
    def tearDown(self):self.temp.cleanup()
    def write(self,name,data,mode=0o644):
        p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data);p.chmod(mode);return p
    def test_json_duplicate_nonfinite_and_bool_integer_rejected(self):
        for raw in [b'{"x":1,"x":2}',b'{"x":NaN}',b'{"x":1e999}',b'['*1001+b'0'+b']'*1001,b'[] '*1000]:
            with self.assertRaises(Refused):parse(raw,128)
        self.assertFalse(audit.same({'x':True},{'x':1}))
    def test_symlink_parent_hardlink_fifo_and_writable_refused(self):
        p=self.write('data',b'safe')
        with Tree(self.root) as t:self.assertEqual(t.read('data'),b'safe')
        self.root.joinpath('link').symlink_to(p)
        with self.assertRaises(OSError):read_file(self.root/'link')
        os.link(p,self.root/'hard')
        with self.assertRaises(Refused):read_file(p)
        (self.root/'hard').unlink();p.chmod(0o666)
        with self.assertRaises(Refused):read_file(p)
        os.mkfifo(self.root/'fifo')
        with self.assertRaises(Refused):read_file(self.root/'fifo')
        (self.root/'parent').symlink_to(self.root,target_is_directory=True)
        with self.assertRaises(OSError):read_file(self.root/'parent/data')
    def test_file_budget_and_exact_mode(self):
        self.write('data',b'012345')
        with Tree(self.root) as t:
            with self.assertRaises(Refused):t.read('data',5)
            with self.assertRaises(Refused):t.read('data',6,mode=0o444)
            self.assertEqual(t.read('data',6),b'012345')
    def test_private_ancestor_is_refused_before_open(self):
        with mock.patch('safeio.os.open',side_effect=AssertionError('must not open')):
            with self.assertRaises(Refused):Tree(self.root/'.private')
    def test_public_key_and_certificate_are_allowed(self):
        for raw in [b'-----BEGIN PUBLIC KEY-----\n'+b'A'*64+b'\n-----END PUBLIC KEY-----',
                    b'-----BEGIN CERTIFICATE-----\n'+b'A'*64+b'\n-----END CERTIFICATE-----']:
            self.assertFalse(public.inspect('release.public',raw)['private_material_detected'])
    def test_synthetic_secrets_redacted(self):
        examples=[('fixture.pem',b'-----BEGIN PRIVATE KEY-----\n'+b'A'*64+b'\n-----END PRIVATE KEY-----'),
                  ('fixture.der',bytes.fromhex('302e020100300506032b657004220420')+b'X'*32),
                  ('fixture',b'openssh-key-v1\0synthetic'),('fixture.json',b'{"nested":{"password":"SYNTHETIC_ONLY"}}'),
                  ('identity.seed',b'SYNTHETIC_ONLY')]
        for name,raw in examples:
            with self.subTest(name=name),self.assertRaises(Refused) as error:public.inspect(name,raw)
            self.assertNotIn('SYNTHETIC_ONLY',str(error.exception))
    def test_source_archive_checks_real_git_blobs_missing_extra_tamper(self):
        repo=self.root/'repo';repo.mkdir()
        def git(*args):return subprocess.check_output(['git','-C',str(repo),*args],stderr=subprocess.DEVNULL)
        git('init','-q');(repo/'test.txt').write_bytes(b'owned fixture\n');git('add','test.txt')
        git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','synthetic')
        revision=git('rev-parse','HEAD').decode().strip();raw=git('archive','--format=tar',revision)
        archive=self.write('source.tar',raw)
        self.assertEqual(audit.source_archive(repo,archive,revision,sha(raw))['committed_blobs'],1)
        broken=bytearray(raw);offset=raw.index(b'owned fixture');broken[offset]^=1
        bad=self.write('broken.tar',broken)
        with self.assertRaises((Refused,ValueError,AssertionError)):audit.source_archive(repo,bad,revision,sha(broken))
        with self.assertRaises(Refused):audit.source_archive(repo,archive,revision,'0'*64)
    def report(self):return json.loads((Path(__file__).parent/'fixtures/profile-build-legacy.json').read_bytes())
    def test_original_profile_contract_and_negative_mutations(self):
        report=self.report();args=('legacy-v1',report['source_archive_sha256'],report['revision'],audit.arm,audit.ap.CONTRACT)
        self.assertEqual(contracts.check_report(report,*args),(False,False,False,False))
        for key,value in [('profile','product-v1'),('installable',True),('target','x86_64'),('format',True)]:
            bad=copy.deepcopy(report);bad[key]=value
            with self.subTest(key=key),self.assertRaises((Refused,AssertionError)):contracts.check_report(bad,*args)
        bad=copy.deepcopy(report);bad['programs'].pop('rustsimhub-core')
        with self.assertRaises((Refused,AssertionError)):contracts.check_report(bad,*args)
    def test_missing_license_and_tampered_bytes_are_refused(self):
        report=self.report()
        with Tree(self.root) as tree:
            with self.assertRaises(OSError):audit.licenses(tree,report,self.root,report['revision'],False)
        for name,digest in report['licenses'].items():self.write(name,b'different license')
        with Tree(self.root) as tree:
            with self.assertRaises(Refused):audit.licenses(tree,report,self.root,report['revision'],False)
    def test_android_bounded_compressed_banner(self):
        kernel=gzip.compress(b'xxx Linux version 6.12.49-fixture foo\0')
        raw=bytearray(2048);raw[:8]=b'ANDROID!';raw[8:12]=len(kernel).to_bytes(4,'little');raw[36:40]=(2048).to_bytes(4,'little');raw+=kernel
        audit.kernel_contract(bytes(raw),'6.12.49-fixture')
        with self.assertRaises(Refused):audit.kernel_contract(bytes(raw),'other')
        raw[8:12]=(9999999).to_bytes(4,'little')
        with self.assertRaises(Refused):audit.kernel_contract(bytes(raw),'6.12.49-fixture')
    def test_stream_digest_budget_and_deadline(self):
        raw=b'x'*2048;self.write('image',raw);entry={'bytes':len(raw),'sha256':sha(raw)}
        with Tree(self.root) as tree:
            self.assertEqual(audit._large_file(tree,'image',entry,2048),raw)
            with self.assertRaises(Refused):audit._large_file(tree,'image',entry,2047)
            with mock.patch.object(audit.time,'monotonic',side_effect=[0,121]):
                with self.assertRaises(Refused):audit._large_file(tree,'image',entry,2048)
    def test_signed_original_format2_and_tamper(self):
        app=self.root/'application';app.mkdir();files=[];programs={}
        for name in contracts.APP_FILES:
            raw=bytearray(64) if name!='Opus-COPYING.txt' else bytearray(b'owned license')
            if name!='Opus-COPYING.txt':
                raw[:7]=b'\x7fELF\x02\x01\x01';raw[16:18]=(3).to_bytes(2,'little');raw[18:20]=(183).to_bytes(2,'little');raw[52:54]=(64).to_bytes(2,'little')
                programs[name]={'sha256':sha(bytes(raw))}
            self.write('application/'+name,raw)
            files.append({'name':name,'bytes':len(raw),'sha256':sha(raw)})
        source='a'*64;value={'format':2,'kind':'application','board':'ufi003-v02','arch':'aarch64-linux','epoch':1,'schema_min':6,'schema_max':6,'source_sha256':source,'development_fixtures':False,'files':files}
        raw=json.dumps(value).encode();self.write('application/manifest.json',raw)
        key=self.root/'synthetic.pem';public=self.root/'synthetic.der';signed=self.write('message',b'RustSimHub.AppManifest.v1\0'+raw)
        def cmd(*argv):subprocess.run(argv,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        cmd('openssl','genpkey','-algorithm','ED25519','-out',str(key));cmd('openssl','pkey','-in',str(key),'-pubout','-outform','DER','-out',str(public))
        external=self.write('release.public',public.read_bytes()[-32:])
        cmd('openssl','pkeyutl','-sign','-inkey',str(key),'-rawin','-in',str(signed),'-out',str(app/'manifest.sig'))
        args=audit.parser().parse_args(['--repository',str(self.root),'--source','b'*40,'--archive',str(self.root/'source.tar'),'--build-root',str(self.root),'--profile','legacy-v1','--signed-application',str(app),'--release-public',str(external),'--manifest-sha256',sha(raw)])
        component={'programs':programs,'archive_sha256':source}
        self.signed_fixture=(args,component,key)
        self.assertTrue(audit.signed_application(args,component)['signature_verified'])
        self.write('application/rustsimhub-core',b'tampered')
        with self.assertRaises(Refused):audit.signed_application(args,component)
    def test_real_root_signature_cross_binding_and_finite_targets(self):
        import root_materials
        self.test_signed_original_format2_and_tamper()
        args,component,signer=self.signed_fixture
        args.root_materials=self.root/'root-materials';args.root_materials.mkdir()
        args.build_root=self.root/'profile-build';args.build_root.mkdir()
        args.roles_policy=self.write('approved-roles.json',b'{"format":1,"roles":{"nkn":{"uid":1201,"gid":1201},"ble":{"uid":1202,"gid":1202}}}')
        app={'manifest_sha256':args.manifest_sha256}
        rows=[]
        for name in ['profile_config','core_dropin','launcher']:
            raw=('owned '+name+' fixture').encode();folder,file=root_materials.SOURCE_FILES[name]
            target=args.build_root/folder/file;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
            (args.root_materials/(name+'.payload')).write_bytes(raw)
            rows.append({'id':name,'bytes':len(raw),'sha256':sha(raw)})
        bindings={'profile':args.profile,'source_commit':args.source,'source_archive_sha256':component['archive_sha256'],
            'application_manifest_sha256':app['manifest_sha256'],'core_sha256':component['programs']['rustsimhub-core']['sha256'],
            'approved_role_policy_sha256':sha(args.roles_policy.read_bytes())}
        value={'format':1,'kind':'product-maintenance','board':'ufi003-v02','arch':'aarch64-linux','bindings':bindings,'roles':['core','edge','media','forwarder'],'files':rows}
        raw=json.dumps(value).encode();(args.root_materials/'manifest.json').write_bytes(raw);args.root_manifest_sha256=sha(raw)
        message=self.write('root-message',b'RustSimHub.ProductMaintenance.v1\0'+raw)
        subprocess.run(['openssl','pkeyutl','-sign','-inkey',str(signer),'-rawin','-in',str(message),'-out',str(args.root_materials/'manifest.sig')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        def blob(*_):return ('SOURCE_FILES='+repr(root_materials.SOURCE_FILES)).encode()
        self.assertEqual(root_materials.verify(args,component,app,blob,(False,False,False,False))['fixed_targets_checked'],3)
        altered=copy.deepcopy(component);altered['archive_sha256']='c'*64
        with self.assertRaises(Refused):root_materials.verify(args,altered,app,blob,(False,False,False,False))
        (args.root_materials/'launcher.payload').write_bytes(b'tampered')
        with self.assertRaises(Refused):root_materials.verify(args,component,app,blob,(False,False,False,False))
    def system_fixture(self):
        folder=self.root/'published';folder.mkdir()
        def put(name,raw,mode=0o644):
            p=folder/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw);p.chmod(mode)
            return {'path':name,'bytes':len(raw),'sha256':sha(raw)}
        source='b'*40;archive='a'*64;release='6.12.49-fixture'
        kernel=gzip.compress(b'Linux version '+release.encode()+b' compiler fixture ')
        boot=bytearray(2048);boot[:8]=b'ANDROID!';boot[8:12]=len(kernel).to_bytes(4,'little');boot[36:40]=(2048).to_bytes(4,'little');boot+=kernel
        fs=bytearray(2048);fs[1080:1082]=b'\x53\xef'
        artifacts={'boot':put('boot.img',bytes(boot)),'modules':put('modules.tar',b'owned modules'),
          'dtb':put('board.dtb',bytes.fromhex('d00dfeed')+(40).to_bytes(4,'big')+bytes(32)),
          'rootfs':put('rootfs.img',bytes(fs))}
        policies={'firmware_policy':{'format':1,'kind':'extract-own-firmware-v1','extract_at_first_boot':True,'contains_device_firmware':False,'source_commit':source},
          'identity_policy':{'format':1,'kind':'generic-identity-initialization-v1','generate_on_device':True,'identity_in_image':False,'fixed_owner_password':False,'source_commit':source},
          'default_off':{'format':1,'forwarding':False,'nkn':False,'ble':False,'cellular_data':False,'source_commit':source}}
        for role,value in policies.items():artifacts[role]=put(role+'.json',json.dumps(value).encode())
        staged={}
        for name in audit.STAGED:
            row=put('staged/'+name,b'owned source overlay fixture\n',0o755 if name.startswith('usr/') else 0o644);staged[name]=row['sha256']
        index={'format':1,'kind':'web5-system-audit-v1','source_commit':source,'source_archive_sha256':archive,'profile':'legacy-v1','artifacts':artifacts,'staged_files':staged,
          'public_files':[r['path'] for k,r in artifacts.items() if k in policies]+['staged/'+n for n in staged], 'maintenance_trust':{'included':False,'public_key':None}}
        path=self.write('index.json',json.dumps(index).encode())
        args=audit.parser().parse_args(['--repository',str(self.root),'--source',source,'--archive',str(self.root/'source.tar'),'--build-root',str(self.root),'--profile','legacy-v1','--system-root',str(folder),'--system-index',str(path)])
        lock={'kernel':{'release':release,'boot_img_sha256':artifacts['boot']['sha256'],'modules_sha256':artifacts['modules']['sha256']}}
        def blob(repo,revision,name):return json.dumps(lock).encode() if name=='firmware/base-system.lock.json' else b'owned source overlay fixture\n'
        component={'source_commit':source,'archive_sha256':archive}
        return args,index,path,component,blob
    def test_complete_structural_system_set_and_unknown_or_mixed_refusals(self):
        args,index,path,component,blob=self.system_fixture()
        with mock.patch.object(audit,'git_blob',side_effect=blob):
            result=audit.system_audit(args,component)
            self.assertTrue(result['input_set_complete']);self.assertFalse(result['public_secrecy_proven']);self.assertFalse(result['staged_tree_matches_image'])
            for key,value in [('source_commit','c'*40),('kind','unknown'),('format',True)]:
                bad={**index,key:value};path.write_text(json.dumps(bad))
                with self.subTest(key=key),self.assertRaises(Refused):audit.system_audit(args,component)
            path.write_text(json.dumps({**index,'artifacts':{k:v for k,v in index['artifacts'].items() if k!='rootfs'}}))
            with self.assertRaises(Refused):audit.system_audit(args,component)
            path.write_text(json.dumps({**index,'public_files':index['public_files'][:-1]}))
            with self.assertRaises(Refused):audit.system_audit(args,component)
            path.write_text(json.dumps(index));(args.system_root/'staged'/audit.STAGED[0]).write_bytes(b'altered')
            with self.assertRaises(Refused):audit.system_audit(args,component)
    def test_self_use_maintenance_trust_and_public_secrets(self):
        import base64
        args,index,path,component,blob=self.system_fixture()
        key=b'ssh-ed25519 '+base64.b64encode(b'\0\0\0\x0bssh-ed25519\0\0\0\x20'+bytes(32))+b' synthetic-only\n'
        (args.system_root/'maintenance.public').write_bytes(key)
        index['public_files'].append('maintenance.public');index['maintenance_trust']={'included':True,'public_key':'maintenance.public'};path.write_text(json.dumps(index))
        with mock.patch.object(audit,'git_blob',side_effect=blob):
            with self.assertRaises(Refused):audit.system_audit(args,component)
            args.distribution='self-use';self.assertTrue(audit.system_audit(args,component)['maintenance_public_trust_included'])
            (args.system_root/'leak.json').write_bytes(b'{"password":"SYNTHETIC_ONLY"}')
            index['public_files'].append('leak.json');path.write_text(json.dumps(index))
            with self.assertRaises(Refused) as error:audit.system_audit(args,component)
            self.assertEqual(str(error.exception),'private_json_field')
    def test_source_review_real_ancestry_scope_archive_and_docs_only_heads(self):
        import review_sources
        repo=self.root/'source-review';repo.mkdir()
        def git(*args):return subprocess.check_output(['git','-C',str(repo),*args],stderr=subprocess.DEVNULL)
        git('init','-q');git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','--allow-empty','-qm','owned base')
        base=git('rev-parse','HEAD').decode().strip();index={}
        for name,path in [('console','crates/edge/src/web/console.js'),('firstboot','firmware/system/firstboot-ap/runtime.py'),('matrix','tools/product-integration/matrix/run.py')]:
            git('checkout','-qb',name,base)
            target=repo/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(b'owned source fixture')
            git('add',path);git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','owned product')
            product=git('rev-parse','HEAD').decode().strip();archive=sha(git('archive','--format=tar',product))
            doc=repo/'docs/deliveries'/('web5-'+name+'.md');doc.parent.mkdir(parents=True,exist_ok=True);doc.write_bytes(b'owned report')
            git('add',str(doc));git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','owned report')
            index[name]={'base':base,'product':product,'head':git('rev-parse','HEAD').decode().strip(),'archive_sha256':archive}
        self.assertEqual(len(review_sources.review(repo,index)['packages']),3)
        bad=copy.deepcopy(index);bad['console']['archive_sha256']='0'*64
        with self.assertRaises(Refused):review_sources.review(repo,bad)
        # A later code change cannot be presented as a docs-only final HEAD.
        target=repo/'Auth.rs';target.write_bytes(b'out of scope');git('add','Auth.rs');git('-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','unexpected source change')
        bad=copy.deepcopy(index);bad['matrix']['head']=git('rev-parse','HEAD').decode().strip()
        with self.assertRaises(Refused):review_sources.review(repo,bad)
    def test_default_full_scope_missing_signed_input_has_no_green_conclusion(self):
        args=audit.parser().parse_args(['--repository',str(self.root),'--source','b'*40,'--archive',str(self.root/'source.tar'),'--build-root',str(self.root),'--profile','legacy-v1'])
        with mock.patch.object(audit,'profile_audit',return_value={'programs':{}}):
            result=audit.audit(args)
        self.assertFalse(result['audit_passed']);self.assertFalse(result['release_input_complete'])
        for name in ('installable','may_sign','may_confirm','authorization_verified','health_verified','flash_allowed'):self.assertFalse(result[name])
        self.assertEqual(result['findings'][0]['code'],'signed_application_external_inputs_required')

if __name__=='__main__':unittest.main()
