# SPDX-License-Identifier: AGPL-3.0-only
"""Synthetic archive boundary tests; no extraction, SSH or device operations."""
import importlib.util
import io
import pathlib
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('stage_tree', pathlib.Path(__file__).resolve().parents[1] / 'stage-system-tree.py')
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


class StageArchiveTests(unittest.TestCase):
    def check(self, rows):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / 'synthetic.tar.gz'
            with tarfile.open(path, 'w:gz') as tar:
                for name, kind, target in rows:
                    entry = tarfile.TarInfo(name)
                    entry.type = kind
                    entry.linkname = target
                    entry.size = 1 if kind == tarfile.REGTYPE else 0
                    tar.addfile(entry, io.BytesIO(b'x') if entry.size else None)
            stage.check_archive(path)

    def test_rootfs_symlink_leaf_and_internal_regular_hardlink_are_allowed(self):
        self.check([('usr/lib/data', tarfile.REGTYPE, ''),
                    ('usr/lib/alias', tarfile.LNKTYPE, 'usr/lib/data'),
                    ('etc/localtime', tarfile.SYMTYPE, '/usr/share/zoneinfo/Etc/UTC')])

    def test_absolute_parent_escape_device_and_duplicate_members_are_refused(self):
        for rows in [[('/etc/file', tarfile.REGTYPE, '')],
                     [('../../file', tarfile.REGTYPE, '')],
                     [('dev/raw', tarfile.CHRTYPE, '')],
                     [('file', tarfile.REGTYPE, ''), ('file', tarfile.REGTYPE, '')]]:
            with self.subTest(rows=rows), self.assertRaises(AssertionError):
                self.check(rows)

    def test_regular_and_hardlink_paths_cannot_traverse_an_archive_symlink(self):
        for entry in [('usr/lib/evil', tarfile.REGTYPE, ''),
                      ('alias', tarfile.LNKTYPE, 'usr/lib/evil'),
                      ('alias', tarfile.LNKTYPE, 'usr/lib')]:
            with self.subTest(entry=entry), self.assertRaises(AssertionError):
                self.check([('usr/lib', tarfile.SYMTYPE, '/outside'), entry])
