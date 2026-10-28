#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Read the signed-application store state inside an already verified frozen backup.

Answers one question before a restore plan is accepted: which confirmed application epoch
does this snapshot actually contain? It never mounts the image, opens USB or writes a byte;
NSHSTATE is mapped through the same pinned read-only loop as the offline verifier and read
with debugfs in its default read-only mode.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import subprocess

import edl_readonly as recovery

spec = importlib.util.spec_from_file_location('offline_backup', pathlib.Path(__file__).with_name('verify-offline-backup.py'))
offline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(offline)

STATE_PATH = '/rustsimhub-v1/system/app/state.json'
STATE_LIMIT = 8192
require = offline.require


def hex_digest(value):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) is not None


def debugfs(loop, request):
    # No -w: debugfs opens the device read-only; the loop itself is read-only as well.
    result = subprocess.run(['debugfs', '-R', request, loop], capture_output=True, timeout=60)
    require(result.returncode == 0, 'debugfs failed')
    return result.stdout


def read_state(loop):
    info = debugfs(loop, 'stat ' + STATE_PATH).decode(errors='replace')
    require(re.search(r'Type: regular\b', info) is not None, 'Application state is not a regular file in the snapshot')
    data = debugfs(loop, 'cat ' + STATE_PATH)
    require(0 < len(data) <= STATE_LIMIT, 'Application state is missing or oversized')
    return data


def check_state(data, expected_key_sha256, expected_epoch):
    """Mirror the store's own validity rules, then require an idle store at the expected epoch."""
    state = json.loads(data.decode('utf-8'))
    require(isinstance(state, dict) and set(state) == {'format', 'key_sha256', 'floor', 'confirmed', 'candidate', 'attempted'},
            'Unexpected application state fields')
    floor, confirmed, candidate = state['floor'], state['confirmed'], state['candidate']
    require(state['format'] == 1 and hex_digest(state['key_sha256']) and type(floor) is int
            and type(state['attempted']) is bool, 'Invalid application state')
    for version in (confirmed, candidate):
        if version is not None:
            require(isinstance(version, dict) and set(version) == {'hash', 'epoch'} and hex_digest(version['hash'])
                    and type(version['epoch']) is int and 0 < version['epoch'] <= floor, 'Invalid application version')
    require(not state['attempted'] or candidate is not None, 'Invalid application state')
    require(state['key_sha256'] == expected_key_sha256, 'Snapshot trusts a different release key')
    require(confirmed is not None and confirmed['epoch'] == expected_epoch, 'Snapshot does not hold the expected confirmed epoch')
    require(floor == expected_epoch, 'Snapshot rollback floor differs from the expected epoch')
    require(candidate is None and not state['attempted'], 'Snapshot was taken with a pending update')
    return {'floor': floor, 'confirmed_epoch': confirmed['epoch'], 'confirmed_hash': confirmed['hash'],
            'candidate': None, 'attempted': False, 'key_sha256': state['key_sha256']}


def inspect(image, offline_report, output, expected_sha256, expected_gpt_sha256, expected_key_sha256, expected_epoch):
    require(os.geteuid() == 0, 'Run on the backup host as the local Root operator')
    for value in (expected_sha256, expected_gpt_sha256, expected_key_sha256):
        require(hex_digest(value), 'Trusted prior digests required')
    require(type(expected_epoch) is int and expected_epoch > 0, 'Expected epoch required')
    require(output.is_absolute() and output.resolve() == output and not output.exists(),
            'Output must be a new canonical directory; preserve all previous results')
    recovery.private_directory(output.parent, 0)
    evidence = json.loads(offline_report.read_text(encoding='utf-8'))
    require(evidence.get('passed') is True and evidence.get('independent_readback_sha256') == expected_sha256
            and all(evidence.get('filesystems', {}).get(n, {}).get('passed') is True for n in ('rootfs', 'NSHSTATE')),
            'Run verify-offline-backup.py on this exact image first')
    descriptor, before = offline.open_image(image)
    report = {'format': 1, 'passed': False, 'partition_writes': False, 'image_write_requested': False,
              'usb_opened': False, 'recovery_verified': False, 'flash_allowed': False,
              'offline_report_sha256': hashlib.sha256(offline_report.read_bytes()).hexdigest(),
              'expected_epoch': expected_epoch}

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
        require(offline.digest_fd(descriptor) == expected_sha256, 'Backup digest mismatch')
        primary = offline.read_at(descriptor, 0, 34 * 512)
        backup = offline.read_at(descriptor, recovery.DISK_BYTES - 33 * 512, 33 * 512)
        require(hashlib.sha256(primary + backup).hexdigest() == expected_gpt_sha256, 'Trusted GPT digest mismatch')
        layout = recovery.validate_gpt(primary, backup, recovery.SECTORS, recovery.PARTITIONS)
        mapping = {}
        with offline.readonly_loop(descriptor, layout['NSHSTATE'], mapping) as loop:
            data = read_state(loop)
        require(mapping.get('detached') is True, 'Read-only mapping was not released')
        report['application'] = check_state(data, expected_key_sha256, expected_epoch)
        require(offline.signature(os.fstat(descriptor)) == before == offline.signature(image.lstat()),
                'Backup metadata or path changed during inspection')
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
    parser.add_argument('--offline-report', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--expected-sha256', required=True)
    parser.add_argument('--expected-gpt-sha256', required=True)
    parser.add_argument('--expected-key-sha256', required=True)
    parser.add_argument('--expected-epoch', required=True, type=int)
    args = parser.parse_args()
    os.umask(0o077)
    report = inspect(args.image, args.offline_report, args.output, args.expected_sha256, args.expected_gpt_sha256,
                     args.expected_key_sha256, args.expected_epoch)
    print('Snapshot holds confirmed application epoch', report['application']['confirmed_epoch'], 'with no pending update')


if __name__ == '__main__':
    main()
