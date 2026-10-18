# SPDX-License-Identifier: AGPL-3.0-only
import importlib.util
import io
import os
import pathlib
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('kernel_tree',ROOT/'tools/verify-kernel-source-tree.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


def archive(rows):
    result=io.BytesIO()
    with tarfile.open(fileobj=result,mode='w:gz') as tar:
        for name,kind,data in rows:
            row=tarfile.TarInfo(name);row.mode=0o644
            if kind=='file':row.size=len(data);tar.addfile(row,io.BytesIO(data))
            else:
                row.type={'dir':tarfile.DIRTYPE,'symlink':tarfile.SYMTYPE,'hardlink':tarfile.LNKTYPE,'device':tarfile.CHRTYPE}[kind]
                row.linkname=data.decode();tar.addfile(row)
    result.seek(0);return result


class KernelSourceTreeTests(unittest.TestCase):
    @unittest.skipIf(os.name=='nt','Unix executable bit/symlink Git reference fixture')
    def test_real_git_archive_matches_git_write_tree_including_modes_links_and_sort_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            repo=pathlib.Path(temporary)
            def git(*args):return subprocess.check_output(['git','-C',str(repo),*args],stderr=subprocess.PIPE)
            git('init','--quiet')
            (repo/'README').write_bytes(b'ordinary source\n')
            (repo/'a').mkdir();(repo/'a/child').write_bytes(b'nested\n')
            (repo/'a.c').write_bytes(b'before-directory\n');(repo/'a0').write_bytes(b'after-directory\n')
            (repo/'run').write_bytes(b'#!/bin/sh\nexit 0\n');(repo/'run').chmod(0o755)
            (repo/'z-link').symlink_to('README')
            git('add','.');tree=git('write-tree').decode().strip()
            data=git('archive','--format=tar.gz','--prefix=linux/',tree)
            result=module.source_tree(io.BytesIO(data))
            self.assertEqual(result['calculated_git_tree'],tree)
            self.assertEqual(result['file_count'],6)
            self.assertEqual(result['archive_prefix'],'linux')

    def test_unsafe_names_duplicate_shadow_links_devices_and_multiple_roots_refused(self):
        cases=[ [('linux/../escape','file',b'')], [('/linux/file','file',b'')],
                [('linux/./file','file',b'')], [('linux//file','file',b'')],
                [('linux/a','file',b'a'),('linux/a','file',b'b')],
                [('linux/a/child','file',b''),('linux/a','file',b'')],
                [('linux/a','symlink',b'outside'),('linux/a/child','file',b'')],
                [('linux/a','hardlink',b'linux/file')], [('linux/device','device',b'')],
                [('linux/a','file',b''),('other/b','file',b'')], [] ]
        for rows in cases:
            with self.subTest(rows=rows):
                with self.assertRaises(ValueError):module.source_tree(archive(rows))

    def test_file_total_entry_and_symlink_bounds_refuse_without_extracting(self):
        rows=[('linux/a','file',b'aa'),('linux/b','file',b'bb')]
        for name,value in [('MAX_FILE',1),('MAX_TOTAL',3),('MAX_ENTRIES',1)]:
            with patch.object(module,name,value):
                with self.assertRaises(ValueError):module.source_tree(archive(rows))
        with self.assertRaises(ValueError):module.source_tree(archive([('linux/link','symlink',b'x'*16385)]))

    def test_wrong_input_or_existing_report_never_extracts_or_overwrites(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=pathlib.Path(temporary)
            source=directory/'source';source.write_bytes(b'wrong upstream')
            report=directory/'report'
            with self.assertRaises(ValueError):module.verify(source,report)
            self.assertFalse(report.exists())
            report.write_bytes(b'preserve')
            with self.assertRaises(ValueError):module.verify(source,report)
            self.assertEqual(report.read_bytes(),b'preserve')
            self.assertEqual({p.name for p in directory.iterdir()},{'source','report'})
