# SPDX-License-Identifier: AGPL-3.0-only
"""Real systemd condition/namespace ordering and recovery budget, isolated unit."""
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[2]
OVERLAY = ROOT/'firmware/system/overlay'
UNIT = OVERLAY/'etc/systemd/system/rustsimhub-app-recover.service'
SCRIPT = OVERLAY/'usr/local/libexec/rustsimhub-app-recover'


def real_systemd():
    return (getattr(os, 'geteuid', lambda: -1)() == 0 and pathlib.Path('/run/systemd/system').is_dir()
            and pathlib.Path('/proc/1/comm').read_text().strip() == 'systemd'
            and shutil.which('systemctl') is not None)


@unittest.skipUnless(real_systemd(), 'root and running systemd required')
class AppRecoverUnitTests(unittest.TestCase):
    def test_uninitialized_skip_precedes_namespace_and_initialized_budget_is_preserved(self):
        unit = 'rustsimhub-recover-fixture-'+uuid.uuid4().hex+'.service'
        unit_path = pathlib.Path('/run/systemd/system')/unit

        def ctl(*args, check=True):
            return subprocess.run(['systemctl', *args], check=check,
                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  text=True, timeout=30)

        def properties():
            result = ctl('show', unit, '-p', 'ConditionResult,Result,ExecMainStatus')
            return dict(line.split('=', 1) for line in result.stdout.splitlines())

        with tempfile.TemporaryDirectory(prefix='rustsimhub-recover-fixture-', dir='/run') as folder:
            base = pathlib.Path(folder)
            runtime = base/'runtime'
            complete = base/'installation.complete'
            calls = runtime/'calls'
            barriers = runtime/'barriers'
            fake = base/'fake-systemctl'
            fake.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> '+str(calls)+'\n')
            fake.chmod(0o755)
            sync = base/'fake-sync'
            sync.write_text('#!/bin/sh\nset -eu\ntest "$#" = 2\ntest "$1" = -f\n'
                            'printf "%s\\n" "$2" >> '+str(barriers)+'\n')
            sync.chmod(0o755)
            executable = base/'recover'
            script = SCRIPT.read_text().replace('/run/rustsimhub-app', str(runtime))
            script = script.replace('systemctl', str(fake)).replace('sync -f', str(sync)+' -f')
            executable.write_text(script)
            executable.chmod(0o755)
            source = UNIT.read_text().replace('/usr/local/libexec/rustsimhub-app-recover', str(executable))
            source = source.replace('/var/lib/rustsimhub/rustsimhub-v1/system/installation.complete', str(complete))
            source = source.replace('/run/rustsimhub-app', str(runtime))
            created = False
            try:
                with unit_path.open('x') as file:
                    file.write(source)
                created = True
                ctl('daemon-reload')
                skipped = ctl('start', unit, check=False)
                self.assertEqual(skipped.returncode, 0, skipped.stdout)
                self.assertEqual(properties()['ConditionResult'], 'no')
                self.assertFalse(runtime.exists(), 'No namespace setup or retry marker before initialization')

                complete.touch(mode=0o600)
                missing = ctl('start', unit, check=False)
                self.assertNotEqual(missing.returncode, 0)
                self.assertEqual(properties()['ConditionResult'], 'yes')
                self.assertEqual(properties()['ExecMainStatus'], '226', 'Missing runtime must still fail at namespace setup')
                self.assertFalse(runtime.exists())

                ctl('reset-failed', unit)
                runtime.mkdir(mode=0o755)
                runtime.chmod(0o755)
                first = ctl('start', unit, check=False)
                self.assertEqual(first.returncode, 0, first.stdout)
                marker = runtime/'recovery.used'
                self.assertEqual(marker.read_text(), 'RustSimHub.AppRecovery.v1\n')
                self.assertEqual(calls.read_text(), '--no-block restart rustsimhub-app.target\n')
                self.assertEqual(barriers.read_text().splitlines(), [str(marker), str(runtime)])

                second = ctl('start', unit, check=False)
                self.assertNotEqual(second.returncode, 0)
                self.assertEqual(marker.read_text(), 'RustSimHub.AppRecovery.v1\n')
                self.assertEqual(calls.read_text(), '--no-block restart rustsimhub-app.target\n'
                                 '--no-block stop rustsimhub-app.target\n')
            finally:
                if created:
                    ctl('stop', unit, check=False)
                    ctl('reset-failed', unit, check=False)
                    unit_path.unlink()
                    ctl('daemon-reload')
