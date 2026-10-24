#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real preparation CLIs in disposable directories; no --serve, services, radio or TLS trust changes."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile
import unittest


class Preparation(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='rustsimhub-setup-material-')
        self.base = Path(self.tmp.name)
        self.base.chmod(0o755)
        self.edge_parent = self.base / 'edge'
        self.root_parent = self.base / 'root'
        self.binding = self.base / 'binding'
        for folder in (self.edge_parent, self.root_parent, self.binding):
            folder.mkdir(mode=0o700)
        os.chown(self.edge_parent, EDGE_USER.pw_uid, EDGE_USER.pw_gid)
        record = {'format': 1, 'realm': '00000000-0000-4000-8000-000000000002',
                  'core_uid': 986, 'device_public_key': [7] * 32}
        path = self.binding / 'binding.json'
        path.write_text(json.dumps(record), encoding='utf-8')
        path.chmod(0o600)

    def tearDown(self):
        self.tmp.cleanup()

    def command(self, argv, success=True):
        result = subprocess.run(argv, capture_output=True, text=True, timeout=15, encoding='utf-8')
        self.assertTrue((result.returncode == 0) == success, 'bounded CLI result differs')
        if not success:
            self.assertTrue(not result.stdout, 'failure must not disclose material')
            return None
        value = json.loads(result.stdout)
        self.assertTrue(value.get('ap_started') is not True and value.get('listener_started') is not True)
        return value

    def edge_prepare(self):
        return self.command(['runuser', '-u', EDGE_USER.pw_name, '--', EDGE,
                             '--prepare-setup-local', str(self.edge_parent)])

    def root_prepare(self, success=True):
        return self.command([AP, '--prepare-material', str(self.root_parent), str(self.binding),
                             str(self.edge_parent / 'installation'), str(EDGE_USER.pw_uid)], success)

    def test_real_edge_and_root_commands_share_exact_cert_and_keep_secret_private(self):
        edge = self.edge_prepare()
        first = self.root_prepare()
        config = self.root_parent / 'installation/config.json'
        original = config.read_bytes()
        secret = json.loads(original)['passphrase']
        second = self.root_prepare()
        self.assertTrue(first == second and config.read_bytes() == original, 'repeat must preserve identity')
        self.assertTrue(secret not in json.dumps(first) and secret not in json.dumps(edge), 'summary disclosed secret')
        export = self.root_parent / 'trusted-copy'
        result = self.command([AP, '--export-material-trusted', str(config.parent), str(export)])
        self.assertTrue(secret not in json.dumps(result), 'export summary disclosed secret')
        connection = json.loads((export / 'connection.json').read_bytes())
        certificate = (export / 'cert.der').read_bytes()
        self.assertTrue(connection['passphrase'] == secret, 'trusted copy differs')
        self.assertTrue(connection['device_public_key'] == '07' * 32)
        self.assertTrue(connection['certificate_sha256'] == hashlib.sha256(certificate).hexdigest())
        self.assertTrue(certificate == (self.edge_parent / 'installation/tls/cert.der').read_bytes())
        for file in (config, export / 'connection.json', export / 'cert.der'):
            stat = file.stat()
            self.assertTrue(stat.st_uid == 0 and stat.st_gid == 0 and stat.st_mode & 0o7777 == 0o600)
        self.command([AP, '--export-material-trusted', str(config.parent), str(export)], False)

    def test_wrong_owner_or_changed_material_does_not_replace_existing_files(self):
        self.edge_prepare()
        self.root_prepare()
        config = self.root_parent / 'installation/config.json'
        original = config.read_bytes()
        path = self.binding / 'binding.json'
        path.chmod(0o644)
        self.root_prepare(False)
        self.assertTrue(config.read_bytes() == original)
        path.chmod(0o600)
        path.write_bytes(path.read_bytes() + b' ')
        self.root_prepare(False)
        self.assertTrue(config.read_bytes() == original)

    def test_actual_core_binding_and_first_setup_code_are_private_and_cannot_reopen(self):
        self.edge_prepare()
        wrong_user=[self.base/name for name in ('root-state','root-identity','root-setup')]
        for folder in wrong_user:
            folder.mkdir(mode=0o700)
        self.command([CORE,'--prepare-device-ap',*(str(p) for p in wrong_user)],False)
        self.assertTrue(all(not list(p.iterdir()) for p in wrong_user))
        folders=[self.base / name for name in ('core-state','core-identity','core-setup')]
        for folder in folders:
            folder.mkdir(mode=0o700); os.chown(folder,986,986)
        core=["setpriv","--reuid=986","--regid=986","--clear-groups",CORE,"--prepare-device-ap",*(str(p) for p in folders)]
        result=self.command(core)
        setup=folders[2]
        binding=json.loads((setup/'binding.json').read_bytes())
        code=(setup/'setup.code').read_bytes().hex()
        actual_key=bytes(binding['device_public_key']).hex()
        self.assertTrue(result['device_public_key_hex']==actual_key and binding['core_uid']==986)
        self.assertTrue(code not in json.dumps(result) and not (setup/'setup.used').exists())
        self.command(core)
        self.assertTrue((setup/'setup.code').read_bytes().hex()==code)
        self.command([AP,'--prepare-from-core',str(self.root_parent),str(setup),str(self.edge_parent/'installation'),str(EDGE_USER.pw_uid),'986'])
        output=self.root_parent/'first-setup-copy'
        exported=self.command([AP,'--export-first-setup',str(self.root_parent/'installation'),str(setup),'986',str(output)])
        self.assertTrue(code not in json.dumps(exported))
        connection=json.loads((output/'connection.json').read_bytes())
        self.assertTrue(connection['setup_code']==code and connection['device_public_key']==actual_key)
        original_binding=(setup/'binding.json').read_bytes()
        wrong=dict(binding,realm='00000000-0000-4000-8000-000000000099')
        (setup/'binding.json').write_bytes(json.dumps(wrong).encode())
        self.command([AP,'--export-first-setup',str(self.root_parent/'installation'),str(setup),'986',str(self.root_parent/'wrong-realm-copy')],False)
        (setup/'binding.json').write_bytes(original_binding)
        self.command([AP,'--export-first-setup',str(self.root_parent/'installation'),str(setup),'987',str(self.root_parent/'wrong-uid-copy')],False)
        locked=os.open(setup,os.O_RDONLY|os.O_DIRECTORY)
        try:
            fcntl.flock(locked,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.command([AP,'--export-first-setup',str(self.root_parent/'installation'),str(setup),'986',str(self.root_parent/'busy-copy')],False)
        finally:
            os.close(locked)
        marker=setup/'setup.used';marker.write_bytes(b'used-fixture')
        self.command(core,False)
        self.command([AP,'--export-first-setup',str(self.root_parent/'installation'),str(setup),'986',str(self.root_parent/'used-copy')],False)
        self.assertTrue(not (self.root_parent/'used-copy').exists())


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--edge', required=True)
    parser.add_argument('--core', required=True)
    parser.add_argument('--ap', required=True)
    parser.add_argument('--edge-user', required=True)
    args, remaining = parser.parse_known_args()
    if os.geteuid() != 0:
        parser.error('requires an isolated root test invocation')
    EDGE = str(Path(args.edge).resolve(strict=True))
    CORE = str(Path(args.core).resolve(strict=True))
    AP = str(Path(args.ap).resolve(strict=True))
    EDGE_USER = pwd.getpwnam(args.edge_user)
    if EDGE_USER.pw_uid == 0 or EDGE_USER.pw_uid != EDGE_USER.pw_gid:
        parser.error('requires a non-root fixture user with matching UID/GID')
    unittest.main(argv=[sys.argv[0], *remaining])
