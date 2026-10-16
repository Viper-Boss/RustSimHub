# SPDX-License-Identifier: AGPL-3.0-only
import os,pathlib,subprocess,tempfile,unittest
SCRIPT=pathlib.Path(__file__).resolve().parents[2]/'firmware/system/overlay/usr/local/libexec/rustsimhub-app-recover'

@unittest.skipUnless(os.geteuid()==0,'isolated root required')
class AppRecoverTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.runtime=self.base/'runtime';self.runtime.mkdir(mode=0o755)
        self.fake=self.base/'systemctl';self.calls=self.base/'calls'
        self.fake.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> '+str(self.calls)+'\n');self.fake.chmod(0o755)
    def tearDown(self):self.tmp.cleanup()
    def run_recovery(self):
        text=SCRIPT.read_text().replace('/run/rustsimhub-app',str(self.runtime)).replace('systemctl',str(self.fake))
        return subprocess.run(['sh','-ec',text],capture_output=True,text=True,timeout=5)
    def test_one_retry_only_and_marker_is_preserved(self):
        self.assertEqual(self.run_recovery().returncode,0)
        marker=self.runtime/'recovery.used';before=marker.read_bytes()
        self.assertNotEqual(self.run_recovery().returncode,0)
        self.assertEqual(marker.read_bytes(),before)
        self.assertEqual(self.calls.read_text(),'--no-block restart rustsimhub-app.target\n--no-block stop rustsimhub-app.target\n')
    def test_linked_marker_or_writable_runtime_never_calls_systemctl(self):
        marker=self.runtime/'recovery.used';marker.symlink_to(self.base/'outside')
        self.assertNotEqual(self.run_recovery().returncode,0);self.assertFalse((self.base/'outside').exists())
        marker.unlink();self.runtime.chmod(0o777)
        self.assertNotEqual(self.run_recovery().returncode,0);self.assertFalse(self.calls.exists())
