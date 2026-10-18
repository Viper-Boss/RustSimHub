# SPDX-License-Identifier: AGPL-3.0-only
import contextlib
import hashlib
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

DIRECTORY = pathlib.Path(__file__).resolve().parents[1] / 'recovery'
spec = importlib.util.spec_from_file_location('edl_readonly', DIRECTORY / 'edl_readonly.py')
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)
with patch.dict(sys.modules, {'edl_readonly': recovery}):
    spec = importlib.util.spec_from_file_location('offline_backup', DIRECTORY / 'verify-offline-backup.py')
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)


@unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0, 'Linux Root fixtures')
class OfflineBackupTests(unittest.TestCase):
    def fixture(self, folder):
        base = pathlib.Path(folder)
        base.chmod(0o700)
        image = base / 'backup.img'
        image.write_bytes(b'first backup contents')
        image.chmod(0o600)
        return base, image

    def test_short_wrong_mode_and_linked_inputs_stop_before_output_or_commands(self):
        with tempfile.TemporaryDirectory() as folder:
            base, image = self.fixture(folder)
            expected = '0' * 64
            with patch.object(verifier.subprocess, 'run', side_effect=AssertionError('No subprocess allowed')):
                with self.assertRaisesRegex(ValueError, 'regular file'):
                    verifier.verify(image, base/'out', expected, expected)
                self.assertFalse((base/'out').exists())
                with patch.object(recovery, 'DISK_BYTES', image.stat().st_size):
                    image.chmod(0o644)
                    with self.assertRaisesRegex(ValueError, 'permissions'):
                        verifier.open_image(image)
                    image.chmod(0o600)
                    os.link(image, base/'hardlink')
                    with self.assertRaisesRegex(ValueError, 'link count'):
                        verifier.open_image(image)
                    (base/'hardlink').unlink()
                    (base/'symlink').symlink_to(image)
                    with self.assertRaisesRegex(ValueError, 'Noncanonical'):
                        verifier.open_image(base/'symlink')

    def test_digest_mismatch_cannot_attach_loop_and_preserves_failed_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            base, image = self.fixture(folder)
            with patch.object(recovery, 'DISK_BYTES', image.stat().st_size), \
                    patch.object(verifier.subprocess, 'check_output', side_effect=AssertionError('No loop allowed')):
                with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                    verifier.verify(image, base/'out', '0'*64, '1'*64)
            report = json.loads((base/'out/report.json').read_text())
            self.assertFalse(report['passed'])
            self.assertEqual(report['filesystems'], {})
            self.assertEqual(image.read_bytes(), b'first backup contents')
            with self.assertRaisesRegex(ValueError, 'preserve'):
                verifier.verify(image, base/'out', '0'*64, '1'*64)
            self.assertEqual(json.loads((base/'out/report.json').read_text()), report)

    def test_pinned_fd_does_not_follow_replaced_path_and_short_reads_fail(self):
        with tempfile.TemporaryDirectory() as folder:
            base, image = self.fixture(folder)
            with patch.object(recovery, 'DISK_BYTES', image.stat().st_size):
                descriptor, before = verifier.open_image(image)
            try:
                image.rename(base/'original')
                image.write_bytes(b'replacement')
                self.assertEqual(verifier.digest_fd(descriptor), hashlib.sha256(b'first backup contents').hexdigest())
                self.assertNotEqual(verifier.signature(image.stat()), before)
                with self.assertRaisesRegex(ValueError, 'Short'):
                    verifier.read_at(descriptor, 10**6, 1)
            finally:
                os.close(descriptor)

    def test_writable_or_wrong_bounds_loop_is_detached_without_running_checker(self):
        with tempfile.TemporaryDirectory() as folder:
            base, image = self.fixture(folder)
            descriptor = os.open(image, os.O_RDONLY)
            part = {'first_sector': 34, 'bytes': 4096}
            valid = {'name': '/dev/loop7', 'ro': True, 'offset': 34*512,
                     'sizelimit': 4096, 'back-file': str(image)}
            try:
                for changed in ({'ro': False}, {'offset': 0}, {'sizelimit': 8192}, {'back-file': '/dev/null'}):
                    state = {}
                    with self.subTest(changed=changed), \
                            patch.object(verifier.subprocess, 'check_output', side_effect=[
                                '/dev/loop7\n', json.dumps({'loopdevices': [{**valid, **changed}]})]), \
                            patch.object(verifier.subprocess, 'run') as run:
                        with self.assertRaises(ValueError):
                            with verifier.readonly_loop(descriptor, part, state):
                                self.fail('Invalid mapping must never reach e2fsck')
                        run.assert_called_once_with(['losetup', '--detach', '/dev/loop7'], check=True, timeout=30)
                        self.assertTrue(state['detached'])
            finally:
                os.close(descriptor)

    def test_first_dirty_filesystem_or_timeout_does_not_skip_second_or_repair(self):
        @contextlib.contextmanager
        def loop(descriptor, part, state):
            try:
                yield '/dev/loop7'
            finally:
                state['detached'] = True

        for first in (types.SimpleNamespace(returncode=4), subprocess.TimeoutExpired('e2fsck', 300)):
            with self.subTest(first=first), tempfile.TemporaryDirectory() as folder:
                output = pathlib.Path(folder)
                layout = {name: {'first_sector': 34, 'bytes': 4096} for name in ('rootfs','NSHSTATE')}
                report = {'filesystems': {}}
                with patch.object(verifier, 'readonly_loop', loop), \
                        patch.object(verifier.subprocess, 'run', side_effect=[first,types.SimpleNamespace(returncode=0)]) as run:
                    failures = verifier.check_filesystems(99, layout, output, report, lambda: None)
                self.assertEqual(failures, ['rootfs'])
                self.assertTrue(report['filesystems']['NSHSTATE']['passed'])
                self.assertTrue(all(state['detached'] for state in report['filesystems'].values()))
                self.assertEqual(run.call_count, 2)
                for call in run.call_args_list:
                    self.assertEqual(call.args[0], ['e2fsck','-f','-n','/dev/loop7'])
