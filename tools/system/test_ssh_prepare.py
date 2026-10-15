# SPDX-License-Identifier: AGPL-3.0-only
"""Real synthetic Ed25519 host keys in disposable directories; never starts SSH."""
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / 'firmware/system/overlay/usr/local/libexec/rustsimhub-ssh-prepare'


@unittest.skipUnless(os.geteuid() == 0 and shutil.which('ssh-keygen'), 'isolated root and ssh-keygen required')
class SshPrepareTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = pathlib.Path(self.tmp.name)
        self.system = self.base/'system';self.system.mkdir(mode=0o700)
        self.access = self.system/'ssh-access';self.access.mkdir(mode=0o700)
        self.host = self.system/'ssh-host'
        self.pending = self.system/'ssh-host.pending'
        subprocess.run(['ssh-keygen','-q','-t','ed25519','-N','','-C','synthetic maintainer','-f',str(self.base/'client')],check=True)
        self.key = self.access/'authorized_keys'
        self.key.write_bytes((self.base/'client.pub').read_bytes());self.key.chmod(0o600)

    def tearDown(self):
        self.tmp.cleanup()

    def run_prepare(self, guard='true'):
        source = SCRIPT.read_text().replace('/usr/local/libexec/rustsimhub-state-check',guard)
        source = source.replace('/var/lib/rustsimhub/rustsimhub-v1/system',str(self.system))
        return subprocess.run(['sh','-ec',source],capture_output=True,text=True,timeout=10)

    def test_host_identity_persists_and_complete_pending_resumes_without_new_key(self):
        result = self.run_prepare();self.assertEqual(result.returncode,0,result.stderr)
        before = (self.host/'key').read_bytes()
        self.assertEqual(self.run_prepare().returncode,0)
        self.assertEqual((self.host/'key').read_bytes(),before)
        self.host.rename(self.pending)
        self.assertEqual(self.run_prepare().returncode,0)
        self.assertEqual((self.host/'key').read_bytes(),before)
        self.assertFalse(self.pending.exists())

    def test_missing_untrusted_or_multiple_maintenance_keys_never_generate_host_identity(self):
        original = self.key.read_bytes()
        for data in (b'',b'ssh-ed25519 bad\n',original+original,b'command="shell" '+original):
            self.key.write_bytes(data)
            self.assertNotEqual(self.run_prepare().returncode,0)
            self.assertFalse(self.host.exists());self.assertFalse(self.pending.exists())
        self.key.unlink();self.key.symlink_to(self.base/'client.pub')
        self.assertNotEqual(self.run_prepare().returncode,0)
        self.assertFalse(self.host.exists())

    def test_incomplete_conflicting_or_linked_host_material_is_preserved(self):
        self.pending.mkdir(mode=0o700)
        (self.pending/'torn').write_bytes(b'partial')
        self.assertNotEqual(self.run_prepare().returncode,0)
        self.assertEqual((self.pending/'torn').read_bytes(),b'partial')
        (self.pending/'torn').unlink();self.pending.rmdir()
        self.assertEqual(self.run_prepare().returncode,0)
        before = (self.host/'key').read_bytes()
        (self.host/'key.pub').write_bytes((self.base/'client.pub').read_bytes())
        self.assertNotEqual(self.run_prepare().returncode,0)
        self.assertEqual((self.host/'key').read_bytes(),before)
        self.pending.symlink_to(self.host,target_is_directory=True)
        self.assertNotEqual(self.run_prepare().returncode,0)

    def test_mount_guard_wrong_directory_owner_and_hardlinked_access_are_rejected(self):
        self.assertNotEqual(self.run_prepare('false').returncode,0)
        self.assertFalse(self.pending.exists())
        os.chown(self.system,986,986)
        self.assertNotEqual(self.run_prepare().returncode,0)
        os.chown(self.system,0,0)
        os.link(self.key,self.base/'linked-key')
        self.assertNotEqual(self.run_prepare().returncode,0)
        self.assertFalse(self.pending.exists())
