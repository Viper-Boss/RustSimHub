# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import run
from checks import CHECKS, Check


class Results(unittest.TestCase):
    def good(self, text):
        return {'execution': 'executed', 'exit_code': 0, 'cleanup_complete': True, 'summary': text}

    def test_real_counter_required_and_zero_tests_rejected(self):
        for text in ['', 'Ran 0 tests in 0s\nOK', 'Ran 3 tests in 1s\nFAILED']:
            self.assertEqual(run.classify(self.good(text), 'unittest'), 'failed')
        row = self.good('Ran 3 tests in 1s\nOK\n')
        self.assertEqual(run.classify(row, 'unittest'), 'passed')
        self.assertEqual(row['counts'], {'total': 3, 'skipped': 0})

    def test_skips_are_partial_unexecuted_coverage(self):
        row = self.good('Ran 18 tests in 1s\nOK (skipped=2)\n')
        self.assertEqual(run.classify(row, 'unittest'), 'not_run')
        self.assertEqual(row['counts']['skipped'], 2)

    def test_exit_timeout_output_or_cleanup_failure_not_overridden_by_ok_text(self):
        for field, value in [('exit_code', 1), ('cleanup_complete', False), ('execution', 'timeout'),
                             ('execution', 'output_limit'), ('execution', 'orphaned_children')]:
            row = self.good('Ran 8 tests in 1s\nOK\n')
            row[field] = value
            self.assertEqual(run.classify(row, 'unittest'), 'failed')

    def test_rust_empty_and_ignored_are_distinct(self):
        for text, expected in [('test result: ok. 0 passed; 0 failed; 0 ignored;', 'failed'),
            ('test result: ok. 3 passed; 0 failed; 1 ignored;', 'not_run'),
            ('test result: ok. 3 passed; 0 failed; 0 ignored;', 'passed')]:
            self.assertEqual(run.classify(self.good(text), 'rust'), expected)

    def test_node_must_reach_existing_assertion_completion_marker(self):
        self.assertEqual(run.classify(self.good(''), 'node'), 'failed')
        self.assertEqual(run.classify(self.good('PASS: real production-module assertions'), 'node'), 'passed')
        self.assertEqual(run.classify(self.good('Historical forwarding UI passed: assertions finished'), 'node'), 'passed')

    def test_missing_prerequisite_is_not_run(self):
        self.assertEqual(run.classify({'execution': 'not_run'}, 'unittest'), 'not_run')

    def test_old_node_runtime_is_a_missing_prerequisite(self):
        for version, expected in [(b'v16.20.2\n', False), (b'v22.15.0\n', True), (b'v24.22.1\n', True)]:
            with patch.object(run.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, version, b'')):
                self.assertEqual(run.capability('node'), expected)

    def test_offline_environment_disables_background_fixture_maintenance(self):
        env = run.environment()
        self.assertEqual(env['CARGO_NET_OFFLINE'], 'true')
        self.assertEqual(env['GOPROXY'], 'off')
        self.assertEqual(env['GIT_CONFIG_KEY_0'], 'gc.auto')
        self.assertEqual(env['GIT_CONFIG_VALUE_0'], '0')
        self.assertEqual(env['GIT_CONFIG_KEY_1'], 'maintenance.auto')
        self.assertEqual(env['GIT_CONFIG_VALUE_1'], 'false')

    def test_fixed_selection_has_no_device_download_hotspot_or_ble_commands(self):
        self.assertEqual(len(CHECKS), len({c.name for c in CHECKS}))
        for check in CHECKS:
            command = ' '.join(check.argv)
            for forbidden in ('adb ', 'ssh ', 'fastboot ', 'curl ', 'pip ', 'firstboot-ap', 'test-web-ap-',
                              'test-web-setup', 'airsim', 'test-ble', 'configurator', 'native/audio'):
                self.assertNotIn(forbidden, command)
            self.assertGreater(check.seconds, 0)
            self.assertTrue(check.actions)

    def test_empty_selection_cannot_be_success(self):
        with tempfile.TemporaryDirectory() as temp, self.assertRaises(ValueError):
            run.run([], Path(temp))


