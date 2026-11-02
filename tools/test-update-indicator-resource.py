#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Exercise optional update indication under real systemd TasksMax=1.

Only a synthetic hint directory is writable. The fixed /run path is rebound in
a private mount namespace, with no devices, network or Linux capabilities.
No installed updater, ledger, business data or physical LED is accessed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() == 0, 'run through sudo on the test host'
    assert Path('/proc/1/comm').read_text().strip() == 'systemd'
    fixture = args.fixture.resolve(strict=True)
    metadata = fixture.stat()
    assert (stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1
            and not metadata.st_mode & 0o022 and 0 < metadata.st_size <= 64 * 1024 * 1024)
    assert fixture.read_bytes()[:4] == b'\x7fELF'
    with tempfile.TemporaryDirectory(prefix='rsh-indicator-resource-', dir='/var/tmp') as directory:
        directory = Path(directory)
        binary = directory / 'fixture'
        shutil.copyfile(fixture, binary)
        binary.chmod(0o555)
        info = subprocess.run([str(binary), '--build-info'], capture_output=True,
                              check=True, timeout=5)
        assert json.loads(info.stdout) == {
            'development_fixtures': True, 'role': 'indicator-resource-fixture'}
        hints = directory / 'hints'
        hints.mkdir(mode=0o700)
        unit = 'rsh-indicator-resource-' + uuid.uuid4().hex
        properties = [
            'User=root', 'Group=root', 'TasksMax=1', 'MemoryMax=96M',
            'RuntimeMaxSec=10', 'TimeoutStopSec=2', 'KillMode=control-group',
            'NoNewPrivileges=yes', 'CapabilityBoundingSet=', 'AmbientCapabilities=',
            'PrivateNetwork=yes', 'PrivateDevices=yes', 'ProtectSystem=strict',
            'ProtectHome=yes', 'TemporaryFileSystem=/run', 'LimitCORE=0',
            'RestrictSUIDSGID=yes', 'UMask=0077',
            f'BindPaths={hints}:/run/rustsimhub-led-update',
            'ReadWritePaths=/run/rustsimhub-led-update',
        ]
        command = ['systemd-run', '--quiet', '--wait', '--pipe', '--collect',
                   '--unit=' + unit]
        for prop in properties:
            command += ['--property=' + prop]
        result = subprocess.run(command + [str(binary), '--check'],
                                capture_output=True, timeout=20)
        # Do not accept a sandbox/setup failure as an exercised resource failure.
        assert result.returncode == 0, (result.returncode,
                                        result.stdout.decode(), result.stderr.decode())
        receipt = json.loads(result.stdout)
        assert receipt == {'actual_thread_exhaustion': True,
                           'operation_continued': True, 'upgrading': False}
        state = json.loads((hints / 'state.json').read_text())
        assert state['upgrading'] is False and state['available'] is False
        assert [p.name for p in hints.iterdir()] == ['state.json']
        print(json.dumps({'passed': True, 'tasks_max': 1, **receipt,
                          'fixture_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                          'device_access': False, 'production_data_access': False}))


if __name__ == '__main__':
    main()
