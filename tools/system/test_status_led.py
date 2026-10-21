# SPDX-License-Identifier: AGPL-3.0-only
"""Exercise the fixed LED helper using fixture files, never host GPIO/sysfs."""
import os,pathlib,subprocess,tempfile,unittest

ROOT=pathlib.Path(__file__).resolve().parents[2]
SCRIPT=ROOT/'firmware/system/overlay/usr/local/libexec/rustsimhub-status-led'

@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Linux root fixture required')
class StatusLedTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.tmp.name)
        self.led=self.root/'sys/devices/platform/leds/leds/green:wan';self.led.mkdir(parents=True)
        self.driver=self.root/'sys/bus/platform/drivers/leds-gpio';self.driver.mkdir(parents=True)
        (self.led/'device').mkdir();(self.led/'device/driver').symlink_to(self.driver)
        alias=self.root/'sys/class/leds/green:wan';alias.parent.mkdir(parents=True);alias.symlink_to(self.led)
        self.board=self.root/'sys/firmware/devicetree/base/compatible';self.board.parent.mkdir(parents=True)
        self.board.write_bytes(b'thwc,ufi003\0qcom,msm8916\0')
        self.config=self.root/'etc/rustsimhub/status-led.conf';self.config.parent.mkdir(parents=True)
        self.config.write_text('heartbeat\n');self.config.chmod(0o644)
        for name,value in [('max_brightness','1\n'),('brightness','0\n'),('trigger','[none] timer heartbeat mmc0\n')]:
            (self.led/name).write_text(value)
        self.script=self.root/'helper'
        self.script.write_text(SCRIPT.read_text().replace('/sys/',str(self.root/'sys')+'/').replace('/etc/rustsimhub/',str(self.config.parent)+'/'))
    def tearDown(self):self.tmp.cleanup()
    def invoke(self,*args):return subprocess.run(['sh',str(self.script),*args],capture_output=True,text=True)
    def test_heartbeat_and_stop_use_kernel_trigger(self):
        result=self.invoke();self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.led/'trigger').read_text(),'heartbeat\n')
        result=self.invoke('--off');self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.led/'trigger').read_text(),'none\n');self.assertEqual((self.led/'brightness').read_text(),'0\n')
    def test_explicit_off_configuration(self):
        self.config.write_text('off\n');(self.led/'brightness').write_text('1\n')
        self.assertEqual(self.invoke().returncode,0)
        self.assertEqual((self.led/'brightness').read_text(),'0\n')
    def test_wrong_board_driver_or_trigger_refuses_before_writing(self):
        before=(self.led/'trigger').read_text()
        self.board.write_bytes(b'other,board\0');self.assertNotEqual(self.invoke().returncode,0)
        self.assertEqual((self.led/'trigger').read_text(),before)
        self.board.write_bytes(b'thwc,ufi003\0')
        (self.led/'device/driver').unlink();(self.led/'device/driver').symlink_to(self.root)
        self.assertNotEqual(self.invoke().returncode,0);self.assertEqual((self.led/'trigger').read_text(),before)
        (self.led/'device/driver').unlink();(self.led/'device/driver').symlink_to(self.driver)
        (self.led/'trigger').write_text('[none] timer notheartbeat\n')
        self.assertNotEqual(self.invoke().returncode,0)
        self.assertEqual((self.led/'trigger').read_text(),'[none] timer notheartbeat\n')
    def test_untrusted_config_and_arbitrary_action_refused(self):
        before=(self.led/'trigger').read_text()
        self.config.write_text('heartbeat; touch '+str(self.root/'injected'))
        self.assertNotEqual(self.invoke().returncode,0);self.assertFalse((self.root/'injected').exists())
        self.config.write_text('heartbeat\n');self.config.chmod(0o666)
        self.assertNotEqual(self.invoke().returncode,0)
        self.config.chmod(0o644);self.config.rename(self.config.with_suffix('.saved'))
        self.config.symlink_to(self.config.with_suffix('.saved'))
        self.assertNotEqual(self.invoke().returncode,0)
        self.assertNotEqual(self.invoke('/sys/class/leds/red:').returncode,0)
        self.assertEqual((self.led/'trigger').read_text(),before)

if __name__=='__main__':unittest.main()
