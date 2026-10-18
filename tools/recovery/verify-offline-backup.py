#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Check a frozen UFI003 backup file without opening USB or writing a partition."""
import argparse
import contextlib
import hashlib
import json
import os
import pathlib
import re
import stat
import subprocess

import edl_readonly as recovery


def require(condition, message):
    if not condition:
        raise ValueError(message)


def signature(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns,
            info.st_ctime_ns, info.st_uid, info.st_gid, info.st_mode, info.st_nlink)


def open_image(path):
    require(path.is_absolute() and path.resolve() == path, 'Noncanonical backup file')
    recovery.private_directory(path.parent, 0)
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(descriptor)
        require(stat.S_ISREG(info.st_mode) and info.st_size == recovery.DISK_BYTES,
                'Expected the fixed full user-area backup as a regular file')
        require(info.st_uid == info.st_gid == 0 and stat.S_IMODE(info.st_mode) == 0o600
                and info.st_nlink == 1, 'Backup owner, permissions or link count mismatch')
        require(signature(path.lstat()) == signature(info), 'Backup path changed while opening')
        return descriptor, signature(info)
    except BaseException:
        os.close(descriptor)
        raise


def digest_fd(descriptor):
    result = hashlib.sha256()
    os.lseek(descriptor, 0, os.SEEK_SET)
    while True:
        data = os.read(descriptor, 4 * 1024 * 1024)
        if not data:
            return result.hexdigest()
        result.update(data)


def read_at(descriptor, offset, size):
    data = bytearray()
    while len(data) < size:
        block = os.pread(descriptor, min(size - len(data), 4 * 1024 * 1024), offset + len(data))
        require(bool(block), 'Short backup read')
        data.extend(block)
    return bytes(data)


@contextlib.contextmanager
def readonly_loop(descriptor, part, state):
    # Inherit only this pinned read-only FD. losetup never resolves the input path again.
    loop = subprocess.check_output(
        ['losetup', '--find', '--show', '--read-only', '--offset', str(part['first_sector'] * 512),
         '--sizelimit', str(part['bytes']), '/proc/self/fd/' + str(descriptor)],
        pass_fds=(descriptor,), text=True, timeout=30).strip()
    require(re.fullmatch(r'/dev/loop[0-9]+', loop) is not None, 'Unexpected loop device name')
    state.update(loop=loop, detached=False)
    try:
        rows = json.loads(subprocess.check_output(
            ['losetup', '--list', '--json', '--output', 'NAME,BACK-FILE,OFFSET,SIZELIMIT,RO', loop],
            text=True, timeout=30))['loopdevices']
        require(len(rows) == 1, 'Ambiguous loop mapping')
        row = rows[0]
        require(row['name'] == loop and row['ro'] in (True, 1, '1')
                and int(row['offset']) == part['first_sector'] * 512
                and int(row['sizelimit']) == part['bytes'], 'Loop mapping is not the bounded read-only partition')
        # The backing inode must still be the pinned input, not another regular file.
        backing = pathlib.Path(row['back-file'])
        require(signature(backing.stat()) == signature(os.fstat(descriptor)), 'Loop backing file differs from input')
        yield loop
    finally:
        subprocess.run(['losetup', '--detach', loop], check=True, timeout=30)
        state['detached'] = True


def check_filesystems(descriptor, layout, output, report, save):
    failures = []
    for name in ('rootfs', 'NSHSTATE'):
        part = layout[name]
        state = {'read_only': True, 'offset': part['first_sector'] * 512,
                 'bytes': part['bytes'], 'passed': False}
        report['filesystems'][name] = state
        save()
        try:
            with readonly_loop(descriptor, part, state) as loop:
                with (output / (name + '-e2fsck.log')).open('xb') as log:
                    result = subprocess.run(['e2fsck', '-f', '-n', loop], stdout=log,
                                            stderr=subprocess.STDOUT, timeout=300)
                state['e2fsck_exit'] = result.returncode
                state['passed'] = result.returncode == 0
                if not state['passed']:
                    failures.append(name)
        except Exception as exc:
            state['error'] = str(exc)
            failures.append(name)
        finally:
            save()
    return failures


def verify(image, output, expected_sha256, expected_gpt_sha256):
    require(os.geteuid() == 0, 'Run on the backup host as the local Root operator')
    for value in (expected_sha256, expected_gpt_sha256):
        require(re.fullmatch('[0-9a-f]{64}', value) is not None, 'Trusted prior digests required')
    require(output.is_absolute() and output.resolve() == output and not output.exists(),
            'Output must be a new canonical directory; preserve all previous results')
    recovery.private_directory(output.parent, 0)
    descriptor, before = open_image(image)
    report = {'format': 1, 'passed': False, 'partition_writes': False, 'image_write_requested': False,
              'recovery_verified': False, 'flash_allowed': False, 'boot_areas_captured': False,
              'filesystems': {}, 'bytes': recovery.DISK_BYTES}

    def save():
        temporary = output / 'report.json.next'
        with temporary.open('x', encoding='utf-8') as stream:
            json.dump(report, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(output / 'report.json')

    created = False
    try:
        output.mkdir(mode=0o700)
        created = True
        save()
        report['independent_readback_sha256'] = digest_fd(descriptor)
        require(report['independent_readback_sha256'] == expected_sha256, 'Backup digest mismatch')
        primary = read_at(descriptor, 0, 34 * 512)
        backup = read_at(descriptor, recovery.DISK_BYTES - 33 * 512, 33 * 512)
        require(hashlib.sha256(primary + backup).hexdigest() == expected_gpt_sha256, 'Trusted GPT digest mismatch')
        layout = recovery.validate_gpt(primary, backup)
        boot = layout['boot']
        require(hashlib.sha256(read_at(descriptor, boot['first_sector'] * 512, boot['bytes'])).hexdigest()
                == recovery.BOOT_SHA256, 'Current boot digest mismatch')
        report.update(gpt_copies_checked=True, current_boot_verified=True)
        failures = check_filesystems(descriptor, layout, output, report, save)
        require(digest_fd(descriptor) == expected_sha256, 'Backup changed during filesystem checks')
        require(signature(os.fstat(descriptor)) == before == signature(image.lstat()),
                'Backup metadata or path changed during filesystem checks')
        report['image_unchanged_after_fs_checks'] = True
        require(not failures, 'Frozen filesystems did not pass: ' + ', '.join(failures))
        report['passed'] = True
        return report
    except Exception as exc:
        report['error'] = str(exc)
        raise
    finally:
        os.close(descriptor)
        if created:
            save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--expected-sha256', required=True)
    parser.add_argument('--expected-gpt-sha256', required=True)
    args = parser.parse_args()
    os.umask(0o077)
    verify(args.image, args.output, args.expected_sha256, args.expected_gpt_sha256)
    print('Frozen backup readback, GPT, boot and both read-only filesystem checks passed')


if __name__ == '__main__':
    main()
