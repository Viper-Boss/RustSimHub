#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Private updater 2MiB tmpfs fixture; reuses the existing forwarding isolation guards.

Root only prepares fresh mount/net namespaces. The synthetic test ELF runs as
UID65534 with no privileges; no ordinary filesystem may be filled.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('existing_enospc', ROOT/'tools/test-forwarding-storage-full.py')
isolation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(isolation)


def binary(path):
    with path.open('rb') as manifest:
        raw = manifest.read(2*1024*1024+1)
    if len(raw) > 2*1024*1024:
        raise ValueError('artifact manifest exceeds bound')
    found = set()
    for line in raw.decode().splitlines():
        row = json.loads(line)
        if (row.get('reason') == 'compiler-artifact'
                and row.get('target', {}).get('name') == 'crash_matrix'
                and row['target']['kind'] == ['test']
                and row.get('profile', {}).get('test') and row.get('executable')):
            found.add(row['executable'])
    if len(found) != 1:
        raise ValueError('expected exactly one crash_matrix test ELF')
    return isolation.checked_binary(Path(found.pop()))


def child(source):
    for kind in ('mnt', 'net'):
        if os.readlink('/proc/self/ns/'+kind) == os.readlink('/proc/1/ns/'+kind):
            raise RuntimeError('fresh private namespaces required')
    with tempfile.TemporaryDirectory(prefix='rustsimhub-updater-full-', dir='/var/tmp') as value:
        base = Path(value)
        if base.resolve().parent != Path('/var/tmp') or base.is_symlink():
            raise RuntimeError('fixture boundary')
        base.chmod(0o755)
        elf = base/'updater-tests'
        shutil.copyfile(source, elf)
        elf.chmod(0o555)
        digest = hashlib.sha256(elf.read_bytes()).hexdigest()
        full = base/'full'
        full.mkdir(mode=0o700)
        subprocess.run(['/bin/mount', '-t', 'tmpfs', '-o',
                        'size=2M,mode=0700,uid=65534,gid=65534,nosuid,nodev,noexec',
                        'rustsimhub-updater-fixture', str(full)], check=True, timeout=10)
        try:
            stat = os.statvfs(full)
            if not os.path.ismount(full) or stat.f_blocks*stat.f_frsize != 2*1024*1024:
                raise RuntimeError('refuse unverified capacity')
            proc = subprocess.run([str(elf), '--ignored', '--exact', 'c14_tmpfs_enospc', '--nocapture'],
                                  cwd=base, env={'PATH':'/usr/bin:/bin', 'LANG':'C.UTF-8', 'C14_TMPFS':str(full)},
                                  preexec_fn=isolation.drop, capture_output=True, text=True, timeout=30)
            print(proc.stdout, end='')
            print(proc.stderr, end='', file=sys.stderr)
            if (proc.returncode != 0 or proc.stdout.count('C14_TMPFS_PASSED format=') != 2
                    or '1 passed; 0 failed; 0 ignored;' not in proc.stdout):
                raise RuntimeError('actual two-format ENOSPC test did not pass')
        finally:
            subprocess.run(['/bin/umount', str(full)], check=True, timeout=10)
        print(json.dumps({'passed':True, 'capacity':2*1024*1024, 'uid':65534,
                          'test_elf_sha256':digest, 'formats':[1,2], 'device':False,
                          'host_storage_filled':False, 'external_network':False,
                          'tmpfs_unmounted':True}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--cargo-artifacts', type=Path)
    source.add_argument('--test-binary', type=Path)
    parser.add_argument('--child', action='store_true', help=argparse.SUPPRESS)
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error('Root only for private namespace/tmpfs preparation')
    elf = binary(args.cargo_artifacts) if args.cargo_artifacts else isolation.checked_binary(args.test_binary)
    if args.child:
        child(elf)
    else:
        proc = subprocess.Popen(['unshare', '--mount', '--net', '--propagation', 'private', '--',
                                 sys.executable, str(Path(__file__).resolve()), '--child', '--test-binary', str(elf)],
                                start_new_session=True)
        try:
            code = proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            raise
        if code:
            raise subprocess.CalledProcessError(code, proc.args)


if __name__ == '__main__':
    main()
