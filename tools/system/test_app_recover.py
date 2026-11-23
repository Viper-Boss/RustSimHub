# SPDX-License-Identifier: AGPL-3.0-only
import os,pathlib,subprocess,tempfile,unittest
SCRIPT=pathlib.Path(__file__).resolve().parents[2]/'firmware/system/overlay/usr/local/libexec/rustsimhub-app-recover'

@unittest.skipUnless(os.geteuid()==0,'isolated root required')
class AppRecoverTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.runtime=self.base/'runtime';self.runtime.mkdir(mode=0o755);self.runtime.chmod(0o755)
        self.fake=self.base/'systemctl';self.calls=self.base/'calls'
        self.fake.write_text('#!/bin/sh\nif test "$1" = show; then echo failed; exit; fi\nprintf "%s\\n" "$*" >> '+str(self.calls)+'\n');self.fake.chmod(0o755)
        self.updater=self.base/'update';self.answer=self.base/'answer'
        self.answer.write_text('no fallback\n')
        self.updater.write_text('#!/bin/sh\ncat '+str(self.answer)+'\n');self.updater.chmod(0o755)
        # sync -f invokes syncfs on the entire containing filesystem. A shared
        # runner's unrelated writes must not determine this isolated fixture's
        # timeout. Record the requested barriers and explicitly test failure.
        self.sync=self.base/'sync';self.barriers=self.base/'barriers';self.fail_sync=self.base/'fail-sync'
        self.sync.write_text('#!/bin/sh\nset -eu\ntest "$#" = 2\ntest "$1" = -f\nprintf "%s\\n" "$2" >> '+str(self.barriers)+'\ntest ! -e '+str(self.fail_sync)+'\n');self.sync.chmod(0o755)
    def tearDown(self):self.tmp.cleanup()
    def run_recovery(self):
        text=SCRIPT.read_text().replace('/run/rustsimhub-app',str(self.runtime)).replace('systemctl',str(self.fake))
        text=text.replace('sync -f',str(self.sync)+' -f')
        text=text.replace('/usr/libexec/rustsimhub/rustsimhub-update',str(self.updater))
        return subprocess.run(['sh','-ec',text],capture_output=True,text=True,timeout=5)
    def test_one_retry_only_and_marker_is_preserved(self):
        self.assertEqual(self.run_recovery().returncode,0)
        marker=self.runtime/'recovery.used'
        self.assertEqual(marker.read_text(),'RustSimHub.AppRecovery.v2:1\n')
        self.assertNotEqual(self.run_recovery().returncode,0)
        self.assertEqual(marker.read_text(),'RustSimHub.AppRecovery.v2:2\n')
        self.assertEqual(self.calls.read_text(),'--no-block restart rustsimhub-app.target\n--no-block stop rustsimhub-app.target\n')
        barriers=self.barriers.read_text().splitlines()
        self.assertEqual(barriers[1::2],[str(self.runtime),str(self.runtime)])
        self.assertTrue(all(pathlib.Path(path).name.startswith('recovery.') for path in barriers[::2]))
    def test_failed_sync_never_restarts_group(self):
        self.fail_sync.touch()
        self.assertNotEqual(self.run_recovery().returncode,0)
        self.assertFalse(self.calls.exists())
        marker=self.runtime/'recovery.used'
        self.assertFalse(marker.exists())
        self.assertEqual(len(self.barriers.read_text().splitlines()),1)
        self.fail_sync.unlink()
        self.assertEqual(self.run_recovery().returncode,0)
        self.assertEqual(marker.read_text(),'RustSimHub.AppRecovery.v2:1\n')
    def test_two_backups_and_one_trial_are_the_maximum(self):
        self.answer.write_text('selected fallback\n')
        for _ in range(3):
            self.assertEqual(self.run_recovery().returncode,0)
        self.assertNotEqual(self.run_recovery().returncode,0)
        self.assertEqual(self.calls.read_text().splitlines(),
            ['--no-block restart rustsimhub-app.target']*3+['--no-block stop rustsimhub-app.target'])
    def test_linked_marker_or_writable_runtime_never_calls_systemctl(self):
        marker=self.runtime/'recovery.used';marker.symlink_to(self.base/'outside')
        self.assertNotEqual(self.run_recovery().returncode,0);self.assertFalse((self.base/'outside').exists())
        marker.unlink();self.runtime.chmod(0o777)
        self.assertNotEqual(self.run_recovery().returncode,0);self.assertFalse(self.calls.exists())
