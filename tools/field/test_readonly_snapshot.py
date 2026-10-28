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
import threading
import time
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
        self.root = base / 'root'
        for name, value in (('red:', '0'), ('green:wan', '0'), ('blue:', '1')):
            led = self.root / 'sys/class/leds' / name
            led.mkdir(parents=True)
            (led / 'brightness').write_text(value + '\n')
            (led / 'trigger').write_text('[none] timer heartbeat mmc0\n')
        conf = self.root / 'etc/rustsimhub/status-led.conf'
        conf.parent.mkdir(parents=True)
        conf.write_text('auto\n')
        hint = self.root / 'run/rustsimhub-led-input/state.json'
        hint.parent.mkdir(parents=True)
        hint.write_text('{"expires_ms": 123456, "configuring": false, "unread_sms": true,'
                        ' "missed_calls": false, "unexpected": "must-not-print"}')
        self.env = {'PATH': f'{bin_dir}:/usr/bin:/bin', 'RSH_FIELD_ROOT': str(self.root)}

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
        self.assertIn('led rgb=001 trigger=none,none,none mode=auto installed=no', out)
        self.assertIn('led-input expires_ms=123456 configuring=false unread_sms=true missed_calls=false', out)
        self.assertIn('led-update absent', out)
        self.assertNotIn('must-not-print', out)
        self.assert_read_only_calls()

    def test_led_trace_reports_changes_with_bounded_window(self):
        uptime = self.root / 'proc/uptime'
        uptime.parent.mkdir(parents=True, exist_ok=True)
        red = self.root / 'sys/class/leds/red:/brightness'
        start = time.monotonic()
        stop = threading.Event()

        def replace(path, text):
            temporary = path.with_name(path.name + '.tmp')
            temporary.write_text(text)
            os.replace(temporary, path)

        def clock():
            # Start at x.08 seconds: the fraction must not be parsed as octal.
            while not stop.is_set():
                elapsed = time.monotonic() - start
                replace(uptime, f'{100.08 + elapsed:.2f} 50.00\n')
                replace(red, '1\n' if 0.3 <= elapsed < 0.6 else '0\n')
                time.sleep(0.005)

        replace(uptime, '100.08 50.00\n')
        worker = threading.Thread(target=clock)
        worker.start()
        try:
            result = self.run_script('led-trace', '1')
        finally:
            stop.set()
            worker.join()
        self.assertEqual(result.returncode, 0, result.stderr)
        rows = re.findall(r'^t=(\d+) rgb=(\d{3})$', result.stdout, re.M)
        self.assertEqual([rgb for _, rgb in rows], ['001', '101', '001'], result.stdout)
        on, off = int(rows[1][0]), int(rows[2][0])
        self.assertTrue(20 <= on <= 45 and 20 <= off - on <= 45, rows)
        end = int(re.search(r'^led-trace-end t=(\d+)$', result.stdout, re.M).group(1))
        self.assertTrue(100 <= end <= 110, end)
        self.assertFalse(self.log.exists())

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
        for args in ([], ['watch'], ['watch', '0'], ['watch', '901'], ['watch', '5s'], ['dial'],
                     ['led-trace'], ['led-trace', '0'], ['led-trace', '121']):
            result = self.run_script(*args)
            self.assertEqual(result.returncode, 2, args)
        self.assertFalse(self.log.exists())


if __name__ == '__main__':
    unittest.main()
