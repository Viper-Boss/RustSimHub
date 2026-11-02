# SPDX-License-Identifier: AGPL-3.0-only
"""Synthetic runner tests; these are not firmware or Root acceptance evidence."""
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
from checklist import Check, CHECKS, GROUPS


class Processes(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='mainline-runner-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sequence = 0

    def command(self, code, timeout=3, limit=100000):
        self.sequence += 1
        return run.execute([sys.executable, '-c', code], self.root,
                           self.root / (str(self.sequence) + '.log'), timeout, limit=limit)

    def test_real_exit_code_stdout_stderr_and_signals(self):
        r = self.command('import sys; print("out"); print("err", file=sys.stderr); sys.exit(7)')
        self.assertEqual(r['exit_code'], 7)
        self.assertEqual(r['execution'], 'executed')
        self.assertIn('out', r['summary'])
        self.assertIn('err', r['summary'])
        self.assertEqual(run.classify(r, 'command'), 'failed')
        r = self.command('import os,signal; os.kill(os.getpid(), signal.SIGTERM)')
        self.assertEqual(r['exit_code'], -15)
        r = self.command('print("ok")')
        self.assertEqual(run.classify(r, 'command'), 'passed')

    def test_missing_executable_is_not_run(self):
        r = run.execute(['/nonexistent-mainline-tool'], self.root, self.root/'missing.log', 1)
        self.assertIsNone(r['exit_code'])
        self.assertEqual(run.classify(r, 'command'), 'not_run')
        self.assertTrue(r['ended'])

    def test_timeout_kills_and_reaps_term_ignoring_child(self):
        pidfile = self.root/'child.pid'
        code = '''import os,signal,time
signal.signal(signal.SIGTERM, signal.SIG_IGN)
pid=os.fork()
if pid==0:
    open('child.pid','w').write(str(os.getpid()))
while True: time.sleep(.02)
'''
        start = time.monotonic()
        r = self.command(code, timeout=.25)
        self.assertEqual(r['execution'], 'timeout')
        self.assertEqual(r['exit_code'], -9)
        self.assertTrue(r['cleanup_complete'])
        self.assertLess(time.monotonic()-start, 4)
        with self.assertRaises(ProcessLookupError):
            os.kill(int(pidfile.read_text()), 0)

    def test_timeout_reaps_descendant_in_a_new_session(self):
        code = """import os,signal,time
if os.fork()==0:
    os.setsid()
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    open('escaped.pid','w').write(str(os.getpid()))
while True: time.sleep(.02)
"""
        r = self.command(code, timeout=.25)
        self.assertEqual(r['execution'], 'timeout')
        self.assertTrue(r['cleanup_complete'])
        escaped = int((self.root/'escaped.pid').read_text())
        self.assertIn(escaped, [p['pid'] for p in r['processes']])
        with self.assertRaises(ProcessLookupError):
            os.kill(escaped, 0)

    def test_parent_exit_and_inherited_pipe_does_not_hang(self):
        r = self.command('import os,time\nif os.fork()==0: time.sleep(20)', timeout=.2)
        self.assertEqual(r['execution'], 'timeout')
        self.assertTrue(r['cleanup_complete'])
        self.assertEqual(r['exit_code'], 0)
        self.assertEqual(run.classify(r, 'command'), 'failed')

    def test_closed_pipes_still_have_deadline(self):
        r = self.command('import os,time; os.close(1); os.close(2); time.sleep(20)', timeout=.2)
        self.assertEqual(r['execution'], 'timeout')
        self.assertTrue(r['cleanup_complete'])

    def test_closed_pipe_background_child_is_reclaimed(self):
        code = 'import os,time\nif os.fork()==0:\n os.close(1); os.close(2); time.sleep(20)'
        r = self.command(code)
        self.assertEqual(r['execution'], 'orphaned_children')
        self.assertTrue(r['cleanup_complete'])

    def test_output_limit_bounded_file_and_memory(self):
        r = self.command('import os\nwhile True: os.write(1,b"x"*65536)', limit=1234)
        self.assertEqual(r['execution'], 'output_limit')
        self.assertEqual((self.root/r['log']).stat().st_size, 1234)
        self.assertLessEqual(len(r['summary'].encode()), run.TAIL_LIMIT)
        self.assertTrue(r['truncated'])
        self.assertTrue(r['cleanup_complete'])

    def test_interrupt_reclaims_original_group(self):
        import signal
        import threading
        timer = threading.Timer(.2, lambda: os.kill(os.getpid(), signal.SIGINT))
        timer.start()
        try:
            r = self.command('import time; time.sleep(20)')
        finally:
            timer.cancel()
            timer.join()
        self.assertEqual(r['execution'], 'interrupted')
        self.assertTrue(r['cleanup_complete'])
        self.assertEqual(run.classify(r, 'command'), 'failed')

    def test_no_shell_interpretation(self):
        r = run.execute([sys.executable, '-c', 'import sys; print(sys.argv[1])', '$(touch injected);`touch injected`'],
                        self.root, self.root/'literal.log', 2)
        self.assertEqual(r['exit_code'], 0)
        self.assertFalse((self.root/'injected').exists())
        self.assertIn('$(touch injected)', r['summary'])

    def test_artifact_only_contains_stdout(self):
        artifact = self.root/'cargo.json'
        r = run.execute([sys.executable, '-c', 'import sys; print("json"); print("warning",file=sys.stderr)'],
                        self.root, self.root/'artifact.log', 2, artifact=artifact)
        self.assertEqual(r['exit_code'], 0)
        self.assertEqual(artifact.read_text(), 'json\n')


class SourceAndOutput(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='mainline-source-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        (self.root/'.gitignore').write_text('.local/\n')
        (self.root/'product.rs').write_text('committed')
        self.git('add', '.')
        self.git('-c', 'user.name=Synthetic', '-c', 'user.email=fixture@example.invalid',
                 '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.DEVNULL)

    def test_dirty_product_and_untracked_are_recorded(self):
        a = run.source(self.root)
        self.assertEqual(len(a['sha']), 40)
        self.assertFalse(a['dirty'])
        (self.root/'product.rs').write_text('changed')
        b = run.source(self.root)
        self.assertTrue(b['dirty'])
        self.assertTrue(b['tested_code_changed'])
        self.assertNotEqual(a['changes_sha256'], b['changes_sha256'])
        (self.root/'untracked').write_text('new')
        self.assertIn('untracked', run.source(self.root)['changed_paths'])
        self.assertEqual(run.totals([{'status':'passed'}], dirty=True)['status'], 'not_run')

    def test_output_protects_sources_existing_directory_and_symlinks(self):
        for path in ['product.rs', 'docs/report.json', '.local/report.json', '../out/report.json', '/tmp/out/report.json']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                run.reserve_output(self.root, path)
        out = run.reserve_output(self.root, '.local/first/report.json')
        out.write_text('preserve')
        with self.assertRaises(ValueError):
            run.reserve_output(self.root, '.local/first/other.json')
        self.assertEqual(out.read_text(), 'preserve')
        (self.root/'.local/link').symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            run.reserve_output(self.root, '.local/link/new/report.json')
        self.assertEqual((self.root/'product.rs').read_text(), 'committed')

    def test_run_records_actual_failure_and_dependency_not_run(self):
        checks = [Check('bad', 'runner', (sys.executable, '-c', 'raise SystemExit(9)')),
                  Check('blocked', 'runner', (sys.executable, '-c', 'print("never")'), depends=('bad',))]
        with patch.object(run, 'CHECKS', checks), patch.object(run, 'environment', return_value={'capabilities':{}}):
            self.assertEqual(run.run(['runner'], '.local/failure/report.json', self.root), 1)
        data = json.loads((self.root/'.local/failure/report.json').read_text())
        self.assertEqual(data['totals'], {'status':'failed','passed':0,'failed':1,'not_run':1})
        self.assertEqual(data['checks'][0]['exit_code'], 9)
        self.assertEqual(data['checks'][0]['source']['sha'], data['source']['sha'])
        self.assertIsNone(data['checks'][1]['exit_code'])

    def test_missing_capability_never_executes_fixture(self):
        check = Check('root', 'runner', (sys.executable, '-c', 'raise SystemExit(99)'), requires=('root',))
        with patch.object(run, 'CHECKS', [check]), patch.object(run, 'environment', return_value={'capabilities':{'root':False}}):
            self.assertEqual(run.run(['runner'], '.local/no-root/report.json', self.root), 1)
        r = json.loads((self.root/'.local/no-root/report.json').read_text())['checks'][0]
        self.assertEqual(r['execution'], 'not_run')
        self.assertIsNone(r['exit_code'])

    def test_midrun_source_mutation_fails_and_blocks_remaining(self):
        checks = [Check('mutate', 'runner', (sys.executable, '-c', "open('product.rs','w').write('changed')")),
                  Check('later', 'runner', (sys.executable, '-c', "open('should-not-exist','w').write('bad')"))]
        with patch.object(run, 'CHECKS', checks), patch.object(run, 'environment', return_value={'capabilities':{}}):
            self.assertEqual(run.run(['runner'], '.local/mutation/report.json', self.root), 1)
        data = json.loads((self.root/'.local/mutation/report.json').read_text())
        self.assertEqual(data['checks'][0]['status'], 'failed')
        self.assertTrue(data['checks'][0]['source_after']['tested_code_changed'])
        self.assertEqual(data['checks'][1]['status'], 'not_run')
        self.assertFalse((self.root/'should-not-exist').exists())

    def test_dirty_source_can_diagnose_but_never_passes_overall(self):
        (self.root/'product.rs').write_text('dirty')
        check = Check('ok', 'runner', (sys.executable, '-c', 'print("ok")'))
        with patch.object(run, 'CHECKS', [check]), patch.object(run, 'environment', return_value={'capabilities':{}}):
            self.assertEqual(run.run(['runner'], '.local/dirty/report.json', self.root), 1)
        data = json.loads((self.root/'.local/dirty/report.json').read_text())
        self.assertEqual(data['checks'][0]['status'], 'passed')
        self.assertTrue(data['checks'][0]['source']['dirty'])
        self.assertEqual(data['totals']['status'], 'not_run')


class SelectionAndCounts(unittest.TestCase):
    def test_unknown_empty_and_zero_selection(self):
        for groups in ([], ['unknown']):
            with self.assertRaises(ValueError):
                run.select_checks(groups)
        with patch.object(run, 'CHECKS', []), self.assertRaises(ValueError):
            run.select_checks(['ordinary'])
        p = subprocess.run([sys.executable, str(Path(run.__file__)), '--group', 'unknown'], capture_output=True)
        self.assertNotEqual(p.returncode, 0)

    def test_fixed_inventory_unique_and_dependencies_ordered(self):
        seen = set()
        for c in CHECKS:
            self.assertNotIn(c.name, seen)
            self.assertIn(c.group, GROUPS)
            self.assertTrue(set(c.depends) <= seen)
            seen.add(c.name)
            self.assertIsInstance(c.argv, tuple)
            self.assertNotIn('-c', c.argv)

    def test_zero_tests_fail_skips_not_run_and_rust_counts(self):
        base = {'execution':'executed','exit_code':0,'cleanup_complete':True,'summary':''}
        for kind, output in [('unittest','Ran 0 tests in 0.01s\nOK'), ('rust','test result: ok. 0 passed; 0 failed; 0 ignored;'), ('go','? package [no test files]')]:
            self.assertEqual(run.classify(dict(base, summary=output), kind), 'failed')
        r = dict(base, summary='Ran 3 tests in 0.1s\nOK (skipped=1)')
        self.assertEqual(run.classify(r, 'unittest'), 'not_run')
        r = dict(base, summary='test result: ok. 4 passed; 0 failed; 2 ignored;\ntest result: ok. 3 passed; 0 failed; 0 ignored;')
        self.assertEqual(run.classify(r, 'rust'), 'passed')
        self.assertEqual(r['test_counts'], {'passed':7,'failed':0,'ignored':2})
        self.assertEqual(run.totals([])['status'], 'not_run')
        self.assertEqual(run.totals([{'status':'passed'}], expected_count=4),
                         {'status':'not_run','passed':1,'failed':0,'not_run':3})
        self.assertEqual(run.totals([{'status':'passed'},{'status':'not_run'}])['status'], 'not_run')


if __name__ == '__main__':
    unittest.main()
