#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real Core/Auth/Store confirmation in a disposable, networkless native container.

Native ELF architecture and metadata/PID1 adapters are cfg(test) ONLY. This does
not assert ARM/PID1/device acceptance. No host mounts, NSS, buses or image pulls.
The input must be the updater LIB unit-test executable built with a recorded
RUSTSIMHUB_BUILD_SOURCE_SHA256. Never runs ignored UID tests on the host.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tests', type=Path, required=True)
    parser.add_argument('--updater', type=Path, required=True)
    args = parser.parse_args()
    test = args.tests.resolve(strict=True)
    updater = args.updater.resolve(strict=True)
    environment = os.environ.copy()
    for key in ['DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS',
                'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH']:
        environment.pop(key, None)
    docker = ['docker', '--host=unix:///var/run/docker.sock']
    try:
        check = subprocess.run([*docker, 'info', '--format', '{{.ServerVersion}}'],
                               env=environment, capture_output=True, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        print(json.dumps({'status': 'not_run', 'reason': 'local Docker unavailable'}))
        return 77
    if check.returncode:
        print(json.dumps({'status': 'not_run', 'reason': 'local Docker unavailable'}))
        return 77
    repository = Path(__file__).resolve().parents[1]
    head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repository,
                          capture_output=True, text=True, check=True).stdout.strip()
    dirty = subprocess.run(['git', 'status', '--porcelain'], cwd=repository,
                           capture_output=True, check=True).stdout
    info = json.loads(subprocess.run([str(updater), '--build-info'],
        capture_output=True, text=True, check=True, timeout=5).stdout)
    image = 'rsh-web3-health-native-' + uuid.uuid4().hex
    name = image + '-run'
    private = Path(__file__).resolve().parents[1]/'.local'/'web3-health-native'
    private.mkdir(parents=True, exist_ok=True, mode=0o700)
    with tempfile.TemporaryDirectory(dir=private) as temporary:
        stage = Path(temporary)
        root = stage/'rootfs'
        def copy(source, relative):
            destination = root/relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if destination.exists():
                destination.chmod(0o755)
            shutil.copyfile(source, destination)
            destination.chmod(0o555)
        for source, relative in [(test, 'fixture/update-tests'),
                                 (updater, 'fixture/updater'),
                                 (Path('/usr/bin/setpriv'), 'fixture/setpriv'),
                                 (Path('/usr/bin/sleep'), 'fixture/sleep')]:
            copy(source, relative)
            libs = subprocess.run(['ldd', str(source)], check=True,
                                  capture_output=True, text=True, timeout=5).stdout
            for lib in re.findall(r'(?:=>\s+)?(/[^\s]+)\s+\(', libs):
                copy(Path(lib).resolve(strict=True), lib.lstrip('/'))
        (root/'fixture/RSH_WEB3_ISOLATED').write_text('private-native-health-only\n')
        for directory in [root, *root.rglob('*')]:
            if directory.is_dir():
                directory.chmod(0o755)
        (stage/'Dockerfile').write_text('FROM scratch\nCOPY rootfs/ /\n')
        build = subprocess.run([*docker, 'build', '--network=none', '-t', image, str(stage)],
                               env=environment, capture_output=True, timeout=60)
        if build.returncode:
            os.sys.stderr.buffer.write(build.stderr[:8192])
            return 1
        try:
            command = [*docker, 'run', '--rm', '--name', name, '--pull=never',
                       '--network=none', '--read-only', '--cap-drop=ALL']
            for cap in ['CHOWN', 'SETUID', 'SETGID', 'SETPCAP', 'KILL',
                        'SYS_PTRACE', 'DAC_OVERRIDE']:
                command += ['--cap-add='+cap]
            command += ['--security-opt=no-new-privileges', '--memory=512m', '--pids-limit=64',
                        '-e', 'RSH_WEB3_PRIVATE_CONTAINER=yes']
            for mount, size, mode, execute in [('/run', 8, '0755', False),
                    ('/etc', 8, '0755', False), ('/usr/share/rustsimhub', 8, '0755', False),
                    ('/var/lib/rustsimhub', 64, '0755', False),
                    ('/opt/rustsimhub', 64, '0755', True), ('/test-state', 8, '0755', False),
                    ('/tmp', 8, '1777', False)]:
                command += ['--tmpfs', f'{mount}:rw,nosuid,nodev,{"exec" if execute else "noexec"},size={size}m,mode={mode}']
            command += [image, '/fixture/update-tests', '--exact',
                'runtime_observation::confirmation::tests::live::kernel_real_core_confirm_once_and_failures',
                '--ignored', '--nocapture', '--test-threads=1']
            result = subprocess.run(command, env=environment, capture_output=True, timeout=60)
            if result.returncode:
                os.sys.stderr.buffer.write(result.stdout[:16384] + result.stderr[:8192])
                return 1
            print(json.dumps({'status': 'pass', 'scope': 'native-real-core-auth-store-unix-confirm',
                'tests_sha256': hashlib.sha256(test.read_bytes()).hexdigest(),
                'updater_sha256': hashlib.sha256(updater.read_bytes()).hexdigest(),
                'tested_source_head': head, 'source_worktree_dirty': bool(dirty),
                'source_archive_sha256': info['source_archive_sha256'],
                'arm': 'not_run', 'systemd_pid1': 'not_run',
                'synthetic': ['unit/cgroup membership', 'signed build-info capture output',
                              'external business readiness', 'native-only ELF test adapter']}))
        except subprocess.TimeoutExpired:
            print('private health fixture timed out', file=os.sys.stderr)
            return 1
        finally:
            subprocess.run([*docker, 'rm', '--force', name], env=environment,
                           capture_output=True, timeout=10)
            subprocess.run([*docker, 'image', 'rm', image], env=environment,
                           capture_output=True, timeout=10)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