class FilesAndReports(unittest.TestCase):
    def test_output_exclusive_and_source_or_private_location_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root/'repo'
            source.mkdir()
            output = run.fresh_output(root/'new', source)
            self.assertTrue(output.is_dir())
            for value in (output, source, source/'run', root/'.private'/'run', root/'.LOCAL'/'run'):
                with self.assertRaises((ValueError, FileExistsError)):
                    run.fresh_output(value, source)

    def test_missing_capability_writes_non_green_report_without_starting_process(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            state = {'head': '1'*40, 'tree': '2'*40, 'worktree_sha256': '3'*64, 'dirty': True}
            check = Check('missing', 'process', ('unavailable',), 'unittest', ('UPD-05',), ('missing',))
            with patch.object(run, 'source', return_value=state), patch.object(run, 'capability', return_value=False), \
                 patch.object(run, 'execute', side_effect=AssertionError('must not run')):
                self.assertEqual(run.run([check], out), 3)
            report = json.loads((out/'report.json').read_text())
            self.assertEqual(report['checks'][0]['status'], 'not_run')
            self.assertFalse(report['installable'])
            self.assertFalse(report['hardware_tested'])
            self.assertFalse(report['health_authorized'])

    def test_source_changed_during_run_cannot_pass(self):
        with tempfile.TemporaryDirectory() as temp:
            states = [{'head': '1'*40}, {'head': '2'*40}]
            check = Check('actual', 'process', ('fixture',), 'unittest', ('SEC-06',))
            with patch.object(run, 'source', side_effect=states), patch.object(run, 'execute', return_value={
                'execution': 'executed', 'exit_code': 0, 'cleanup_complete': True,
                'summary': 'Ran 1 test in 1s\nOK\n'}):
                self.assertEqual(run.run([check], Path(temp)), 2)
            self.assertTrue(json.loads((Path(temp)/'report.json').read_text())['source_changed'])

    def test_worktree_hash_detects_untracked_byte_changes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args):
                return subprocess.check_output(['git', '-c', 'safe.directory='+root.as_posix(),
                    '-C', str(root), *args], stderr=subprocess.DEVNULL)
            git('init', '-q')
            (root/'fixed').write_bytes(b'fixed')
            git('add', 'fixed')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'owned fixture')
            (root/'new').write_bytes(b'a')
            before = run.source(root)
            (root/'new').write_bytes(b'b')
            after = run.source(root)
            self.assertEqual(before['head'], after['head'])
            self.assertTrue(before['dirty'] and after['dirty'])
            self.assertNotEqual(before['worktree_sha256'], after['worktree_sha256'])


class ActualProcesses(unittest.TestCase):
    def test_actual_reviewed_fixture_runs_to_completion_through_backend(self):
        if not run.capability('node'):
            self.fail('Node is required for this nonempty actual-backend regression')
        check = next(c for c in CHECKS if c.name == 'web-usage-stats')
        with tempfile.TemporaryDirectory() as temp:
            result = run.execute(check, run.ROOT, Path(temp)/'real-module.log', 15)
            self.assertEqual(run.classify(result, check.kind), 'passed', result)
            self.assertTrue(result['cleanup_complete'])

    def test_owned_descendants_are_killed_on_this_platform(self):
        if os.name == 'nt':
            from windows_job import Job
            job = Job()
            with tempfile.TemporaryDirectory() as temp:
                marker = Path(temp)/'ready'
                script = ("import sys;sys.path.insert(0,sys.argv[1]);from windows_job import join;"
                    "join(sys.argv[2]);import subprocess,time;from pathlib import Path;"
                    "subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']);"
                    "Path(sys.argv[3]).write_text('joined');time.sleep(60)")
                proc = subprocess.Popen([sys.executable, '-I', '-S', '-c', script,
                    str(Path(__file__).parent), job.name, str(marker)], stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    until = time.monotonic()+5
                    while not marker.exists() and time.monotonic() < until and proc.poll() is None:
                        time.sleep(.02)
                    self.assertTrue(marker.exists(), 'bootstrap must join owned job')
                    self.assertGreaterEqual(job.active(), 2)
                    job.stop()
                    proc.wait(timeout=3)
                    until = time.monotonic()+3
                    while job.active() and time.monotonic() < until:
                        time.sleep(.02)
                    self.assertEqual(job.active(), 0)
                finally:
                    job.close()
                    proc.wait(timeout=3)
        elif sys.platform == 'linux':
            with tempfile.TemporaryDirectory() as temp:
                check = Check('owned-fixture', 'process', (sys.executable, '-c',
                    "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']);time.sleep(60)"),
                    'unittest', ('SEC-06',))
                result = run.execute(check, run.ROOT, Path(temp)/'bounded.log', .3)
                self.assertEqual(result['execution'], 'timeout')
                self.assertTrue(result['cleanup_complete'])
                self.assertEqual(run.classify(result, check.kind), 'failed')
        else:
            self.assertFalse(run.capability('linux'))

    def test_unknown_check_never_executes_an_arbitrary_command(self):
        result = subprocess.run([sys.executable, str(Path(__file__).with_name('worker.py')),
            'not-a-job', 'not-a-check'], capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 87)


if __name__ == '__main__':
    unittest.main()
