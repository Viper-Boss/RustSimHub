# SPDX-License-Identifier: AGPL-3.0-only
"""Tampering must fail real source/notice checks; never edit the registry cache."""
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import inventory


class SourceEvidence(unittest.TestCase):
    def fixture(self, directory):
        root = Path(directory)
        shutil.copyfile(inventory.ROOT / 'Cargo.lock', root / 'Cargo.lock')
        (root / 'fixtures').mkdir()
        for name in ['snow-xx-aesgcm-sha256.json', 'snow-LICENSE-MIT', 'snow-LICENSE-APACHE']:
            shutil.copyfile(inventory.ROOT / 'fixtures' / name, root / 'fixtures' / name)
        return root

    def test_real_archive_matches_selected_vector_and_both_notices(self):
        result = inventory.verify_sources()
        self.assertEqual((result['selected_vectors'], result['known_answer_messages']), (1, 5))
        self.assertEqual(len(result['notice_sha256']), 2)

    def test_changed_expected_ciphertext_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            path = root / 'fixtures/snow-xx-aesgcm-sha256.json'
            value = json.loads(path.read_text())
            value['vectors'][0]['messages'][0]['ciphertext'] = '00'
            path.write_text(json.dumps(value))
            with patch.object(inventory, 'ROOT', root), self.assertRaises(AssertionError):
                inventory.verify_sources()

    def test_missing_copyright_notice_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = self.fixture(directory)
            (root / 'fixtures/snow-LICENSE-MIT').write_text('MIT\n')
            with patch.object(inventory, 'ROOT', root), self.assertRaises(AssertionError):
                inventory.verify_sources()

    def test_altered_registry_archive_is_refused_before_unpacking(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / 'registry/cache/synthetic-registry'
            cache.mkdir(parents=True)
            (cache / 'snow-0.10.0.crate').write_bytes(b'synthetic corrupt archive')
            with patch.dict(os.environ, {'CARGO_HOME': str(root)}), self.assertRaises(AssertionError):
                inventory.verify_sources()


if __name__ == '__main__':
    unittest.main()
