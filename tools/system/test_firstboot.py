# SPDX-License-Identifier: AGPL-3.0-only
"""Run board preparation in disposable directories, never mounted device state."""
import os
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / 'firmware/system/overlay/usr/local/libexec/rustsimhub-firstboot'


@unittest.skipUnless(os.geteuid() == 0, 'numeric owner checks require an isolated root test run')
class FirstbootTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = pathlib.Path(self.tmp.name)
        self.state = self.base / 'state'
        self.state.mkdir(mode=0o700)
        os.chown(self.state, 101, 103)
        self.legacy = self.state / 'legacy-identity'
        self.legacy.write_bytes(b'synthetic old identity must stay unchanged')
        self.legacy.chmod(0o600)
        os.chown(self.legacy, 101, 103)
        self.marker = self.base / 'candidate.json'
        self.marker.write_text('{"kind":"internal-system-candidate"}')
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        fake_id = self.bin / 'id'
        fake_id.write_text('#!/usr/bin/python3\nimport sys\n'
                           'if sys.argv[1:]==["-u"]:print(0)\n'
                           'else:print({"rustsimhub-core":986,"rustsimhub-edge":985,"rustsimhub-media":984}[sys.argv[2]])\n')
        fake_id.chmod(0o755)
        getent = self.bin / 'getent'
        getent.write_text('#!/bin/sh\nprintf "rustsimhub-ipc:x:987:\\n"\n')
        getent.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin)+':'+os.environ['PATH'])
        self.root = self.state / 'rustsimhub-v1'
        self.serial = self.root / 'system/usb.serial'

    def tearDown(self):
        self.tmp.cleanup()

    def run_prepare(self, state_check='true'):
        text = SCRIPT.read_text().replace('/var/lib/rustsimhub', str(self.state))
        text = text.replace('/usr/share/rustsimhub/system-candidate.json', str(self.marker))
        text = text.replace('/usr/local/libexec/rustsimhub-state-check', state_check)
        script = self.base / 'prepare'
        script.write_text(text)
        return subprocess.run(['sh', str(script)], env=self.env, capture_output=True, text=True, timeout=10)

    def test_preparation_preserves_legacy_and_stable_serial_without_enrolment(self):
        before = self.legacy.read_bytes()
        result = self.run_prepare()
        self.assertEqual(result.returncode, 0, result.stderr)
        value = self.serial.read_bytes()
        self.assertRegex(value, rb'^[0-9a-f]{32}\n$')
        self.assertEqual(self.run_prepare().returncode, 0)
        self.assertEqual(self.serial.read_bytes(), value)
        self.assertEqual(self.legacy.read_bytes(), before)
        self.assertEqual((self.legacy.stat().st_uid, self.legacy.stat().st_gid), (101, 103))
        self.assertEqual(self.state.stat().st_uid, 0)
        for name in ('state', 'identity', 'setup', 'journal', 'archive', 'access', 'compat'):
            directory = self.root / 'core' / name
            self.assertEqual((directory.stat().st_uid, directory.stat().st_gid, directory.stat().st_mode & 0o777), (986, 986, 0o700))
            self.assertEqual(list(directory.iterdir()), [])

    def test_mount_guard_failure_never_changes_state(self):
        self.assertNotEqual(self.run_prepare('false').returncode, 0)
        self.assertEqual(self.state.stat().st_uid, 101)
        self.assertFalse(self.root.exists())

    def test_existing_namespace_link_or_wrong_owner_is_not_repaired(self):
        outside = self.base / 'outside'
        outside.mkdir(mode=0o700)
        self.root.symlink_to(outside, target_is_directory=True)
        self.assertNotEqual(self.run_prepare().returncode, 0)
        self.assertEqual(list(outside.iterdir()), [])
        self.root.unlink()
        self.root.mkdir(mode=0o700)
        os.chown(self.root, 986, 986)
        self.assertNotEqual(self.run_prepare().returncode, 0)
        self.assertEqual(self.root.stat().st_uid, 986)

    def test_synced_pending_serial_resumes_without_generating_another_identity(self):
        self.assertEqual(self.run_prepare().returncode, 0)
        original = self.serial.read_bytes()
        pending = self.serial.with_name('usb.serial.pending')
        self.serial.rename(pending)
        self.assertEqual(self.run_prepare().returncode, 0)
        self.assertEqual(self.serial.read_bytes(), original)
        self.assertFalse(pending.exists())

    def test_invalid_or_linked_serial_is_never_overwritten(self):
        self.assertEqual(self.run_prepare().returncode, 0)
        original = self.serial.read_bytes()
        for value in (b'bad\n', b'a'*32+b'\x00'):
            self.serial.write_bytes(value)
            self.assertNotEqual(self.run_prepare().returncode, 0)
            self.assertEqual(self.serial.read_bytes(), value)
        self.serial.write_bytes(original)
        os.link(self.serial, self.serial.with_name('extra-link'))
        self.assertNotEqual(self.run_prepare().returncode, 0)
        self.assertEqual(self.serial.read_bytes(), original)
