#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real preparation CLIs in disposable directories; no --serve, services, radio or TLS trust changes."""
import argparse
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


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--edge', required=True)
    parser.add_argument('--ap', required=True)
    parser.add_argument('--edge-user', required=True)
    args, remaining = parser.parse_known_args()
    if os.geteuid() != 0:
        parser.error('requires an isolated root test invocation')
    EDGE = str(Path(args.edge).resolve(strict=True))
    AP = str(Path(args.ap).resolve(strict=True))
    EDGE_USER = pwd.getpwnam(args.edge_user)
    if EDGE_USER.pw_uid == 0 or EDGE_USER.pw_uid != EDGE_USER.pw_gid:
        parser.error('requires a non-root fixture user with matching UID/GID')
    unittest.main(argv=[sys.argv[0], *remaining])
