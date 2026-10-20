# SPDX-License-Identifier: AGPL-3.0-only
"""Directory invariants only; actual loop/fsck integration is separately reported on NAS."""
import hashlib,importlib.util,json,os,pathlib,subprocess,tempfile,types,unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('maintenance_state',pathlib.Path(__file__).with_name('build-maintenance-state.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

@unittest.skipUnless(os.geteuid()==0,'isolated numeric owner tests require Root')
class MaintenanceStateTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.base.chmod(0o700)
        self.mount=self.base/'state';self.mount.mkdir(mode=0o700);os.chown(self.mount,101,103)
        self.legacy=self.mount/'legacy';self.legacy.write_bytes(b'old state bytes stay unchanged')
        self.legacy.chmod(0o600);os.chown(self.legacy,101,103)
        subprocess.run(['/usr/bin/ssh-keygen','-q','-t','ed25519','-N','','-f',str(self.base/'client')],check=True)
        public=self.base/'client.pub';public.chmod(0o600)
        self.digest=hashlib.sha256(public.read_bytes()).hexdigest()
        self.seed=self.base/'seed';module.seed.prepare(public,self.digest,self.seed)
    def tearDown(self):self.tmp.cleanup()
    def args(self):
        return types.SimpleNamespace(output=self.base/'output',seed_directory=self.seed,
            expected_public_sha256=self.digest,plan=self.base/'missing-plan',plan_sha256='0'*64,
            capture_report=self.base/'capture',offline_report=self.base/'offline')
    def namespace_stat(self,real_stat):
        def checked(path,*args,**kwargs):
            if str(path)=='/proc/self/ns/mnt':return types.SimpleNamespace(st_ino=2)
            if str(path)=='/proc/1/ns/mnt':return types.SimpleNamespace(st_ino=1)
            return real_stat(path,*args,**kwargs)
        return checked
    def test_real_seed_copy_preserves_all_legacy_links_bytes_metadata_and_external_symlink_target(self):
        outside=self.base/'outside';outside.write_bytes(b'do not read or replace')
        (self.mount/'link').symlink_to(outside)
        os.link(self.legacy,self.mount/'hardlink')
        before=module.snapshot_tree(self.mount);external=outside.stat()
        result=module.install_namespace(self.mount,self.seed,self.digest)
        self.assertEqual(before,module.snapshot_tree(self.mount,exclude_namespace=True))
        self.assertEqual(outside.stat().st_atime_ns,external.st_atime_ns)
        self.assertTrue(result['legacy_entries_unchanged'])
        self.assertEqual((self.mount.stat().st_uid,self.mount.stat().st_gid,self.mount.stat().st_mode&0o777),(0,0,0o700))
        source=self.seed/'rustsimhub-v1/system'
        target=self.mount/'rustsimhub-v1/system'
        for name in ('usb.serial','ssh-host/key','ssh-host/key.pub','ssh-access/authorized_keys'):
            self.assertEqual((source/name).read_bytes(),(target/name).read_bytes())
            self.assertEqual((target/name).stat().st_mode&0o777,0o600)
        self.assertFalse((target/'installation.complete').exists())
    def test_existing_namespace_or_wrong_legacy_root_is_never_replaced_or_repaired(self):
        outside=self.base/'outside';outside.mkdir(mode=0o700)
        namespace=self.mount/'rustsimhub-v1';namespace.symlink_to(outside)
        with self.assertRaises(ValueError):module.install_namespace(self.mount,self.seed,self.digest)
        self.assertTrue(namespace.is_symlink());self.assertEqual(list(outside.iterdir()),[])
        self.assertEqual(self.mount.stat().st_uid,101)
        namespace.unlink();os.chown(self.mount,0,0)
        with self.assertRaises(ValueError):module.install_namespace(self.mount,self.seed,self.digest)
        self.assertFalse(namespace.exists())
    def test_special_state_file_is_rejected_without_opening_or_blocking(self):
        os.mkfifo(self.mount/'fifo',0o600)
        with self.assertRaisesRegex(ValueError,'Special file'):module.snapshot_tree(self.mount)
        self.assertTrue((self.mount/'fifo').exists())
    def test_changed_legacy_metadata_is_detected_and_not_silently_repaired(self):
        original=module.shutil.copytree
        def change(*args,**kwargs):
            value=original(*args,**kwargs);self.legacy.chmod(0o640);return value
        with mock.patch.object(module.shutil,'copytree',side_effect=change):
            with self.assertRaisesRegex(ValueError,'legacy state'):module.install_namespace(self.mount,self.seed,self.digest)
        self.assertEqual(self.legacy.stat().st_mode&0o777,0o640)
    def test_existing_output_is_untouched_and_bad_plan_creates_no_image_or_loop(self):
        args=self.args();args.output.mkdir(mode=0o700)
        sentinel=args.output/'report.json';sentinel.write_bytes(b'keep existing attempt')
        real=os.stat
        with mock.patch.object(module.os,'stat',side_effect=self.namespace_stat(real)),mock.patch.object(module.executor,'Source') as source:
            with self.assertRaises(FileExistsError):module.prepare(args)
            source.assert_not_called()
        self.assertEqual(sentinel.read_bytes(),b'keep existing attempt')
        sentinel.unlink();args.output.rmdir()
        with mock.patch.object(module.os,'stat',side_effect=self.namespace_stat(real)),mock.patch.object(
                module.executor,'Source',side_effect=ValueError('untrusted plan')),mock.patch.object(module.subprocess,'check_output') as command:
            with self.assertRaisesRegex(ValueError,'untrusted plan'):module.prepare(args)
            command.assert_not_called()
        result=json.loads((args.output/'report.json').read_text())
        self.assertFalse(result['passed']);self.assertFalse(result['device_partition_writes'])
        self.assertFalse((args.output/'NSHSTATE.img').exists())
    def test_host_mount_namespace_is_refused_before_output_or_source_access(self):
        args=self.args();real=os.stat
        def same(path,*values,**kwargs):
            if str(path) in ('/proc/self/ns/mnt','/proc/1/ns/mnt'):return types.SimpleNamespace(st_ino=1)
            return real(path,*values,**kwargs)
        with mock.patch.object(module.os,'stat',side_effect=same),mock.patch.object(module.executor,'Source') as source:
            with self.assertRaisesRegex(ValueError,'private mount namespace'):module.prepare(args)
            source.assert_not_called()
        self.assertFalse(args.output.exists())
