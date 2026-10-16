# SPDX-License-Identifier: AGPL-3.0-only
import os,pathlib,subprocess,tempfile,unittest
OVERLAY=pathlib.Path(__file__).resolve().parents[2]/'firmware/system/overlay'

@unittest.skipUnless(os.geteuid()==0,'isolated numeric owner tests require root')
class InstallationGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.tmp.name)/'state';self.root.mkdir(mode=0o700)
        files={'system/installation.complete':(0,b'RustSimHub.InitialInstallation.v1\n'),'system/device.identity.anchor':(0,b'synthetic public identity'),
               'core/identity/identity.seed':(986,b'a'*72),'core/state/store.sqlite3':(986,b'synthetic db'),'core/setup/setup.code':(986,b'b'*32),
               'edge/installation/config.json':(985,b'config'),'edge/installation/record.json':(985,b'record'),
               'edge/installation/tls/cert.der':(985,b'cert'),'edge/installation/tls/cert.pem':(985,b'PEM'),'edge/installation/tls/key.der':(985,b'key')}
        for name,(uid,data) in files.items():
            path=self.root/name;path.parent.mkdir(mode=0o700,parents=True,exist_ok=True);path.write_bytes(data);path.chmod(0o600);os.chown(path,uid,uid)
    def tearDown(self):self.tmp.cleanup()
    def run_check(self):
        text=(OVERLAY/'usr/local/libexec/rustsimhub-installation-check').read_text()
        text=text.replace('mountpoint -q /var/lib/rustsimhub','true').replace('test "$(findmnt -rn -M /var/lib/rustsimhub -o FSTYPE)" = ext4','true')
        text=text.replace('/var/lib/rustsimhub/rustsimhub-v1',str(self.root))
        return subprocess.run(['sh','-ec',text],capture_output=True,text=True,timeout=5)
    def test_lost_identity_or_db_does_not_pass_boot_guard_or_create_replacements(self):
        self.assertEqual(self.run_check().returncode,0)
        for name in ['core/identity/identity.seed','core/state/store.sqlite3']:
            path=self.root/name;saved=path.with_suffix('.saved');path.rename(saved)
            self.assertNotEqual(self.run_check().returncode,0);self.assertFalse(path.exists());saved.rename(path)
    def test_partial_completion_and_linked_setup_are_rejected(self):
        marker=self.root/'system/installation.complete';marker.write_bytes(b'partial')
        self.assertNotEqual(self.run_check().returncode,0);self.assertEqual(marker.read_bytes(),b'partial')
        marker.write_bytes(b'RustSimHub.InitialInstallation.v1\n')
        code=self.root/'core/setup/setup.code';code.unlink();code.symlink_to(self.root/'core/identity/identity.seed')
        self.assertNotEqual(self.run_check().returncode,0)

    def test_media_copies_must_match_be_private_and_not_silently_downgrade(self):
        edge=self.root/'edge/installation/media.json';media=self.root/'media/config.json'
        media.parent.mkdir(mode=0o700)
        for path,uid in [(edge,985),(media,984)]:
            path.write_bytes(b'synthetic fixed media configuration');path.chmod(0o600);os.chown(path,uid,uid)
        self.assertEqual(self.run_check().returncode,0)
        media.write_bytes(b'different binding');self.assertNotEqual(self.run_check().returncode,0)
        media.write_bytes(edge.read_bytes());media.chmod(0o644);self.assertNotEqual(self.run_check().returncode,0)
        media.chmod(0o600)
        pending=media.with_suffix('.json.pending');pending.write_bytes(b'partial');self.assertNotEqual(self.run_check().returncode,0);pending.unlink()
        edge.unlink();self.assertNotEqual(self.run_check().returncode,0)
        media.unlink();self.assertEqual(self.run_check().returncode,0)

@unittest.skipUnless(os.geteuid()==0,'isolated numeric owner tests require root')
class InitializationAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.root=self.base/'state';self.root.mkdir(mode=0o700)
        for name,uid in [('system',0),('system/ssh-access',0),('core',986),('edge',985)]:
            path=self.root/name;path.mkdir(mode=0o700);os.chown(path,uid,uid)
        self.fake=self.base/'systemctl';self.fake.write_text('#!/bin/sh\nif test "$1" = show;then echo loaded;else exit 0;fi\n');self.fake.chmod(0o755)
    def tearDown(self):self.tmp.cleanup()
    def run_init(self):
        text=(OVERLAY/'usr/local/libexec/rustsimhub-install-initialize').read_text().replace('/usr/local/libexec/rustsimhub-state-check','/bin/true')
        text=text.replace('/var/lib/rustsimhub/rustsimhub-v1',str(self.root)).replace('systemctl ',str(self.fake)+' ')
        return subprocess.run(['sh','-ec',text,'initializer','/missing-synthetic-public','127.0.0.1:8448','127.0.0.1',str(self.base/'handoff')],capture_output=True,text=True,timeout=5)
    def test_active_group_or_existing_completion_stops_before_new_identity(self):
        self.assertNotEqual(self.run_init().returncode,0)
        self.assertFalse((self.root/'core/identity').exists())
        marker=self.root/'system/installation.complete';marker.write_bytes(b'preserve');marker.chmod(0o600)
        self.assertNotEqual(self.run_init().returncode,0);self.assertEqual(marker.read_bytes(),b'preserve')
