# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import run
sys.path.insert(0, str(run.ROOT/'tools/product-integration'))
from test_verify import cleanup_owned


class SourceStamp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(cleanup_owned, self.temp)
        self.root = Path(self.temp.name)/'repo'
        self.root.mkdir()
        self.log = Path(self.temp.name)/'check.log'
        self.git('init', '-q')
        (self.root/'fixed').write_bytes(b'committed public fixture\n')
        self.git('add', 'fixed')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')

    def git(self, *args):
        return subprocess.check_output(['git', '-c', 'safe.directory='+self.root.as_posix(),
            '-C', str(self.root), *args], stderr=subprocess.DEVNULL, env=run.environment())

    def test_inherited_stamp_is_removed(self):
        with patch.dict(os.environ, {'RUSTSIMHUB_BUILD_SOURCE_SHA256': 'f'*64}):
            self.assertNotIn('RUSTSIMHUB_BUILD_SOURCE_SHA256', run.environment())

    def test_dirty_source_cannot_start_archive_or_receive_stamp(self):
        (self.root/'untracked').write_bytes(b'not committed')
        backend = Mock()
        stamp, row = run.source_stamp(backend, self.root, self.log, 10)
        self.assertIsNone(stamp)
        self.assertEqual(row['execution'], 'not_run')
        backend.execute.assert_not_called()
        self.assertFalse(self.log.exists())

    def test_failed_or_unclean_archive_cannot_supply_stamp(self):
        for index, fields in enumerate([{'exit_code': 1}, {'cleanup_complete': False}, {'execution': 'output_limit'}]):
            row = {'execution': 'executed', 'exit_code': 0, 'cleanup_complete': True, **fields}
            stamp, result = run.source_stamp(Mock(execute=Mock(return_value=row)), self.root,
                                            self.log.with_name(str(index)+'.log'), 10)
            self.assertIsNone(stamp)
            self.assertEqual(run.classify(result, 'rust'), 'failed')

    def test_source_change_during_archive_refuses_stamp(self):
        def mutate(argv, root, archive, seconds, **kwargs):
            archive.write_bytes(b'public fixture archive')
            (root/'fixed').write_bytes(b'changed during archive')
            return {'execution': 'executed', 'exit_code': 0, 'cleanup_complete': True}
        stamp, row = run.source_stamp(Mock(execute=Mock(side_effect=mutate)), self.root, self.log, 10)
        self.assertIsNone(stamp)
        self.assertEqual(row['execution'], 'source_changed')
        self.assertEqual(run.classify(row, 'rust'), 'failed')

    def test_unavailable_archive_is_not_run_not_a_false_failure_or_stamp(self):
        backend = Mock(execute=Mock(return_value={'execution': 'not_run'}))
        stamp, row = run.source_stamp(backend, self.root, self.log, 10)
        self.assertIsNone(stamp)
        self.assertEqual(run.classify(row, 'rust'), 'not_run')
        self.assertFalse(self.log.exists())

    @unittest.skipUnless(sys.platform == 'linux', 'actual pidfd archive integration requires Linux')
    def test_real_archive_digest_matches_complete_unstripped_git_bytes(self):
        expected = hashlib.sha256(self.git('archive', '--format=tar', 'HEAD')).hexdigest()
        stamp, proof = run.source_stamp(run.linux_backend(), self.root, self.log, 10)
        self.assertEqual(stamp, expected)
        self.assertEqual(proof['source_archive_sha256'], expected)
        self.assertEqual(proof['source_archive_head'], self.git('rev-parse', 'HEAD').decode().strip())
        self.assertFalse(self.log.exists())
