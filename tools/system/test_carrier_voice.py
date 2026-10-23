# SPDX-License-Identifier: AGPL-3.0-only
"""Exercise actual startup scripts with isolated synthetic files, never a modem."""
import hashlib, os, pathlib, subprocess, tempfile, unittest

OVERLAY=pathlib.Path(__file__).resolve().parents[2]/'firmware/system/overlay'

class ProfileTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.tmp.name)
        self.base=self.root/'private-modem';self.base.mkdir(mode=0o700)
        self.profile=self.base/'local-volte';self.profile.mkdir(mode=0o700)
        self.marker=self.root/'enabled';self.marker.write_text('RustSimHub.Ufi003.Unicom.VoLTE.v1\n');self.marker.chmod(0o600)
        self.board=self.root/'compatible';self.board.write_bytes(b'thwc,ufi003\0qcom,msm8916\0')
        rows=[]
        for name in ['modem_fs1','modem_fs2','modem_fsc','modem_fsg']:
            data=('synthetic-'+name).encode();p=self.profile/name;p.write_bytes(data);p.chmod(0o600)
            rows.append(hashlib.sha256(data).hexdigest()+'  '+name)
        sums=self.profile/'SHA256SUMS';sums.write_text('\n'.join(rows)+'\n');sums.chmod(0o600)
        self.helper=self.root/'rmtfs';self.helper.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n');self.helper.chmod(0o700)
    def tearDown(self):self.tmp.cleanup()
    def run_script(self,check=False):
        text=(OVERLAY/'usr/local/libexec/rustsimhub-rmtfs-profile').read_text()
        text=text.replace('owner=0','owner='+str(os.getuid())).replace('/usr/local/libexec/rustsimhub-state-check','true').replace('/var/lib/rustsimhub/rustsimhub-v1/system/carrier-voice.enabled',str(self.marker)).replace('/var/lib/rustsimhub/.private-modem',str(self.base)).replace('/proc/device-tree/compatible',str(self.board)).replace('/usr/bin/rmtfs',str(self.helper))
        script=self.root/'script';script.write_text(text)
        return subprocess.run(['sh',str(script)]+(['--check'] if check else []),capture_output=True,text=True)
    def test_opt_in_selects_exact_read_only_shadow_and_check_does_not_start(self):
        p=self.run_script();self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(p.stdout.splitlines()[-4:],['-r','-o',str(self.profile),'-s'])
        p=self.run_script(True);self.assertEqual(p.returncode,0,p.stderr);self.assertNotIn('-o',p.stdout)
    def test_absent_marker_preserves_original_read_only_mode(self):
        self.marker.unlink();p=self.run_script();self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(p.stdout.splitlines(),['-r','-P','-s'])
    def test_bad_checksum_does_not_silently_fall_back(self):
        (self.profile/'modem_fs1').write_bytes(b'corrupted');self.assertNotEqual(self.run_script().returncode,0)
    def test_unknown_marker_and_foreign_board_refuse(self):
        self.marker.write_text('unknown');self.assertNotEqual(self.run_script().returncode,0)
        self.marker.write_text('RustSimHub.Ufi003.Unicom.VoLTE.v1\n');self.board.write_bytes(b'foreign\0');self.assertNotEqual(self.run_script().returncode,0)
    def test_wrong_permissions_and_symlink_refuse(self):
        file=self.profile/'modem_fs1';file.chmod(0o644);self.assertNotEqual(self.run_script().returncode,0)
        file.chmod(0o600);other=self.root/'other';file.rename(other);file.symlink_to(other);self.assertNotEqual(self.run_script().returncode,0)
    def test_duplicate_or_traversal_manifest_refuses_before_access(self):
        file=self.profile/'SHA256SUMS';original=file.read_text();file.write_text(original+original.splitlines()[0]+'\n');self.assertNotEqual(self.run_script().returncode,0)
        file.write_text(original.replace('modem_fsg','../../outside'));self.assertNotEqual(self.run_script().returncode,0)

class CarrierTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.tmp.name);self.bin=self.root/'bin';self.bin.mkdir()
        self.state=self.root/'domain';self.state.write_text('2');self.home=self.root/'home';self.home.write_text('46001')
        self.calls=self.root/'calls';self.calls.write_text('No calls were found\n');self.owned=self.root/'intent';self.log=self.root/'mutations';self.lost=self.root/'lost'
        marker=self.root/'enabled';marker.write_text('synthetic-enabled');self.marker=marker
        mm=self.bin/'mmcli';mm.write_text('''#!/usr/bin/python3
import pathlib,sys
r=pathlib.Path(__file__).resolve().parent.parent
if '--voice-list-calls' in sys.argv:print((r/'calls').read_text(),end='')
elif '-i' in sys.argv:print('sim.properties.operator-code : '+(r/'home').read_text())
else:print('modem.generic.sim : /org/freedesktop/ModemManager1/SIM/0')
''');mm.chmod(0o700)
        self.helper=self.root/'helper';self.helper.write_text('''#!/usr/bin/python3
import pathlib,sys
r=pathlib.Path(__file__).resolve().parent;op=sys.argv[1];current=(r/'domain').read_text()
if op=='--voice-domain-get':print('voice-domain='+current);sys.exit(0)
suffix='ps' if op=='--voice-domain-ps-preferred' else 'cs'
assert (r/'intent').read_text().strip()=='RustSimHub.CarrierVoice.Intent.v1.'+suffix
with (r/'mutations').open('a') as f:f.write(op+'\\n')
(r/'domain').write_text('3' if suffix=='ps' else '2')
if (r/'lost').exists():sys.exit(1)
print('runtime voice domain requested='+('3' if suffix=='ps' else '2'))
''');self.helper.chmod(0o700)
    def tearDown(self):self.tmp.cleanup()
    def once(self):
        text=(OVERLAY/'usr/local/libexec/rustsimhub-carrier-voice').read_text()
        text=text.replace('/usr/libexec/rustsimhub/rustsimhub-network-agent',str(self.helper)).replace('/run/rustsimhub-carrier-voice/ps-applied',str(self.owned)).replace('/usr/local/libexec/rustsimhub-rmtfs-profile --check','true').replace('/var/lib/rustsimhub/rustsimhub-v1/system/carrier-voice.enabled',str(self.marker)).replace('0:0:600:1',f'{os.getuid()}:{os.getgid()}:600:1').replace('while :; do sample; sleep 15 & wait $!; done','sample')
        script=self.root/'script';script.write_text(text)
        return subprocess.run(['sh',str(script)],env={**os.environ,'PATH':str(self.bin)+':'+os.environ['PATH']},capture_output=True,text=True)
    def mutations(self):return self.log.read_text().splitlines() if self.log.exists() else []
    def test_only_one_attempt_after_lost_response_and_retained_terminal_objects(self):
        self.calls.write_text('/org/freedesktop/ModemManager1/Call/0 outgoing (terminated)\n');self.lost.touch()
        self.once();self.assertEqual(self.state.read_text(),'3');self.assertTrue(self.owned.exists())
        self.once();self.assertEqual(self.mutations(),['--voice-domain-ps-preferred'])
    def test_preexisting_ps_and_other_carrier_are_not_owned(self):
        self.state.write_text('3');self.once();self.home.write_text('46000');self.once();self.assertEqual(self.mutations(),[]);self.assertFalse(self.owned.exists())
    def test_own_baseline_restored_on_carrier_change(self):
        self.once();self.home.write_text('46000');self.once();self.assertEqual(self.state.read_text(),'2');self.assertFalse(self.owned.exists());self.assertEqual(len(self.mutations()),2)
    def test_lost_restore_is_observed_not_repeated(self):
        self.once();self.home.write_text('46000');self.lost.touch();self.once();self.assertTrue(self.owned.exists());self.once();self.assertFalse(self.owned.exists());self.assertEqual(len(self.mutations()),2)
    def test_live_malformed_or_oversized_catalogue_blocks_mutation(self):
        for calls in ['','invalid\n','/org/freedesktop/ModemManager1/Call/1 incoming (ringing-in)\n','/org/freedesktop/ModemManager1/Call/1 outgoing (terminated)\n'*65]:
            self.calls.write_text(calls);self.once();self.assertEqual(self.mutations(),[]);self.assertFalse(self.owned.exists())
    def test_unknown_home_baseline_and_malformed_owned_intent_block(self):
        self.home.write_text('--');self.once();self.assertEqual(self.mutations(),[])
        self.home.write_text('46001');self.state.write_text('0');self.once();self.assertEqual(self.mutations(),[])
        self.state.write_text('2');self.owned.write_text('invalid');self.owned.chmod(0o600);self.once();self.assertEqual(self.mutations(),[])

if __name__=='__main__':unittest.main()
