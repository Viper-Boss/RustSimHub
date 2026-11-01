# SPDX-License-Identifier: AGPL-3.0-only
import json
import hashlib
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import evaluate


class VerifiedSources(unittest.TestCase):
    def setUp(self):
        self.cache = Path(os.environ['RSH_SDK_EVAL_SOURCE_CACHE'])

    def test_actual_cache_matches_every_pinned_file(self):
        result = evaluate.verified_cache(self.cache)
        self.assertEqual(len(result), 31)

    def test_altered_sdk_source_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'cache'
            shutil.copytree(self.cache, target)
            source = target / 'third_party/nkn-sdk-go/client.go'
            source.write_bytes(source.read_bytes() + b'\n')
            with self.assertRaises(AssertionError):
                evaluate.verified_cache(target)

    def test_linked_notice_is_rejected_before_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'cache'
            shutil.copytree(self.cache, target)
            source = target / 'third_party/nkn-sdk-go/LICENSE'
            source.unlink()
            source.symlink_to(self.cache / 'third_party/nkn-sdk-go/LICENSE')
            with self.assertRaises(AssertionError):
                evaluate.verified_cache(target)

    def test_duplicate_manifest_identity_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            value = json.loads((evaluate.ROOT / 'source-manifest.json').read_text())
            value['files'].append(value['files'][0])
            (target / 'source-manifest.json').write_text(json.dumps(value))
            with patch.object(evaluate, 'ROOT', target), self.assertRaises(AssertionError):
                evaluate.manifest()

    def test_modified_patch_is_refused_before_running_patch_tool(self):
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)
            shutil.copyfile(evaluate.ROOT/'patch-manifest.json',target/'patch-manifest.json')
            (target/'sdk-compatibility.patch').write_bytes((evaluate.ROOT/'sdk-compatibility.patch').read_bytes()+b'\n')
            with patch.object(evaluate,'ROOT',target),patch.object(evaluate,'command') as command:
                with self.assertRaises(AssertionError):
                    evaluate.apply_candidate(target,{},target)
                command.assert_not_called()

    def test_patch_cannot_add_an_unexpected_target_even_with_new_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            target=Path(directory)
            value=json.loads((evaluate.ROOT/'patch-manifest.json').read_text())
            data=(evaluate.ROOT/'sdk-compatibility.patch').read_bytes()+b'+++ b/../../outside\n'
            value['patch_sha256']=hashlib.sha256(data).hexdigest()
            (target/'patch-manifest.json').write_text(json.dumps(value))
            (target/'sdk-compatibility.patch').write_bytes(data)
            with patch.object(evaluate,'ROOT',target),patch.object(evaluate,'command') as command:
                with self.assertRaises(AssertionError):
                    evaluate.apply_candidate(target,{},target)
                command.assert_not_called()


if __name__ == '__main__':
    unittest.main()
