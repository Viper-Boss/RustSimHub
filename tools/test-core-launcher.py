#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the finite launcher as real UID 986 in a disposable chroot.

No host systemd, device files, network configuration or production paths are
exposed. The Core stand-in emits fixed build metadata and prints the argv/PID
received after exec; this tests the actual launcher, not actual modem startup.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
FACTORY = '9bbc71d42fb5b10d0a2346579dccc56f6d732828689c2879fed48b1b796a5d39'
FORWARDING = 'd641489c23d019d2c7011f8709c51d2890f0fd3aab4d75ecbb42e0d1158f9cae'
APP49 = 'f0226d2b32ec69a7ab1a062516a75dfbe7229b403d8f72210c7c8e2a099896b6'
CONTRACT = json.loads((ROOT / 'firmware/system/setup-ap/contract.json').read_text())
LAUNCHER = None


def copy_executable(source, root, target):
    source = Path(source).resolve(strict=True)
    destination = root / target.lstrip('/')
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755)
    deps = subprocess.run(['ldd', str(source)], capture_output=True, text=True, check=True).stdout
    for line in deps.splitlines():
        found = re.search(r'(?:=>\s+)?(/[^\s]+)\s+\(', line)
        if found:
            dep = Path(found.group(1))
            dest = root / str(dep).lstrip('/')
            dest.parent.mkdir(parents=True, exist_ok=True)
            if not dest.exists():
                shutil.copyfile(dep.resolve(strict=True), dest)
                dest.chmod(0o755)


