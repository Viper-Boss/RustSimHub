# SPDX-License-Identifier: AGPL-3.0-only
"""Run the real field snapshot script against stub system commands.

Every external call is logged and must match the read-only allowlist; phone
numbers from the stubbed call objects must never reach the output.
"""
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent / 'rsh-readonly-snapshot.sh'
NUMBER = '13800009999'
ALLOWED = [
    re.compile(r'systemctl is-active [A-Za-z0-9-]+'),
    re.compile(r'mmcli -m any --voice-list-calls --output-keyvalue'),
    re.compile(r'mmcli -o /org/freedesktop/ModemManager1/Call/[0-9]+ --output-keyvalue'),
    re.compile(r'nmcli -t -f DEVICE,TYPE,STATE device status'),
]
STUBS = {
    'systemctl': '[ "$2" = nassimhub-agent ] && { echo inactive; exit 3; }; echo active',
    'mmcli': f'''if [ "${{RSH_STUB_MM_FAIL:-}}" = 1 ]; then exit 1; fi
case "$*" in
  *--voice-list-calls*) printf 'modem.voice.calls.value[1] : /org/freedesktop/ModemManager1/Call/7\\n';;
  *Call/7*) printf 'call.dbus-path : /org/freedesktop/ModemManager1/Call/7\\ncall.properties.number : {NUMBER}\\ncall.properties.direction : incoming\\ncall.properties.multiparty : no\\ncall.properties.state : ringing-in\\ncall.properties.state-reason : incoming-new\\n';;
esac''',
    'nmcli': 'printf "wlan0:wifi:connected\\nwwan0qmi0:gsm:disconnected\\n"',
}


@unittest.skipUnless(sys.platform.startswith('linux'), 'field snapshot runs on the Linux device shell')
class ReadOnlySnapshot(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.log = base / 'calls.log'
        bin_dir = base / 'bin'
        bin_dir.mkdir()
        for name, body in STUBS.items():
            path = bin_dir / name
            path.write_text(f'#!/bin/sh\necho "{name} $*" >> "{self.log}"\n{body}\n')
            path.chmod(0o755)
        for pcm, state in (('pcm0p', 'closed'), ('pcm0c', 'RUNNING')):
            status = base / 'root/proc/asound/card0' / pcm / 'sub0/status'
            status.parent.mkdir(parents=True)
            status.write_text(state if state == 'closed' else f'state: {state}\nowner_pid   : 1\n')
        self.env = {'PATH': f'{bin_dir}:/usr/bin:/bin', 'RSH_FIELD_ROOT': str(base / 'root')}

    def tearDown(self):
        self.tmp.cleanup()

    def run_script(self, *args, extra=None):
        env = dict(self.env, **(extra or {}))
        return subprocess.run(['/bin/sh', str(SCRIPT), *args], env=env, capture_output=True,
                              text=True, timeout=30)

    def assert_read_only_calls(self):
        calls = self.log.read_text().splitlines() if self.log.exists() else []
        self.assertTrue(calls)
        for call in calls:
            self.assertTrue(any(p.fullmatch(call) for p in ALLOWED), call)

    def test_once_reports_state_without_numbers(self):
        result = self.run_script('once')
        self.assertEqual(result.returncode, 0, result.stderr)
        out = result.stdout
        self.assertIn('service rustsimhub-core active', out)
        self.assertIn('service nassimhub-agent inactive', out)
        self.assertIn('call 7 direction=incoming state=ringing-in state-reason=incoming-new', out)
        self.assertIn('pcm card0/pcm0p/sub0 closed', out)
        self.assertIn('pcm card0/pcm0c/sub0 RUNNING', out)
        self.assertIn('net wwan0qmi0:gsm:disconnected', out)
        self.assertNotIn(NUMBER, out)
        self.assertNotIn('multiparty', out)
        self.assert_read_only_calls()

    def test_unavailable_modem_is_reported_not_guessed(self):
        result = self.run_script('once', extra={'RSH_STUB_MM_FAIL': '1'})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('calls unavailable', result.stdout)
        self.assertNotIn('calls none', result.stdout)

    def test_watch_is_bounded_and_prints_changes_once(self):
        result = self.run_script('watch', '2')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count('snapshot '), 1)
        self.assertIn('watch-end ', result.stdout)
        self.assertNotIn(NUMBER, result.stdout)
        self.assert_read_only_calls()

    def test_invalid_arguments_are_rejected_before_any_call(self):
        for args in ([], ['watch'], ['watch', '0'], ['watch', '901'], ['watch', '5s'], ['dial']):
            result = self.run_script(*args)
            self.assertEqual(result.returncode, 2, args)
        self.assertFalse(self.log.exists())


if __name__ == '__main__':
    unittest.main()
