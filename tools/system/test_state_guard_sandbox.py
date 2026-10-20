# SPDX-License-Identifier: AGPL-3.0-only
"""Real ext4 mount plus hidden /dev; no physical device or target board access."""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[2] / 'firmware/system/overlay/usr/local/libexec/rustsimhub-state-check'


def worker():
    def command(*args):
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT).strip()

    with tempfile.TemporaryDirectory(prefix='rustsimhub-state-sandbox-') as folder:
        base = pathlib.Path(folder)
        image = base/'fixture.ext4'
        with image.open('xb') as file:
            file.truncate(16*1024*1024)
        command('mkfs.ext4', '-q', '-F', '-L', 'NSHSTATE', str(image))
        state = base/'state'
        state.mkdir()
        mounted = hidden = False
        try:
            command('mount', '-o', 'loop', '-t', 'ext4', str(image), str(state))
            mounted = True
            source = command('findmnt', '-rn', '-M', str(state), '-o', 'SOURCE')
            number = command('findmnt', '-rn', '-M', str(state), '-o', 'MAJ:MIN')
            assert pathlib.Path(source).is_block_device()
            block = base/'sys-block'
            disk = block/'mmcblk0'
            part = disk/'mmcblk0p15'
            part.mkdir(parents=True)
            (block/'mmcblk0p15').symlink_to(part, target_is_directory=True)
            for name, value in {'dev':number, 'partition':'15', 'size':'264159', 'uevent':'PARTNAME=NSHSTATE'}.items():
                (part/name).write_text(value+'\n')
            compatible = base/'compatible'
            compatible.write_bytes(b'thwc,ufi003\0qcom,msm8916\0')
            guard = base/'guard'
            guard.write_text(SCRIPT.read_text().replace('/var/lib/rustsimhub', str(state))
                             .replace('/sys/class/block', str(block))
                             .replace('/proc/device-tree/compatible', str(compatible)))
            command('sh', str(guard))
            # Mount namespace is private. This reproduces PrivateDevices hiding
            # the physical device while leaving the already mounted state intact.
            command('mount', '-t', 'tmpfs', '-o', 'mode=0755,nosuid,noexec', 'tmpfs', '/dev')
            hidden = True
            assert not pathlib.Path(source).exists()
            command('sh', str(guard))
            (part/'dev').write_text('0:0\n')
            wrong = subprocess.run(['sh', str(guard)], capture_output=True)
            assert wrong.returncode != 0, 'Wrong mounted device accepted'
            (part/'dev').write_text(number+'\n')
            command('mount', '-o', 'remount,ro', str(state))
            readonly = subprocess.run(['sh', str(guard)], capture_output=True)
            assert readonly.returncode != 0, 'Readonly state accepted'
            print(json.dumps({'hidden_device_guard_passed':True, 'wrong_device_rejected':True,
                              'readonly_state_rejected':True, 'physical_devices_accessed':False}))
        finally:
            if hidden:
                command('umount', '/dev')
            if mounted:
                command('umount', str(state))
            assert not command('losetup', '-j', str(image)), 'Fixture loop remained attached'


@unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0 and
                     all(shutil.which(name) for name in ['unshare','mkfs.ext4','mount','losetup']),
                     'isolated Linux root mount test required')
class StateGuardSandboxTests(unittest.TestCase):
    def test_real_ext4_mount_with_hidden_physical_devices(self):
        result = subprocess.run(['unshare','--mount','--propagation','private',sys.executable,
                                 str(pathlib.Path(__file__).resolve()),'--sandbox-worker'],
                                capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertTrue(json.loads(result.stdout)['hidden_device_guard_passed'])


if __name__ == '__main__' and sys.argv[1:] == ['--sandbox-worker']:
    worker()
