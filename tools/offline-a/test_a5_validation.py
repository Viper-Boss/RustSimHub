# SPDX-License-Identifier: AGPL-3.0-only
"""The test observer must reject empty or failed test selections."""
import json
from pathlib import Path
import os
import sys
import tempfile
import unittest
from validate_a5 import Run


class ObserverBoundaries(unittest.TestCase):
    def observe(self, text, exit_code=0):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        r = Run.__new__(Run)
        r.out = Path(tmp.name)
        r.source = r.out
        r.env = dict(os.environ)
        r.report = {'steps': [], 'passed': False}
        command = [sys.executable, '-c', 'import sys; print(sys.argv[1]); sys.exit(int(sys.argv[2]))', text, str(exit_code)]
        return r, command

    def test_zero_selected_tests_is_not_a_pass(self):
        r, command = self.observe('test result: ok. 0 passed; 0 failed; 0 ignored')
        with self.assertRaises(RuntimeError):
            r.run('empty', command, tests=True)
        self.assertFalse(json.loads((r.out/'result.json').read_text())['steps'][0]['nonempty'])
        self.assertFalse(r.report['passed'])

    def test_nonzero_exit_preserves_failure_even_with_a_success_line(self):
        r, command = self.observe('test result: ok. 1 passed; 0 failed; 0 ignored', 42)
        with self.assertRaises(RuntimeError):
            r.run('failed', command, tests=True)
        self.assertEqual(r.report['steps'][0]['exit_code'], 42)
        self.assertFalse(r.report['passed'])

    def test_one_executed_test_has_evidence_but_is_not_a_whole_suite(self):
        r, command = self.observe('test result: ok. 1 passed; 0 failed; 0 ignored')
        r.run('one', command, tests=True)
        step = json.loads((r.out/'result.json').read_text())['steps'][0]
        self.assertTrue(step['nonempty'])
        self.assertEqual(len(step['log_sha256']), 64)
        self.assertFalse(r.report['passed'])


if __name__ == '__main__':
    unittest.main()
