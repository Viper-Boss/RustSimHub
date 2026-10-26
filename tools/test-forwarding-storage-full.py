#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real ENOSPC against a 2MiB tmpfs in private mount/network namespaces.

Root only prepares the disposable mount and drops the test to UID 65534 with
NoNewPrivileges. No account, service, device, external network or host storage
budget is modified. The Rust test independently checks tmpfs, capacity and UID
before it writes any filler; a normal filesystem is refused.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import stat
import subprocess
import sys
import tempfile

TEST = 'queue::tests::real_filesystem_enospc_preserves_unknown_and_cursor_then_recovers_once'
MARKER = 'BOUNDED_TMPFS_ENOSPC_PASSED'
CAPACITY = 2 * 1024 * 1024


def binary_from_manifest(path):
    with Path(path).open('rb') as manifest:
        raw = manifest.read(2 * 1024 * 1024 + 1)
    if len(raw) > 2 * 1024 * 1024:
        raise ValueError('cargo artifact manifest exceeds bound')
    candidates = set()
    for line in raw.decode('utf-8').splitlines():
        row = json.loads(line)
        if (row.get('reason') == 'compiler-artifact'
                and row.get('target', {}).get('name') == 'rustsimhub_forwarder'
                and row['target']['kind'] == ['lib']
                and row.get('profile', {}).get('test') is True
                and row.get('executable')):
            candidates.add(row['executable'])
    if len(candidates) != 1:
        raise ValueError('expected exactly one forwarder library test executable')
    return Path(candidates.pop())


def checked_binary(path):
    path = path.resolve(strict=True)
    info = path.stat()
    if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
            or info.st_mode & 0o022 or not info.st_mode & 0o111
            or not 0 < info.st_size <= 256 * 1024 * 1024):
        raise ValueError('expected bounded, non-writable executable ELF')
    with path.open('rb') as file:
        if file.read(4) != b'\x7fELF':
            raise ValueError('test input must be ELF, never a shell script')
    return path


def drop():
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(38, 1, 0, 0, 0) != 0:  # PR_SET_NO_NEW_PRIVS
        raise OSError(ctypes.get_errno(), 'NoNewPrivileges')
    os.setgroups([])
    os.setgid(65534)
    os.setuid(65534)
    os.umask(0o077)
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_FSIZE, (4 * 1024 * 1024,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (10, 12))


def child(binary):
    for name in ['mnt', 'net']:
        if os.readlink('/proc/self/ns/' + name) == os.readlink('/proc/1/ns/' + name):
            raise RuntimeError('fresh mount/network namespaces required')
    with tempfile.TemporaryDirectory(prefix='rustsimhub-forwarding-full-', dir='/var/tmp') as value:
        base = Path(value)
        if base.resolve().parent != Path('/var/tmp') or base.is_symlink():
            raise RuntimeError('fixture directory boundary')
        base.chmod(0o755)
        executable = base / 'forwarder-tests'
        shutil.copyfile(binary, executable)
        executable.chmod(0o555)
        with executable.open('rb') as snapshot:
            digest = hashlib.file_digest(snapshot, 'sha256').hexdigest()
        full = base / 'full'
        full.mkdir(mode=0o700)
        subprocess.run(['/bin/mount', '-t', 'tmpfs', '-o',
                        'size=2M,mode=0700,uid=65534,gid=65534,nosuid,nodev,noexec',
                        'rustsimhub-enospc-fixture', str(full)], check=True, timeout=10)
        try:
            info = os.statvfs(full)
            if info.f_blocks * info.f_frsize != CAPACITY or not os.path.ismount(full):
                raise RuntimeError('refuse an unverified filesystem capacity')
            result = subprocess.run(
                [str(executable), '--ignored', '--exact', TEST, '--nocapture'],
                cwd=base, env={'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8',
                               'RUSTSIMHUB_TEST_FULL_FS': str(full)},
                preexec_fn=drop, capture_output=True, text=True, timeout=30)
            print(result.stdout, end='')
            print(result.stderr, file=sys.stderr, end='')
            if (result.returncode != 0 or MARKER not in result.stdout
                    or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in result.stdout):
                raise RuntimeError('actual ENOSPC test did not pass')
            print(json.dumps({'passed': True, 'fixture': 'private-tmpfs',
                              'capacity_bytes': CAPACITY, 'uid': 65534, 'test_binary_sha256': digest,
                              'no_new_privileges': True, 'host_storage_filled': False,
                              'device_access': False, 'external_network': False}))
        finally:
            subprocess.run(['/bin/umount', str(full)], check=True, timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--cargo-artifacts', type=Path)
    source.add_argument('--test-binary', type=Path)
    parser.add_argument('--child', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error('Root required solely for the private namespace/tmpfs setup')
    binary = checked_binary(args.test_binary or binary_from_manifest(args.cargo_artifacts))
    if args.child:
        child(binary)
    else:
        # Fresh namespaces are mandatory, not a fallback option. No mount is
        # performed by the parent, and no host network link is configured.
        subprocess.run(['unshare', '--mount', '--net', '--propagation', 'private',
                        '--', sys.executable, str(Path(__file__).resolve()),
                        '--child', '--test-binary', str(binary)], check=True, timeout=45)


if __name__ == '__main__':
    main()
