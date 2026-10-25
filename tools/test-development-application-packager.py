#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual private packager with synthetic artifacts; no payload execution/trust installation."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from application_package import FILES, FORWARDING_FILES, verify_package

ROOT = Path(__file__).resolve().parents[1]


def test():
    assert sys.platform == 'linux', 'OpenSSL/Unix permission verification fixture'
    private = ROOT/'.private'
    private.mkdir(mode=0o700, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rsh-dev-packager-', dir=private) as temporary:
        base = Path(temporary)
        # WSL's Windows 9p mount may report 0555 after chmod(0444). The
        # source package must still be read-only. Actual slot permissions are
        # independently asserted on a POSIX /tmp by the updater CLI fixture.
        probe = base/'mode-probe'
        probe.write_bytes(b'synthetic mode probe')
        probe.chmod(0o444)
        exact_license_mode = probe.stat().st_mode & 0o777 == 0o444
        artifacts = base/'artifacts'
        artifacts.mkdir(mode=0o700)
        elf = bytearray(64)
        elf[:7] = b'\x7fELF\x02\x01\x01'
        elf[16], elf[18], elf[52] = 2, 183, 64
        hashes = {}
        for name in (*FILES[:3], 'rustsimhub-forwarder'):
            (artifacts/name).write_bytes(elf)
            hashes[name] = hashlib.sha256(elf).hexdigest()
        build = {'target': 'aarch64-unknown-linux-gnu', 'development_fixtures': False,
            'database_schema': 6, 'source_archive_sha256': 'a'*64,
            'forwarding_application_format_supported': True, 'artifact_sha256': hashes}
        (artifacts/'build.json').write_text(json.dumps(build))
        native = base/'native'
        native.write_bytes(elf)
        log = base/'native.log'
        log.write_text(json.dumps({'source_archive_sha256': 'a'*64})+'\n'+
            hashlib.sha256(elf).hexdigest()+'  /out/rustsimhub-audio-alsa\nFinished `release`\n')
        def run(output, forwarding, success=True):
            command = [sys.executable, str(ROOT/'tools/package-development-application.py'),
                '--artifacts', str(artifacts), '--native', str(native), '--native-log', str(log), '--output', str(output)]
            if forwarding:
                command.append('--with-forwarder')
            result = subprocess.run(command, capture_output=True, timeout=15)
            assert (result.returncode == 0) == success, 'development packager result differs'
        for forwarding in (False, True):
            output = base/('forwarding' if forwarding else 'legacy')
            run(output, forwarding)
            for epoch in (1, 2, 3):
                package = output/f'package{epoch}'
                manifest = verify_package(package, output/'release.public', 'a'*64, schema=6)
                assert manifest['epoch'] == epoch and manifest['format'] == (2 if forwarding else 1)
                assert set(p.name for p in package.iterdir()) == set(FORWARDING_FILES if forwarding else FILES)|{'manifest.json','manifest.sig'}
                for name in (FORWARDING_FILES if forwarding else FILES):
                    mode = (package/name).stat().st_mode & 0o777
                    assert mode & 0o222 == 0
                    if name != 'Opus-COPYING.txt':
                        assert mode == 0o555
                    elif exact_license_mode:
                        assert mode == 0o444
            provenance = json.loads((output/'provenance.json').read_text())
            assert provenance['development_signer'] and not provenance['production_release']
        (artifacts/'rustsimhub-forwarder').write_bytes(b'changed worker')
        failed = base/'rejected'
        run(failed, True, success=False)
        assert not failed.exists(), 'bad source hash created signing material'
    print(json.dumps({'passed': True, 'actual_packager': True, 'fixed_formats': [1,2],
        'six_epochs_verified': True, 'changed_worker_rejected_before_key_creation': True,
        'source_license_exact_unix_mode_supported': exact_license_mode,
        'synthetic_build_records': True, 'payloads_executed': False,
        'production_keys_used': False, 'device_access': False}))


if __name__ == '__main__':
    test()
