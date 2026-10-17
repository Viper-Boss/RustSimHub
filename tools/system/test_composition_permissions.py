# SPDX-License-Identifier: AGPL-3.0-only
"""System tree traversal must work without weakening private base directories."""
import importlib.util
import json
import os
import pathlib
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

TOOLS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('compose_tree', TOOLS / 'compose-system-tree.py')
compose = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compose)


class CompositionPermissionsTests(unittest.TestCase):
    def test_restrictive_umask_keeps_program_parents_and_metadata_readable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            previous = os.umask(0o077)
            try:
                compose.write_metadata(root, 'opt/rustsimhub/factory/source/build.json', {'source': 'synthetic'})
                compose.directory(compose.destination(root, 'opt/rustsimhub/selected'), 0o755)
                compose.directory(compose.destination(root, 'opt/rustsimhub/versions'), 0o700)
            finally:
                os.umask(previous)
            for name in ('opt', 'opt/rustsimhub', 'opt/rustsimhub/factory', 'opt/rustsimhub/factory/source', 'opt/rustsimhub/selected'):
                self.assertEqual(stat.S_IMODE((root / name).stat().st_mode), 0o755)
            self.assertEqual(stat.S_IMODE((root / 'opt/rustsimhub/versions').stat().st_mode), 0o700)
            metadata = root / 'opt/rustsimhub/factory/source/build.json'
            self.assertEqual(stat.S_IMODE(metadata.stat().st_mode), 0o644)
            self.assertEqual(json.loads(metadata.read_text()), {'source': 'synthetic'})

    def test_existing_private_parent_is_preserved_and_symlink_escape_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / 'private').mkdir(mode=0o700)
            compose.destination(root, 'private/new/file')
            self.assertEqual(stat.S_IMODE((root / 'private').stat().st_mode), 0o700)
            (root / 'alias').symlink_to(root / 'private', target_is_directory=True)
            with self.assertRaises(AssertionError):
                compose.destination(root, 'alias/escape/file')
            with self.assertRaises(AssertionError):
                compose.destination(root, '../escape')
            (root / 'file').write_text('synthetic')
            with self.assertRaises(AssertionError):
                compose.destination(root, 'file/child')

    @unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0 and shutil.which('setpriv'), 'Linux root/setpriv needed for actual service UID traversal')
    def test_service_uid_can_execute_new_slot_under_restrictive_umask(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            root.chmod(0o755)
            previous = os.umask(0o077)
            try:
                target = compose.destination(root, 'opt/rustsimhub/factory/source/program')
                target.write_text('#!/bin/sh\nprintf "synthetic-service-ok\\n"\n')
                target.chmod(0o555)
            finally:
                os.umask(previous)
            result = subprocess.run(['setpriv', '--reuid=986', '--regid=986', '--clear-groups', str(target)], capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout, 'synthetic-service-ok\n')
