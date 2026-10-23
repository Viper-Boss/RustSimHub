# SPDX-License-Identifier: AGPL-3.0-only
"""Exercise device filtering before/after enumeration with no audio hardware."""
import errno
import os
import pathlib
import stat
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[2]
UNIT = ROOT/'firmware/system/overlay/etc/systemd/system/rustsimhub-audio-routes.service'


@unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0 and
                     pathlib.Path('/run/systemd/system').is_dir(), 'Root systemd device fixture')
class AudioDeviceOrderTests(unittest.TestCase):
    def test_device_is_present_before_the_control_only_filter_is_installed(self):
        self.assertNotIn('4095 ', pathlib.Path('/proc/devices').read_text())
        tag = 'rustsimhub-audio-order-'+uuid.uuid4().hex[:10]
        consumer, producer = tag+'.service', tag+'-node.service'
        paths = [pathlib.Path('/run/systemd/system')/name for name in (consumer, producer)]
        node = pathlib.Path('/dev')/tag
        self.assertFalse(node.exists())

        def ctl(*args, check=True):
            return subprocess.run(['systemctl', *args], check=check, capture_output=True,
                                  text=True, timeout=15)

        def wait(predicate):
            end = time.monotonic()+8
            while time.monotonic() < end:
                if predicate():
                    return
                time.sleep(.05)
            self.fail('Synthetic device ordering fixture timed out')

        def remove_node():
            if node.exists():
                info = node.lstat()
                self.assertTrue(stat.S_ISCHR(info.st_mode))
                self.assertEqual((os.major(info.st_rdev), os.minor(info.st_rdev)), (4095, 250))
                node.unlink()

        created = []
        with tempfile.TemporaryDirectory(prefix=tag+'-', dir='/run') as temporary:
            base = pathlib.Path(temporary)
            ready, gate, output = [base/name for name in ('ready', 'go', 'errno')]
            script = base/'probe.py'
            script.write_text(
                'import os,pathlib,stat,sys,time\n'
                'node=pathlib.Path(sys.argv[2])\n'
                'if sys.argv[1]=="create":\n'
                ' time.sleep(.25)\n'
                ' os.mknod(node,stat.S_IFCHR|0o600,os.makedev(4095,250))\n'
                'else:\n'
                ' pathlib.Path(sys.argv[3]).touch()\n'
                ' end=time.monotonic()+5\n'
                ' while not pathlib.Path(sys.argv[4]).exists():\n'
                '  if time.monotonic()>end:raise TimeoutError()\n'
                '  time.sleep(.02)\n'
                ' try:\n'
                '  fd=os.open(node,os.O_RDONLY|os.O_NONBLOCK);os.close(fd);code=0\n'
                ' except OSError as e:code=e.errno\n'
                ' pathlib.Path(sys.argv[5]).write_text(str(code))\n')
            source = UNIT.read_text()
            # Preserve the production dependency relationship, replacing only
            # hardware and unrelated services with a synthetic producer.
            self.assertIn('dev-snd-controlC0.device', source)
            source = source.replace('rustsimhub-audio-drivers.service', '')
            source = source.replace('Wants=alsa-restore.service\n', '')
            source = source.replace('alsa-restore.service', '')
            source = source.replace('Before=rustsimhub-core.service rustsimhub-media.service\n', '')
            source = source.replace('dev-snd-controlC0.device', producer)
            source = source.replace('/dev/snd/controlC0', str(node))
            source = source.replace('/usr/libexec/rustsimhub/rustsimhub-audio-platform --prepare-routes',
                                    f'{sys.executable} {script} probe {node} {ready} {gate} {output}')
            source += f'\nReadWritePaths={base}\n'
            maker = ('[Service]\nType=oneshot\nRemainAfterExit=yes\n'
                     f'ExecStart={sys.executable} {script} create {node}\n')
            try:
                for path, text in zip(paths, (source.replace(producer, ''), maker)):
                    with path.open('x') as file:
                        file.write(text)
                    created.append(path)
                ctl('daemon-reload')
                ctl('start', '--no-block', consumer)
                wait(ready.exists)
                # The first cgroup was configured while the exact node was absent.
                os.mknod(node, stat.S_IFCHR | 0o600, os.makedev(4095, 250))
                gate.touch()
                wait(output.exists)
                self.assertEqual(int(output.read_text()), errno.EPERM,
                                 'Missing path must not gain permission after filter installation')
                ctl('stop', consumer)
                remove_node()
                for path in (ready, gate, output):
                    path.unlink()
                paths[0].write_text(source)
                gate.touch()
                ctl('daemon-reload')
                ctl('start', consumer)
                self.assertEqual(int(output.read_text()), errno.ENXIO,
                                 'Ordered path is allowed, but has no driver: no hardware was opened')
                self.assertEqual(ctl('show', '--value', '-p', 'CapabilityBoundingSet', consumer).stdout.strip(), '')
                self.assertEqual(ctl('show', '--value', '-p', 'DevicePolicy', consumer).stdout.strip(), 'closed')
            finally:
                for name in (consumer, producer):
                    ctl('stop', name, check=False)
                    ctl('reset-failed', name, check=False)
                remove_node()
                for path in created:
                    path.unlink()
                ctl('daemon-reload')
