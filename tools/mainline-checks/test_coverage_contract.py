# SPDX-License-Identifier: AGPL-3.0-only
"""Portable exact-source contract checks; no Linux process backend imported."""
import copy
import json
from pathlib import Path
import unittest
import workflow_coverage as coverage


class CoverageContract(unittest.TestCase):
    def contract(self):
        return json.loads(Path(coverage.__file__).with_name('workflow-inventory.json').read_text())

    def test_current_exact_workflow_and_all_check_identities_are_audited(self):
        report = coverage.audit()
        self.assertTrue(report['not_general_ci_or_release_approval'])
        excluded = report['excluded_workflow_validations']
        self.assertEqual(len(excluded), 1)
        self.assertIn('AirSIM', excluded[0]['step'])
        self.assertEqual(excluded[0]['status'], 'not_run')
        self.assertEqual(excluded[0]['checks'], [])
        self.assertIn('offline-b-source-inputs', report['runner_only'])

    def test_excluded_scope_cannot_supply_an_executable_check(self):
        value = self.contract()
        row = next(r for r in value['workflow_steps'] if r['disposition'] == 'excluded_scope')
        row['checks'] = ['fmt']
        with self.assertRaisesRegex(ValueError, 'excluded workflow'):
            coverage.audit(contract=value)

    def test_excluded_scope_needs_an_explicit_reason(self):
        value = self.contract()
        next(r for r in value['workflow_steps'] if r['disposition'] == 'excluded_scope')['reason'] = ''
        with self.assertRaises(ValueError):
            coverage.audit(contract=value)

    def test_current_usage_checks_cannot_be_replaced_by_an_unrelated_stage(self):
        value = self.contract()
        row = next(r for r in value['workflow_steps'] if r['step'] == 'Independent usage permission and aggregate query UI')
        self.assertEqual(set(row['checks']), {'web-usage-stats','web-usage-participation','web-nkn-product'})
        owner = next(r for r in value['workflow_steps'] if r['step'] == 'Usage permission through original Owner RPC')
        self.assertEqual(owner['checks'], ['usage-owner-rpc'])
        value['check_identities'][0]['argv'] = ['unreviewed']
        with self.assertRaisesRegex(ValueError, 'identity changed'):
            coverage.audit(contract=value)


if __name__ == '__main__':
    unittest.main()
