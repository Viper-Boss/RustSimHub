# SPDX-License-Identifier: AGPL-3.0-only
"""Actual source-index parser/CLI limits; only owned synthetic lock files."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

import verify


class InputLimits(unittest.TestCase):
    def sample(self):
        return {'format': 1, 'repository': 'Viper-Boss/RustSimHub', 'baseline': '1'*40,
                'inputs': [{'id': 'actual', 'kind': 'product', 'pr': 1, 'head': '2'*40,
                            'requires': [], 'includes': []}]}

    def test_deep_json_is_a_finite_refusal_not_an_unhandled_recursion(self):
        with self.assertRaises(ValueError):
            verify.parse(b'[' * 2000 + b'0' + b']' * 2000)

    def test_cli_refuses_deep_lock_before_git_and_without_traceback(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            path = root/'synthetic-lock.json'
            original = b'[' * 60000 + b'0' + b']' * 60000
            path.write_bytes(original)
            result = subprocess.run([sys.executable, str(Path(verify.__file__)), '--repository',
                str(root/'not-a-repository'), '--integration', '1'*40, '--lock', str(path)],
                stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn(b'Integration source refused:', result.stderr)
            self.assertNotIn(b'Traceback', result.stderr)
            self.assertLess(len(result.stderr), 512)
            self.assertEqual(path.read_bytes(), original)

    def test_actual_file_read_and_duplicate_fields(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'lock.json'
            value = self.sample()
            path.write_bytes(json.dumps(value).encode())
            self.assertEqual(verify.read_lock(path), value)
            path.write_bytes(b'{"format":1,"format":1}')
            with self.assertRaises(ValueError):
                verify.read_lock(path)

    def test_exact_byte_limit_is_accepted_for_a_valid_lock(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'lock.json'
            value = self.sample()
            raw = json.dumps(value).encode()
            path.write_bytes(raw + b' ' * (verify.MAX_LOCK_BYTES - len(raw)))
            self.assertEqual(verify.read_lock(path), value)

    def test_pinned_sources_preserve_history_and_exclude_drafts(self):
        root = Path(verify.__file__).resolve().parents[2]
        raw = subprocess.check_output(['git', '--no-replace-objects', '-c',
            'safe.directory='+root.as_posix(), '-C', str(root), 'show',
            'de663743bd726e77f7c560139d0f91ea78e8fe57:tools/product-integration/inputs.json'], timeout=5)
        original = verify.parse(raw)
        current = verify.read_lock(verify.LOCK)
        self.assertEqual(current['inputs'][:18], original['inputs'])
        self.assertEqual(current['baseline'], original['baseline'])
        self.assertEqual(len(current['inputs']), 32)
        numbers = {row['pr'] for row in current['inputs']}
        self.assertFalse(numbers & {268, 250, 262, 259, 69, 52, 53, 56})
        reports = {row['pr'] for row in current['inputs'] if row['kind'] == 'report'}
        self.assertTrue({261, 264} <= reports)

    def test_oversized_and_nonregular_input_refused_before_open(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            path = root/'oversized'
            path.write_bytes(b' ' * (verify.MAX_LOCK_BYTES + 1))
            for value in (root, path):
                with mock.patch.object(verify.os, 'open', side_effect=AssertionError('must not open')):
                    with self.assertRaises(ValueError):
                        verify.read_lock(value)
            self.assertEqual(path.stat().st_size, verify.MAX_LOCK_BYTES + 1)

    def test_replacement_after_lstat_is_not_accepted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            path, other = root/'lock.json', root/'other.json'
            raw = json.dumps(self.sample()).encode()
            path.write_bytes(raw)
            other.write_bytes(raw)
            original_open = verify.os.open
            def different(_path, flags):
                return original_open(other, flags)
            with mock.patch.object(verify.os, 'open', side_effect=different):
                with self.assertRaisesRegex(ValueError, 'replaced'):
                    verify.read_lock(path)

    def test_original_graph_limit_and_report_dependency_stay_strict(self):
        value = self.sample()
        value['inputs'] = [dict(value['inputs'][0], id='input-'+str(i), pr=i+1,
                                head=f'{i+2:040x}') for i in range(32)]
        self.assertEqual(len(verify.parse(json.dumps(value).encode())['inputs']), 32)
        value['inputs'].append(dict(value['inputs'][0], id='excess', pr=33, head='f'*40))
        with self.assertRaises(ValueError):
            verify.parse(json.dumps(value).encode())
        value = self.sample()
        value['inputs'].append({'id':'report', 'kind':'report', 'pr':2, 'head':'3'*40,
                                'requires':[], 'includes':[]})
        value['inputs'][0]['requires'] = ['report']
        with self.assertRaises(ValueError):
            verify.parse(json.dumps(value).encode())


if __name__ == '__main__':
    unittest.main()
