# SPDX-License-Identifier: AGPL-3.0-only
import importlib.util
import json
from pathlib import Path
import unittest

from checks import CHECKS
import recovery_result
import run


class RecoveryCompletion(unittest.TestCase):
    def complete(self):
        return {'passed': True, 'backend': 'locked-production-rust-rlibs',
                'case_count': 15, 'cases': sorted(recovery_result.CASES),
                'production_changes': False, 'device_operations': 0,
                'transport_sends': 0, 'new_cargo_dependencies': False}

    def classify(self, text, **changes):
        result = {'execution': 'executed', 'exit_code': 0,
                  'cleanup_complete': True, 'summary': text, **changes}
        return run.classify(result, 'recovery-json'), result

    def test_inventory_matches_original_controller_without_running_it(self):
        path = Path(__file__).resolve().parents[1] / 'test-business-recovery-matrix.py'
        spec = importlib.util.spec_from_file_location('original_recovery_controller', path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertEqual(recovery_result.CASES, module.CASES)
        check = next(c for c in CHECKS if c.name == 'business-recovery-matrix')
        self.assertEqual(check.argv[1:], ('tools/test-business-recovery-matrix.py', '--run'))
        self.assertEqual(check.group, 'recovery')
        self.assertEqual(check.kind, 'recovery-json')
        self.assertIn('linux', check.requires)

    def test_exact_completion_preserves_count(self):
        status, result = self.classify(json.dumps(self.complete()))
        self.assertEqual(status, 'passed')
        self.assertEqual(result['counts'], {'passed': 15, 'ignored': 0})

    def test_missing_duplicate_unknown_or_nonstring_case_is_rejected(self):
        original = self.complete()['cases']
        for cases in (original[:-1], original + [original[0]],
                      original[:-1] + [original[0]], original[:-1] + ['invented'],
                      original[:-1] + [{}]):
            with self.subTest(cases=cases):
                row = {**self.complete(), 'cases': cases}
                self.assertEqual(self.classify(json.dumps(row))[0], 'failed')

    def test_plans_markers_malformed_or_multiple_results_cannot_pass(self):
        complete = json.dumps(self.complete())
        for text in ('', 'PASS: recovery passed', '{"execute":false}', 'null', '[]',
                     '{}', complete + complete, complete[:-1], '[' * 2000):
            with self.subTest(text=text[:60]):
                self.assertEqual(self.classify(text)[0], 'failed')

    def test_each_boundary_field_is_required_and_exactly_typed(self):
        for key in self.complete():
            row = self.complete()
            del row[key]
            self.assertEqual(self.classify(json.dumps(row))[0], 'failed', key)
        for key, value in (('passed', 1), ('passed', False), ('backend', 'mock'),
                           ('case_count', True), ('case_count', 15.0),
                           ('production_changes', 0), ('new_cargo_dependencies', 0),
                           ('device_operations', False), ('transport_sends', False),
                           ('device_operations', 1), ('transport_sends', 1)):
            with self.subTest(key=key, value=value):
                row = {**self.complete(), key: value}
                self.assertEqual(self.classify(json.dumps(row))[0], 'failed')

    def test_duplicate_keys_nonfinite_numbers_and_extra_claims_are_rejected(self):
        text = json.dumps(self.complete())
        for bad in (text[:-1] + ',"passed":true}',
                    text.replace('"case_count": 15', '"case_count": NaN'),
                    text.replace('"case_count": 15', '"case_count": Infinity'),
                    text[:-1] + ',"health_authorized":true}'):
            self.assertEqual(self.classify(bad)[0], 'failed')

    def test_process_failures_override_complete_json(self):
        text = json.dumps(self.complete())
        for key, value in (('exit_code', 1), ('cleanup_complete', False),
                           ('execution', 'timeout'), ('execution', 'output_limit'),
                           ('execution', 'orphaned_children')):
            self.assertEqual(self.classify(text, **{key: value})[0], 'failed')
        self.assertEqual(self.classify(text, execution='not_run')[0], 'not_run')


if __name__ == '__main__':
    unittest.main()
