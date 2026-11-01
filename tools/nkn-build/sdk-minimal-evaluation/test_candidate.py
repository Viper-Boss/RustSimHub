# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch
import evaluate


class PatchChain(unittest.TestCase):
    def copied(self, directory):
        target = Path(directory)
        for name in ['patch-manifest.json', 'websocket-only.patch']:
            shutil.copyfile(evaluate.ROOT / name, target / name)
        return target

    def test_exact_patch_and_previous_patch_chain(self):
        self.assertEqual(len(evaluate.metadata()['files']), 2)

    def test_altered_previous_patch_chain_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = self.copied(directory)
            metadata = json.loads((target / 'patch-manifest.json').read_text())
            metadata['base_patch_sha256'] = '0' * 64
            (target / 'patch-manifest.json').write_text(json.dumps(metadata))
            with patch.object(evaluate, 'ROOT', target), patch.object(evaluate.base, 'command') as command:
                with self.assertRaises(AssertionError):
                    evaluate.apply(target, {}, target)
                command.assert_not_called()

    def test_extra_patch_target_is_rejected_even_with_changed_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            target = self.copied(directory)
            data = (target / 'websocket-only.patch').read_bytes() + b'+++ b/../../outside\n'
            metadata = json.loads((target / 'patch-manifest.json').read_text())
            metadata['patch_sha256'] = hashlib.sha256(data).hexdigest()
            (target / 'patch-manifest.json').write_text(json.dumps(metadata))
            (target / 'websocket-only.patch').write_bytes(data)
            with patch.object(evaluate, 'ROOT', target), patch.object(evaluate.base, 'command') as command:
                with self.assertRaises(AssertionError):
                    evaluate.apply(target, {}, target)
                command.assert_not_called()


if __name__ == '__main__':
    unittest.main()
