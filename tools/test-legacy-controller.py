#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run guard shell failure paths with fake commands; never touches systemd/modem."""
import os
import pathlib
import subprocess
import tempfile
import unittest
from legacy_controller import scripts

BASE = '/var/tmp/rustsimhub-web-media-Abcd1234/operator/controller-0123456789abcdef'
CORE = 'rustsimhub-web-media-0123456789-core'


class Validation(unittest.TestCase):
    def test_rejects_unsafe_scope(self):
        for core, base, seconds in [('ModemManager', BASE, 75), (CORE, '/tmp/x', 75), (CORE, BASE, 901), (CORE, BASE, 0)]:
            with self.assertRaises(ValueError):
                scripts(core, base, seconds)


@unittest.skipUnless(pathlib.Path('/bin/sh').exists(), 'Shell behavior runs in Linux/WSL')
class ShellBehavior(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.env = dict(os.environ, PATH=str(self.root)+os.pathsep+os.environ['PATH'], TRIAL=str(self.root))
        self.state = self.root/'agent-state'
        self.state.write_text('active')
        self.env['CALLS'] = 'No calls were found.'
        self.command('mmcli', '#!/bin/sh\nprintf "%s\\n" "$CALLS"\n')
        self.command('sleep', '#!/bin/sh\nexit 0\n')
        self.command('systemctl', '''#!/usr/bin/env python3
import os,pathlib,sys
r=pathlib.Path(os.environ['TRIAL']);args=sys.argv[1:]
with (r/'commands').open('a') as f:f.write(' '.join(args)+'\\n')
if args[0]=='is-active':
 if args[-1]=='nassimhub-agent.service':
  state=(r/'agent-state').read_text();print(state);sys.exit(0 if state=='active' else 3)
 print('active\\nactive\\nactive');sys.exit(0)
if args[0]=='show':print('0');sys.exit(0)
if args[-1]=='nassimhub-agent.service':
 if args[0]=='stop' and os.environ.get('STOP_FAIL'):sys.exit(4)
 (r/'agent-state').write_text('active' if args[0]=='start' else os.environ.get('STOP_STATE','inactive'))
''')
        start, restore = scripts(CORE, BASE, 75)
        self.start = start.replace(BASE, str(self.root))
        self.restore = restore.replace(BASE, str(self.root))

    def command(self, name, text):
        path = self.root/name
        path.write_text(text)
        path.chmod(0o700)

    def execute(self, script):
        return subprocess.run(['/bin/sh'], input=script, text=True, env=self.env, capture_output=True)

    def test_active_and_idempotent_restore(self):
        self.assertEqual(self.execute(self.start).returncode, 0)
        self.assertEqual(self.state.read_text(), 'inactive')
        self.assertTrue((self.root/'ready').exists())
        self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertEqual(self.state.read_text(), 'active')
        commands = (self.root/'commands').read_text()
        self.assertLess(commands.index('stop '+CORE), commands.index('start nassimhub-agent.service'))
        self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertEqual((self.root/'commands').read_text(), commands)

    def test_initially_inactive_stays_inactive(self):
        self.state.write_text('inactive')
        self.assertEqual(self.execute(self.start).returncode, 0)
        self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertEqual(self.state.read_text(), 'inactive')
        self.assertNotIn('start nassimhub-agent', (self.root/'commands').read_text())

    def test_legacy_stop_post_failure_with_no_process_is_stopped(self):
        self.env['STOP_STATE'] = 'failed'
        self.assertEqual(self.execute(self.start).returncode, 0)
        self.assertTrue((self.root/'ready').exists())
        self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertEqual(self.state.read_text(), 'active')

    def test_live_incoming_and_outgoing_refused_before_mutation(self):
        for direction in ['incoming', 'outgoing']:
            self.env['CALLS'] = '/org/freedesktop/ModemManager1/Call/123 '+direction+' (active)'
            self.assertNotEqual(self.execute(self.start).returncode, 0)
            self.assertFalse((self.root/'original').exists())
            self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertNotIn('stop ', (self.root/'commands').read_text())

    def test_stop_failure_keeps_restore_intent(self):
        self.env['STOP_FAIL'] = '1'
        self.assertNotEqual(self.execute(self.start).returncode, 0)
        self.assertFalse((self.root/'ready').exists())
        self.assertEqual(self.execute(self.restore).returncode, 0)
        self.assertEqual(self.state.read_text(), 'active')


if __name__ == '__main__':
    unittest.main()
