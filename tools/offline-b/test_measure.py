# SPDX-License-Identifier: AGPL-3.0-only
import json
import os
from pathlib import Path
import tempfile
import unittest

import measure
import run
from checks import CHECKS


class HostMeasurement(unittest.TestCase):
    def test_only_fixed_profiles_and_small_budgets_are_accepted(self):
        with tempfile.TemporaryDirectory() as temp:
            for profile, iterations, seconds in [('arbitrary', 1, 1), ('sms', 0, 1),
                ('sms', 17, 1), ('sms', 1, 0), ('sms', 1, 21)]:
                with self.assertRaises(ValueError):
                    measure.measure(profile, Path(temp), iterations, seconds)
            self.assertFalse(list(Path(temp).iterdir()))

    def test_actual_job_member_cpu_rss_and_cleanup_or_explicit_platform_refusal(self):
        with tempfile.TemporaryDirectory() as temp:
            if os.name != 'nt':
                report = measure.measure('statistics', Path(temp), 1, 1)
                self.assertEqual(report['status'], 'not_run')
                self.assertEqual(report['exit_code'], 3)
                self.assertFalse(report['board_measured'])
                self.assertFalse(report['installable'])
                return
            check = next(c for c in CHECKS if c.name == 'web-usage-stats')
            result = run.windows_execute(check, run.ROOT, Path(temp)/'owned.log', 10, measure=True)
            self.assertEqual(run.classify(result, check.kind), 'passed', result)
            self.assertTrue(result['cleanup_complete'])
            data = result['resources']
            self.assertGreaterEqual(data['cpu_seconds'], 0)
            self.assertGreater(data['rss_samples'], 0)
            self.assertGreater(data['peak_observed_rss_bytes'], 0)
            self.assertGreater(data['peak_job_committed_bytes'], 0)


if __name__ == '__main__':
    unittest.main()