class Launcher(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rustsimhub-core-launcher-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.root.chmod(0o755)
        # Inert regular-file stdio stand-in, never a bind mount of host /dev.
        (self.root / 'dev').mkdir()
        (self.root / 'dev/null').touch(mode=0o666)
        (self.root / 'dev/null').chmod(0o666)
        for source, target in [(LAUNCHER, '/launcher'), ('/bin/sh', '/bin/sh'),
                               ('/bin/cat', '/bin/cat'), ('/bin/sleep', '/bin/sleep')]:
            copy_executable(source, self.root, target)
        self.core = self.root / 'opt/rustsimhub/selected/rustsimhub-core'
        self.core.parent.mkdir(parents=True)
        self.core.write_text('''#!/bin/sh
if [ "$1" = --build-info ]; then exec /bin/cat /metadata.json; fi
printf 'pid=%s\\n' "$$"
printf 'arg=%s\\n' "$@"
''')
        self.core.chmod(0o755)
        self.metadata = {'source_archive_sha256': FACTORY, 'database_schema': 6,
                         'development_fixtures': False}
        self.contract = self.root / 'etc/rustsimhub/setup-ap-contract.json'
        self.contract.parent.mkdir(parents=True)
        self.contract.write_text(json.dumps(CONTRACT))
        self.contract.chmod(0o644)
        self.info = json.loads(subprocess.check_output([str(LAUNCHER), '--build-info'], text=True))

    def invoke(self, args=(), uid=986):
        (self.root / 'metadata.json').write_text(json.dumps(self.metadata))
        (self.root / 'metadata.json').chmod(0o644)
        proc = subprocess.Popen(['chroot', '--userspec=' + str(uid) + ':' + str(uid),
                                 '--groups=' + str(uid), str(self.root), '/launcher', *args],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        out, err = proc.communicate(timeout=6)
        self.assertNotIn('source_archive_sha256', err)
        self.assertNotIn('/metadata.json', err)
        return proc, out, err

    def mode(self, command, length):
        proc, out, err = self.invoke()
        self.assertEqual(proc.returncode, 0, err)
        lines = out.splitlines()
        self.assertEqual(lines[0], 'pid=' + str(proc.pid), 'launcher must exec in the same PID')
        args = [line.removeprefix('arg=') for line in lines[1:]]
        self.assertEqual(args[0], command)
        self.assertEqual(len(args), length)
        return args

    def explicit(self):
        self.metadata.update(source_archive_sha256='a'*64, component='core',
                             target_arch=self.info['target_arch'],
                             service_launch={'format': 1, 'modes': ['device', 'forwarding_managed', 'setup_ap']},
                             setup_ap_installation_contract=CONTRACT)

    def refused(self, args=(), uid=986):
        proc, out, err = self.invoke(args, uid)
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(out, '')
        self.assertEqual(err, 'core launcher: service admission refused\n')

    def test_factory16_remains_basic_with_ap_material_present(self):
        args = self.mode('--serve-device', 12)
        unit = (ROOT / 'firmware/system/overlay/etc/systemd/system/rustsimhub-core.service').read_text()
        actual = next(line for line in unit.splitlines() if line.startswith('ExecStart=')).split()[1:]
        self.assertEqual(args, actual)

    def test_previous48_remains_managed_forwarding(self):
        self.metadata['source_archive_sha256'] = FORWARDING
        self.assertEqual(self.mode('--serve-device-forwarding-managed', 13)[-1], '983')

    def test_current49_requires_matching_material(self):
        self.metadata['source_archive_sha256'] = APP49
        self.mode('--serve-device-forwarding-managed', 13)
        self.metadata['setup_ap_installation_contract'] = CONTRACT
        self.mode('--serve-device-forwarding-managed-setup-ap', 16)

    def test_explicit_new_capability_and_missing_optional_material(self):
        self.explicit()
        self.mode('--serve-device-forwarding-managed-setup-ap', 16)
        self.contract.unlink()
        self.mode('--serve-device-forwarding-managed', 13)

    def test_untrusted_contract_permissions_owner_links_and_fifo_only_disable_ap(self):
        self.explicit()
        self.contract.chmod(0o666)
        self.mode('--serve-device-forwarding-managed', 13)
        self.contract.chmod(0o644)
        os.chown(self.contract, 986, 986)
        self.mode('--serve-device-forwarding-managed', 13)
        os.chown(self.contract, 0, 0)
        os.link(self.contract, self.contract.with_suffix('.other'))
        self.mode('--serve-device-forwarding-managed', 13)
        self.contract.unlink()
        os.mkfifo(self.contract, 0o644)
        self.mode('--serve-device-forwarding-managed', 13)

    def test_invalid_or_duplicate_contract_and_symlink_only_disable_ap(self):
        self.explicit()
        self.contract.write_text('{"format":2,' + json.dumps(CONTRACT)[1:])
        self.mode('--serve-device-forwarding-managed', 13)
        self.contract.write_bytes(b' '*4097)
        self.mode('--serve-device-forwarding-managed', 13)
        self.contract.unlink()
        self.contract.symlink_to('/metadata.json')
        self.mode('--serve-device-forwarding-managed', 13)

    def test_unknown_old_source_and_wrong_uid_or_cli_are_refused(self):
        self.metadata['source_archive_sha256'] = 'a'*64
        self.refused()
        self.metadata['source_archive_sha256'] = FACTORY
        self.refused(uid=0)
        self.refused(uid=985)
        self.refused(args=('--program', '/bin/sh'))

    def test_untrusted_core_owner_write_bit_and_hardlink_are_refused(self):
        os.chown(self.core,986,986)
        self.refused()
        os.chown(self.core,0,0)
        self.core.chmod(0o775)
        self.refused()
        self.core.chmod(0o755)
        os.link(self.core, self.core.with_suffix('.other'))
        self.refused()

    def test_core_symlink_or_fifo_or_no_execute_are_refused(self):
        self.core.chmod(0o644)
        self.refused()
        self.core.unlink()
        self.core.symlink_to('/bin/sh')
        self.refused()
        self.core.unlink()
        os.mkfifo(self.core, 0o755)
        self.refused()

    def test_actual_metadata_deadline_and_output_cap(self):
        self.core.write_text('#!/bin/sh\nexec /bin/sleep 20\n')
        before = time.monotonic()
        self.refused()
        self.assertLess(time.monotonic() - before, 5)
        self.core.write_text('#!/bin/sh\ni=0; while [ "$i" -lt 17000 ]; do printf x; i=$((i+1)); done\n')
        self.refused()

    def test_exact_limit_valid_metadata_then_one_byte_larger(self):
        # Independent producer (cat) and real UID exercise the production path,
        # including JSON parsing and exec, not only the private reader helper.
        self.metadata['padding']=''
        self.metadata['padding']='a'*(16384-len(json.dumps(self.metadata).encode()))
        self.assertEqual(len(json.dumps(self.metadata).encode()),16384)
        self.mode('--serve-device',12)
        self.metadata['padding']+='a'
        self.refused()


def main():
    global LAUNCHER
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher', required=True, type=Path)
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error('disposable chroot fixtures require root')
    LAUNCHER = args.launcher.resolve(strict=True)
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Launcher))
    raise SystemExit(not result.wasSuccessful())


if __name__ == '__main__':
    main()
