# SPDX-License-Identifier: AGPL-3.0-only
import base64
import hashlib
import importlib.util
import os
import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('maintenance_seed', pathlib.Path(__file__).with_name('prepare-maintenance-seed.py'))
seed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seed)


@unittest.skipUnless(os.geteuid() == 0, 'isolated Root ownership tests required')
class MaintenanceSeedTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = pathlib.Path(self.tmp.name)
        self.base.chmod(0o700)
        subprocess.run(['/usr/bin/ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(self.base/'client')], check=True)
        self.public = self.base/'client.pub'
        self.public.chmod(0o600)
        self.pub = self.public.read_bytes()
        self.digest = hashlib.sha256(self.pub).hexdigest()
        self.output = self.base/'seed'

    def tearDown(self):
        self.tmp.cleanup()

    def prepare(self):
        return seed.prepare(self.public, self.digest, self.output)

    def test_real_host_identity_and_usb_macs_validate_without_copying_client_private_key(self):
        private = (self.base/'client').read_bytes()
        report = self.prepare()
        self.assertEqual(seed.validate_seed(self.output, self.digest)[0], report)
        system = self.output/'rustsimhub-v1/system'
        serial = (system/'usb.serial').read_bytes()
        suffix = hashlib.sha256(serial[:-1]).hexdigest()[:10]
        self.assertEqual(report['usb_host_mac'], '06:' + ':'.join(suffix[i:i+2] for i in range(0, 10, 2)))
        self.assertEqual((system/'ssh-access/authorized_keys').read_bytes(), self.pub)
        for path in self.output.rglob('*'):
            self.assertEqual(path.stat().st_mode & 0o777, 0o700 if path.is_dir() else 0o600)
            if path.is_file():
                self.assertNotEqual(path.read_bytes(), private)
        self.assertFalse(report['device_installed'])
        self.assertFalse(report['live_state_changed'])

    def test_completed_or_partial_output_cannot_be_regenerated_or_overwritten(self):
        self.prepare()
        host = self.output/'rustsimhub-v1/system/ssh-host/key'
        original = host.read_bytes()
        for complete in (True, False):
            if not complete:
                (self.output/'complete.marker').unlink()
            with self.assertRaises(FileExistsError):
                self.prepare()
            self.assertEqual(host.read_bytes(), original)
        with self.assertRaises(FileNotFoundError):
            seed.validate_seed(self.output, self.digest)

    def test_wrong_digest_options_multiline_or_bad_wire_key_stop_before_new_output(self):
        for pub in (b'command="id" '+self.pub, self.pub+self.pub, b'ssh-ed25519 '+base64.b64encode(b'invalid')+b'\n'):
            self.public.write_bytes(pub)
            self.digest = hashlib.sha256(pub).hexdigest()
            with self.assertRaises(ValueError):
                self.prepare()
            self.assertFalse(self.output.exists())
        self.public.write_bytes(self.pub)
        self.digest = '0'*64
        with self.assertRaises(ValueError):
            self.prepare()
        self.assertFalse(self.output.exists())

    def test_linked_or_untrusted_source_and_output_parent_are_refused(self):
        link = self.base/'linked.pub'
        os.link(self.public, link)
        with self.assertRaises(ValueError):
            self.prepare()
        link.unlink()
        self.public.rename(link)
        self.public.symlink_to(link)
        with self.assertRaises(ValueError):
            self.prepare()
        self.public.unlink()
        link.rename(self.public)
        self.public.chmod(0o644)
        with self.assertRaises(ValueError):
            self.prepare()
        self.public.chmod(0o600)
        self.base.chmod(0o755)
        with self.assertRaises(ValueError):
            self.prepare()
        self.base.chmod(0o700)
        self.assertFalse(self.output.exists())

    def test_tampered_host_serial_manifest_or_extra_entry_is_rejected_without_repair(self):
        self.prepare()
        system = self.output/'rustsimhub-v1/system'
        for path, changed in ((system/'ssh-host/key.pub', self.pub), (system/'usb.serial', b'x'*32+b'\n'),
                              (self.output/'manifest.json', b'{}')):
            original = path.read_bytes()
            path.write_bytes(changed)
            with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                seed.validate_seed(self.output, self.digest)
            self.assertEqual(path.read_bytes(), changed)
            path.write_bytes(original)
        (system/'unexpected').write_bytes(b'keep')
        with self.assertRaises(ValueError):
            seed.validate_seed(self.output, self.digest)
        self.assertEqual((system/'unexpected').read_bytes(), b'keep')

    def test_key_generation_failure_preserves_partial_output_without_complete_marker(self):
        with mock.patch.object(seed.subprocess, 'run', side_effect=subprocess.TimeoutExpired('ssh-keygen', 15)):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.prepare()
        self.assertTrue(self.output.exists())
        self.assertFalse((self.output/'complete.marker').exists())
        with self.assertRaises(FileExistsError):
            self.prepare()
