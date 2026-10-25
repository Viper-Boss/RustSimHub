# SPDX-License-Identifier: AGPL-3.0-only
import hashlib,importlib.util,json,os,pathlib,shutil,subprocess,sys,tempfile,unittest
spec=importlib.util.spec_from_file_location('application_package',pathlib.Path(__file__).resolve().parents[1]/'application_package.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

@unittest.skipUnless(shutil.which('openssl'),'OpenSSL required')
class ApplicationPackageTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.package=self.base/'package';self.package.mkdir()
        self.key=self.base/'private.pem';self.pub=self.base/'release.public'
        subprocess.run(['openssl','genpkey','-algorithm','ED25519','-out',str(self.key)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        der=subprocess.check_output(['openssl','pkey','-in',str(self.key),'-pubout','-outform','DER'],stderr=subprocess.DEVNULL);self.pub.write_bytes(der[-32:])
        self.source='a'*64;entries=[]
        for name in module.FILES:
            data=bytearray(64);data[:7]=b'\x7fELF\x02\x01\x01';data[16]=2;data[18]=183;data[52]=64
            if name==module.FILES[-1]:data=bytearray(b'synthetic license')
            (self.package/name).write_bytes(data);entries.append({'name':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()})
        self.manifest={'format':1,'kind':'application','board':'ufi003-v02','arch':'aarch64-linux','epoch':1,'schema_min':5,'schema_max':5,'source_sha256':self.source,'development_fixtures':False,'files':entries}
        self.sign()
    def tearDown(self):self.tmp.cleanup()
    def sign(self,raw=None):
        raw=raw or json.dumps(self.manifest).encode();(self.package/'manifest.json').write_bytes(raw)
        (self.base/'signed').write_bytes(b'RustSimHub.AppManifest.v1\0'+raw)
        subprocess.run(['openssl','pkeyutl','-sign','-inkey',str(self.key),'-rawin','-in',str(self.base/'signed'),'-out',str(self.package/'manifest.sig')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    def verify(self):return module.verify_package(self.package,self.pub,self.source)
    def test_valid_signature_and_wrong_key_or_payload_rejection(self):
        self.assertEqual(self.verify()['epoch'],1)
        public=self.pub.read_bytes();self.pub.write_bytes(b'\0'*32)
        with self.assertRaises(subprocess.CalledProcessError):self.verify()
        self.pub.write_bytes(public);(self.package/module.FILES[0]).write_bytes(b'changed')
        with self.assertRaises(AssertionError):self.verify()
    def test_schema_six_requires_explicit_maintenance_transition(self):
        with self.assertRaises(AssertionError):module.verify_package(self.package,self.pub,self.source,schema=6)
        self.manifest['schema_min']=6;self.manifest['schema_max']=6;self.sign()
        self.assertEqual(module.verify_package(self.package,self.pub,self.source,schema=6)['schema_max'],6)
        with self.assertRaises(AssertionError):self.verify()
    def test_signed_ambiguous_or_unsupported_manifest_is_denied(self):
        for field,value in [('development_fixtures',True),('board','foreign'),('format',True),('schema_min',6)]:
            old=self.manifest[field];self.manifest[field]=value;self.sign()
            with self.assertRaises(AssertionError):self.verify()
            self.manifest[field]=old
        raw=json.dumps(self.manifest).encode().replace(b'"format": 1',b'"format": 2, "format": 1')
        self.sign(raw)
        with self.assertRaises(AssertionError):self.verify()
    def test_links_duplicate_payloads_and_wrong_source_are_denied(self):
        self.source='b'*64
        with self.assertRaises(AssertionError):self.verify()
        self.source='a'*64
        target=self.package/module.FILES[0];target.rename(self.base/'outside');target.symlink_to(self.base/'outside')
        with self.assertRaises(OSError):self.verify()

    def forwarding(self):
        data=(self.package/module.FILES[0]).read_bytes();(self.package/'rustsimhub-forwarder').write_bytes(data)
        self.manifest['format']=2
        self.manifest['files'].insert(4,{'name':'rustsimhub-forwarder','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()});self.sign()
    def test_forwarding_exact_group_signature_worker_and_missing_file(self):
        self.forwarding();self.assertEqual(self.verify()['format'],2)
        worker=self.package/'rustsimhub-forwarder';worker.write_bytes(b'changed')
        with self.assertRaises(AssertionError):self.verify()
        worker.unlink()
        with self.assertRaises(OSError):self.verify()
    def test_forwarding_cross_format_reordering_and_foreign_worker(self):
        self.forwarding();self.manifest['format']=1;self.sign()
        with self.assertRaises(AssertionError):self.verify()
        self.manifest['format']=2;self.manifest['files'][3],self.manifest['files'][4]=self.manifest['files'][4],self.manifest['files'][3];self.sign()
        with self.assertRaises(AssertionError):self.verify()
        self.manifest['files'][3],self.manifest['files'][4]=self.manifest['files'][4],self.manifest['files'][3]
        worker=self.package/'rustsimhub-forwarder';data=bytearray(worker.read_bytes());data[18]=62;worker.write_bytes(data);self.manifest['files'][4]['sha256']=hashlib.sha256(data).hexdigest();self.sign()
        with self.assertRaises(AssertionError):self.verify()

    @unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Root offline composition fixture')
    def test_actual_composition_publishes_both_complete_fixed_groups_and_rejects_reuse(self):
        sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
        spec=importlib.util.spec_from_file_location('compose_signed_fixture',pathlib.Path(__file__).resolve().parents[1]/'compose-system-tree.py')
        compose=importlib.util.module_from_spec(spec);spec.loader.exec_module(compose)
        for format in (1,2):
            if format==2:self.forwarding()
            root=self.base/f'tree{format}';root.mkdir()
            compose.publish_application(root,self.package,self.pub,self.source,5)
            output=root/'usr/share/rustsimhub/factory-app'
            self.assertEqual(set(p.name for p in output.iterdir()),set(module.application_files(format))|{'manifest.json','manifest.sig'})
            for name in module.application_files(format):
                self.assertEqual((output/name).stat().st_mode&0o777,0o444 if name=='Opus-COPYING.txt' else 0o555)
            with self.assertRaises(AssertionError):compose.publish_application(root,self.package,self.pub,self.source,5)
    @unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Root offline composition fixture')
    def test_composition_rejects_changed_worker_before_creating_factory_paths(self):
        sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
        spec=importlib.util.spec_from_file_location('compose_signed_reject',pathlib.Path(__file__).resolve().parents[1]/'compose-system-tree.py')
        compose=importlib.util.module_from_spec(spec);spec.loader.exec_module(compose)
        self.forwarding();(self.package/'rustsimhub-forwarder').write_bytes(b'changed')
        root=self.base/'rejected-tree';root.mkdir()
        with self.assertRaises(AssertionError):compose.publish_application(root,self.package,self.pub,self.source,5)
        self.assertEqual(list(root.iterdir()),[])
