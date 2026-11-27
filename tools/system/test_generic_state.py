# SPDX-License-Identifier: AGPL-3.0-only
"""Generic NSHSTATE builder and first-boot install wrapper; synthetic keys, no device."""
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('build_generic_state', ROOT/'tools/system/build-generic-state.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
WRAPPER = ROOT/'firmware/system/overlay/usr/local/libexec/rustsimhub-generic-install'
UNITS = ROOT/'firmware/system/overlay/etc/systemd/system'


class ContractTests(unittest.TestCase):
    def test_image_is_block_aligned_and_smaller_than_the_fixed_partition(self):
        self.assertEqual(module.IMAGE_BYTES % 16384, 0)
        self.assertEqual(module.IMAGE_BYTES % 4096, 0)
        self.assertLess(module.IMAGE_BYTES, 264159*512)
        self.assertGreater(module.IMAGE_BYTES+16384, 264159*512)

    def test_generic_rootfs_size_fits_the_flash_layout(self):
        spec = importlib.util.spec_from_file_location('system_image', ROOT/'tools/build-system-image.py')
        image = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(image)
        self.assertEqual(image.GENERIC_IMAGE_BYTES % 16384, 0)
        self.assertLessEqual(image.GENERIC_IMAGE_BYTES, 2845656*512)
        self.assertGreater(image.GENERIC_IMAGE_BYTES+16384, 2845656*512)

    def test_first_boot_units_order_extract_and_install_correctly(self):
        extract = (UNITS/'rustsimhub-firmware-extract.service').read_text()
        self.assertIn('Before=rustsimhub-device-firmware.service', extract)
        self.assertNotIn('ConditionPathExists=!/var/lib/rustsimhub/.private-firmware', extract)
        install = (UNITS/'rustsimhub-generic-install.service').read_text()
        for name in ('rustsimhub-app-select.service', 'rustsimhub-core.service', 'rustsimhub-edge.service', 'rustsimhub-mdns.service'):
            self.assertIn(name, install.split('Before=')[1].splitlines()[0])
        self.assertIn('ConditionPathExists=!/var/lib/rustsimhub/rustsimhub-v1/system/installation.complete', install)

    def test_cache_is_slot_local_only_for_generic_ab(self):
        helper = ROOT/'firmware/system/overlay/usr/local/libexec/rustsimhub-firmware-cache-path'
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            source = helper.read_text().replace('/usr/share/rustsimhub', str(root/'release'))
            source = source.replace('/var/lib/rustsimhub', str(root/'state'))
            source = source.replace('/var/cache/rustsimhub', str(root/'cache'))
            marker = root/'release/ab-system.json'
            request = root/'state/rustsimhub-v1/system/generic-provision.request'
            marker.parent.mkdir()
            request.parent.mkdir(parents=True)
            legacy = str(root/'state/.private-firmware')
            def selected():
                return subprocess.check_output(['sh', '-c', source], text=True).strip()
            self.assertEqual(selected(), legacy)
            request.touch()
            self.assertEqual(selected(), legacy)
            marker.touch()
            self.assertEqual(selected(), str(root/'cache/device-firmware'))
            request.unlink()
            self.assertEqual(selected(), legacy)

    def test_usb_dhcp_offers_only_the_host_address_without_gateway_or_dns(self):
        unit = (UNITS/'rustsimhub-usb-dhcp.service').read_text()
        command = ' '.join(line.rstrip(chr(92)).strip() for line in unit.split('ExecStart=')[1].split('Restart=')[0].splitlines())
        for option in ('--port=0', '--interface=usb0', '--conf-file=/dev/null', '--no-resolv', '--leasefile-ro',
                       '--dhcp-range=10.55.0.1,10.55.0.1,255.255.255.248,12h', '--dhcp-option=option:router',
                       '--dhcp-option=option:dns-server', '--user=nobody'):
            self.assertIn(option, command.split())
        self.assertNotIn('wlan0', command)
        self.assertNotIn('wwan', command)
        self.assertIn('rustsimhub-usb-dhcp.service', (ROOT/'tools/compose-system-tree.py').read_text())


@unittest.skipUnless(os.geteuid() == 0 and shutil.which('mke2fs') and shutil.which('ssh-keygen'), 'needs root and e2fsprogs')
class BuilderTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name)
        key = self.root/'id'
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-C', 'synthetic', '-f', str(key)], check=True)
        self.private = key
        self.public = self.root/'id.pub'

    def tearDown(self):
        self.tmp.cleanup()

    def sha(self, path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def test_image_holds_only_the_public_key_and_request(self):
        report = module.build(self.public, self.sha(self.public), self.root/'out')
        image = self.root/'out/NSHSTATE.img'
        self.assertTrue(report['passed'])
        self.assertEqual(image.stat().st_size % 16384, 0)
        self.assertEqual(set(report['tree']), module.EXPECTED_TREE)
        dump = self.root/'dump'
        subprocess.run(['debugfs', '-R', 'dump /rustsimhub-v1/system/ssh-access/authorized_keys '+str(dump), str(image)],
                       check=True, capture_output=True)
        self.assertEqual(dump.read_bytes(), self.public.read_bytes())
        raw = image.read_bytes()
        self.assertNotIn(b'OPENSSH PRIVATE KEY', raw)
        self.assertNotIn(b'local-volte', raw)
        self.assertNotIn(b'.private-firmware', raw)
        self.assertNotIn(b'carrier-voice', raw)

    def test_image_is_reproducible(self):
        one = module.build(self.public, self.sha(self.public), self.root/'one')
        two = module.build(self.public, self.sha(self.public), self.root/'two')
        self.assertEqual(one['image_sha256'], two['image_sha256'])

    def test_private_key_is_refused_before_any_output(self):
        with self.assertRaises(ValueError):
            module.build(self.private, self.sha(self.private), self.root/'out')
        self.assertFalse((self.root/'out').exists())

    def test_digest_mismatch_options_and_second_line_are_refused(self):
        with self.assertRaises(ValueError):
            module.build(self.public, '0'*64, self.root/'out')
        text = self.public.read_text()
        for bad in ('command="id" '+text, text+text):
            path = self.root/'bad.pub'
            path.write_text(bad)
            with self.assertRaises(ValueError):
                module.build(path, self.sha(path), self.root/'out')
        self.assertFalse((self.root/'out').exists())

    def test_existing_output_is_kept(self):
        (self.root/'out').mkdir()
        (self.root/'out/evidence').write_bytes(b'keep')
        with self.assertRaises(ValueError):
            module.build(self.public, self.sha(self.public), self.root/'out')
        self.assertEqual((self.root/'out/evidence').read_bytes(), b'keep')


@unittest.skipUnless(os.geteuid() == 0, 'root-owned marker checks require an isolated root test run')
class WrapperTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name)
        self.system = self.root/'state/rustsimhub-v1/system'
        (self.system/'ssh-access').mkdir(parents=True, mode=0o700)
        self.write(self.system/'generic-provision.request', 'RustSimHub.GenericProvision.v1\n')
        self.write(self.system/'usb.serial', 'ab12'+'0'*28+'\n')
        (self.root/'etc').mkdir()
        self.calls = self.root/'calls'
        stub = self.root/'initialize'
        stub.write_text('#!/bin/sh\nfor a in "$@";do printf "%%s\\n" "$a";done > %s\n' % self.calls)
        stub.chmod(0o755)
        text = WRAPPER.read_text()
        text = text.replace('/var/lib/rustsimhub', str(self.root/'state')).replace('/etc/rustsimhub', str(self.root/'etc'))
        text = text.replace('/usr/local/libexec/rustsimhub-state-check', 'true')
        text = text.replace('/usr/local/libexec/rustsimhub-install-initialize', str(stub))
        self.script = self.root/'wrapper'
        self.script.write_text(text)

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, path, text):
        path.write_text(text)
        path.chmod(0o600)

    def run_it(self):
        return subprocess.run(['sh', str(self.script)], capture_output=True, text=True)

    def test_identity_inputs_come_from_this_device(self):
        result = self.run_it()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), [
            str(self.system/'ssh-access/authorized_keys'), '10.55.0.2:8443', 'rustsimhub-ab12.local,10.55.0.2',
            str(self.system/'first-owner-handoff'), '--media-lan'])
        self.assertEqual((self.root/'etc/device-name').read_text(), 'rustsimhub-ab12\n')
        self.assertEqual((self.root/'etc/device-name').stat().st_mode & 0o777, 0o644)

    def test_complete_install_is_never_repeated(self):
        self.write(self.system/'installation.complete', 'RustSimHub.InitialInstallation.v1\n')
        self.assertEqual(self.run_it().returncode, 0)
        self.assertFalse(self.calls.exists())

    def test_missing_marker_or_bad_serial_or_conflicting_name_is_refused(self):
        (self.root/'etc/device-name').write_text('rustsimhub-zzzz\n')
        self.assertNotEqual(self.run_it().returncode, 0)
        (self.root/'etc/device-name').unlink()
        self.write(self.system/'usb.serial', 'short\n')
        self.assertNotEqual(self.run_it().returncode, 0)
        self.write(self.system/'usb.serial', 'ab12'+'0'*28+'\n')
        (self.system/'generic-provision.request').unlink()
        self.assertNotEqual(self.run_it().returncode, 0)
        self.assertFalse(self.calls.exists())


if __name__ == '__main__':
    unittest.main()
