# SPDX-License-Identifier: AGPL-3.0-only
"""Run the real state guard with fixed synthetic kernel/mount metadata."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'firmware/system/overlay/usr/local/libexec/rustsimhub-state-check'


class StateGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = pathlib.Path(self.tmp.name)
        self.state = self.base / 'state'
        self.state.mkdir()
        self.block = self.base / 'block'
        self.disk = self.block / 'mmcblk0'
        self.part = self.disk / 'mmcblk0p15'
        self.part.mkdir(parents=True)
        (self.block / 'mmcblk0p15').symlink_to(self.part, target_is_directory=True)
        for name, value in {'dev': '179:15\n', 'partition': '15\n', 'size': '264159\n',
                            'uevent': 'MAJOR=179\nMINOR=15\nPARTNAME=NSHSTATE\n'}.items():
            (self.part / name).write_text(value)
        self.compat = self.base / 'compatible'
        self.compat.write_bytes(b'thwc,ufi003\0qcom,msm8916\0')
        self.bin = self.base / 'bin'
        self.bin.mkdir()
        commands = {
            'mountpoint': '#!/bin/sh\nexit "${MOUNT_EXIT:-0}"\n',
            'findmnt': '''#!/bin/sh
case "$*" in
 *MAJ:MIN) printf '%s\\n' "${MOUNT_DEV:-179:15}";;
 *FSTYPE) printf '%s\\n' "${MOUNT_FS:-ext4}";;
 *LABEL) printf '%s\\n' "${MOUNT_LABEL:-NSHSTATE}";;
 *OPTIONS) printf '%s\\n' "${MOUNT_OPTIONS:-rw,relatime}";;
 *) exit 2;;
esac
''',
            'stat': '#!/bin/sh\nif test "$3" = /; then echo 1; else echo "${STATE_ST_DEV:-2}"; fi\n',
        }
        for name, text in commands.items():
            file = self.bin / name
            file.write_text(text)
            file.chmod(0o755)
        text = SCRIPT.read_text().replace('/var/lib/rustsimhub', str(self.state))
        text = text.replace('/sys/class/block', str(self.block)).replace('/proc/device-tree/compatible', str(self.compat))
        self.script = self.base / 'guard'
        self.script.write_text(text)

    def tearDown(self):
        self.tmp.cleanup()

    def check(self, **overrides):
        env = dict(os.environ, PATH=str(self.bin)+':'+os.environ['PATH'], **overrides)
        return subprocess.run(['sh', str(self.script)], env=env, capture_output=True, text=True, timeout=5)

    def test_private_devices_requires_no_physical_device_node(self):
        # No /dev aliases are substituted or created. The real guard uses only
        # kernel partition identity and mounted filesystem metadata.
        self.assertNotIn('/dev/', SCRIPT.read_text())
        self.assertEqual(self.check().returncode, 0)
        unit = (ROOT / 'firmware/system/overlay/etc/systemd/system/rustsimhub-ssh-prepare.service').read_text()
        self.assertIn('PrivateDevices=yes', unit)
        self.assertIn('CapabilityBoundingSet=\n', unit)

    def test_wrong_partition_or_mount_metadata_is_rejected(self):
        for override in [{'MOUNT_DEV':'179:14'}, {'MOUNT_FS':'tmpfs'}, {'MOUNT_LABEL':'rootfs'},
                         {'MOUNT_OPTIONS':'ro,relatime'}, {'MOUNT_EXIT':'1'}, {'STATE_ST_DEV':'1'}]:
            with self.subTest(override=override):
                self.assertNotEqual(self.check(**override).returncode, 0)

    def test_wrong_board_partition_label_size_or_parent_is_rejected(self):
        for name, bad in [('partition','14\n'), ('size','264158\n'), ('uevent','PARTNAME=rootfs\n')]:
            file = self.part / name
            before = file.read_bytes()
            file.write_text(bad)
            self.assertNotEqual(self.check().returncode, 0, name)
            file.write_bytes(before)
        self.compat.write_bytes(b'other,board\0')
        self.assertNotEqual(self.check().returncode, 0)
        self.compat.write_bytes(b'thwc,ufi003\0')
        (self.block/'mmcblk0p15').unlink()
        outside = self.base/'other/mmcblk0p15'
        outside.mkdir(parents=True)
        (self.block/'mmcblk0p15').symlink_to(outside, target_is_directory=True)
        self.assertNotEqual(self.check().returncode, 0)

    def test_state_symlink_and_arguments_are_rejected(self):
        self.assertNotEqual(subprocess.run(['sh', str(self.script), 'other'], capture_output=True).returncode, 0)
        self.state.rmdir()
        self.state.symlink_to(self.base, target_is_directory=True)
        self.assertNotEqual(self.check().returncode, 0)
