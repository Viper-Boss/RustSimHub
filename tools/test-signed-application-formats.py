#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual updater signing/verification/slot CLI; synthetic ELF bytes are never executed.

Run as Root inside a fresh network namespace. Only private temporary files are
changed; no device, service, production trust key or business database is used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from application_package import FORWARDING_FILES, verify_package


def test(binary):
    assert os.geteuid() == 0
    assert socket.if_nameindex() == [(1, 'lo')], 'fresh isolated network required'
    info = json.loads(subprocess.check_output([str(binary), '--build-info'], timeout=5))
    assert info['signed_application_formats'] == [1, 2]
    assert info['forwarding_application_format_supported'] is True
    previous = os.umask(0o077)
    try:
        with tempfile.TemporaryDirectory(prefix='rsh-signed-formats-') as temporary:
            base = Path(temporary)
            def call(*args, success=True):
                result = subprocess.run([str(binary), *map(str, args)], capture_output=True, timeout=10)
                assert (result.returncode == 0) == success, 'updater command result differs'
                return result.stdout
            keys, source, store, runtime = (base/name for name in ('keys', 'source', 'store', 'runtime'))
            for directory in (keys, source, store, runtime):
                directory.mkdir(mode=0o700)
            call('keygen', keys)
            public = keys/'release.public'
            for name in FORWARDING_FILES:
                data = bytearray(64)
                data[:7] = b'\x7fELF\x02\x01\x01'
                data[16], data[18], data[52] = 2, 183, 64
                if name == 'Opus-COPYING.txt':
                    data = b'synthetic license'
                (source/name).write_bytes(data)
            packages = []
            for command, epoch in [('make', 1), ('make-forwarding', 2), ('make-forwarding', 3)]:
                package = base/f'package{epoch}'
                package.mkdir(mode=0o700)
                call(command, keys/'release.seed', 'a'*64, epoch, source, package)
                verified = verify_package(package, public, 'a'*64, schema=6)
                assert verified['format'] == (1 if epoch == 1 else 2)
                assert json.loads(call('verify-package', public, package))['verified']
                packages.append(package)
            call('install-factory', store, public, packages[0])
            state = json.loads(call('stage', store, public, packages[1]))
            candidate_hash = state['manifest_sha256']
            selected = Path(call('boot-select', store, public, packages[0], runtime, store/'versions').decode().strip())
            assert selected.name == candidate_hash
            assert (selected/'rustsimhub-forwarder').stat().st_mode & 0o777 == 0o555
            assert (selected/'Opus-COPYING.txt').stat().st_mode & 0o777 == 0o444
            assert (runtime/'selected').resolve() == selected
            fallback = Path(call('boot-select', store, public, packages[0], runtime, store/'versions').decode().strip())
            assert fallback != selected and not (fallback/'rustsimhub-forwarder').exists()
            assert json.loads(call('status', store, public))['floor_epoch'] == 2
            # Corrupt a signed provider payload; rejection precedes slot/floor changes.
            before = (store/'state.json').read_bytes()
            versions = set((store/'versions').iterdir())
            (packages[2]/'rustsimhub-forwarder').write_bytes(b'corrupt worker')
            call('verify-package', public, packages[2], success=False)
            call('stage', store, public, packages[2], success=False)
            assert before == (store/'state.json').read_bytes()
            assert versions == set((store/'versions').iterdir())
            # A worker must not be appended unsigned to the legacy selected group.
            (fallback/'rustsimhub-forwarder').write_bytes((source/'rustsimhub-forwarder').read_bytes())
            call('choose', store, public, success=False)
            assert hashlib.sha256((packages[0]/'manifest.json').read_bytes()).hexdigest() == fallback.name
    finally:
        os.umask(previous)
    print(json.dumps({'passed': True, 'actual_updater_cli': True,
        'rust_python_signature_agreement': True, 'fixed_formats': [1, 2],
        'six_file_selection_and_five_file_fallback': True,
        'corrupt_worker_preserves_floor_and_slots': True, 'unsigned_extra_worker_denied': True,
        'synthetic_payloads_executed': False, 'outbox_rollback_verified': False,
        'device_access': False, 'production_keys_used': False}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--updater', type=Path, required=True)
    args = parser.parse_args()
    test(args.updater.resolve(strict=True))
