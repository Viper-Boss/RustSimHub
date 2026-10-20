#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Make a private, fixed rootfs+NSHSTATE restore plan. Never opens USB or writes a device."""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import secrets
import stat

import edl_readonly as recovery

spec = importlib.util.spec_from_file_location('offline_backup_verifier', pathlib.Path(__file__).with_name('verify-offline-backup.py'))
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)

TARGETS = ((14, 'rootfs'), (15, 'NSHSTATE'))
MAX_REPORT_BYTES = 128 * 1024
# Audited 9947a0f inspector: the full preimage was verified before its reset
# envelope was refused. Recognize this producer explicitly; never relabel the
# failed transaction as a successful capture or accept arbitrary executors.
PREIMAGE_INSPECTOR_SHA256 = '574a5a190820e6f10f86bc192cc05307fc85273a2d675e3dac8eb248642e9a2f'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def private_json(path):
    require(path.is_absolute() and path.resolve() == path, 'Noncanonical private report')
    recovery.private_directory(path.parent, 0)
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(descriptor)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == info.st_gid == 0
                and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1
                and 0 < info.st_size <= MAX_REPORT_BYTES, 'Invalid private report metadata')
        raw = backup.read_at(descriptor, 0, info.st_size)
        require(backup.signature(os.fstat(descriptor)) == backup.signature(info)
                == backup.signature(path.lstat()), 'Report changed during read')
        value = json.loads(raw)
        require(isinstance(value, dict), 'Private report must be an object')
        return value, hashlib.sha256(raw).hexdigest()
    finally:
        os.close(descriptor)


def geometry(layout):
    targets = []
    for index, name in TARGETS:
        expected_name, expected_bytes = recovery.PARTITIONS[index]
        part = layout[name]
        require(expected_name == name and type(part['index']) is int and part['index'] == index
                and type(part['bytes']) is int and part['bytes'] == expected_bytes
                and type(part['first_sector']) is int, 'Unexpected restore partition')
        start = part['first_sector'] * 512
        require(start >= 34 * 512 and part['bytes'] > 0 and part['bytes'] % 512 == 0
                and start + part['bytes'] <= recovery.DISK_BYTES - 33 * 512, 'Restore range crosses GPT or disk bounds')
        targets.append({'name': name, 'index': index, 'offset': start, 'bytes': part['bytes'],
                        'start_sector': part['first_sector'], 'sector_count': part['bytes'] // 512,
                        'physical_partition_number': 0})
    cursor = 0
    protected = []
    for part in sorted(targets, key=lambda p: p['offset']):
        require(part['offset'] >= cursor, 'Overlapping restore targets')
        if part['offset'] > cursor:
            protected.append({'offset': cursor, 'bytes': part['offset'] - cursor})
        cursor = part['offset'] + part['bytes']
    if cursor < recovery.DISK_BYTES:
        protected.append({'offset': cursor, 'bytes': recovery.DISK_BYTES - cursor})
    boot = layout['boot']
    boot_start = boot['first_sector'] * 512
    require(any(p['offset'] <= boot_start and boot_start + boot['bytes'] <= p['offset'] + p['bytes']
                for p in protected), 'Boot must be wholly protected')
    return targets, protected


def check_evidence(capture, offline, expected_gpt, layout):
    if capture.get('action') == 'inspect':
        require(type(capture.get('format')) is int and capture['format'] == 1
                and capture.get('executor_sha256') == PREIMAGE_INSPECTOR_SHA256
                and capture.get('passed') is False and capture.get('usb_opened') is True
                and capture.get('partition_writes_may_have_occurred') is False
                and capture.get('normal_reset_attempted') is True
                and capture.get('normal_reset_acknowledged') is False
                and capture.get('phase') == 'failed-inspect-current-mode'
                and capture.get('error_type') == 'ValueError'
                and capture.get('error') == 'Explicit ACK missing; no automatic retry or success inference',
                'Only the audited read-only preimage with the recorded reset refusal is accepted')
        before = capture.get('current_before', {})
        require(isinstance(before, dict) and before.get('gpt_sha256') == expected_gpt
                and before.get('boot_sha256') == recovery.BOOT_SHA256
                and isinstance(before.get('sha256'), str)
                and re.fullmatch('[0-9a-f]{64}', before['sha256']) is not None,
                'Verified inspection preimage digests are required')
        targets, protected = geometry(layout)
        for key, ranges in (('targets', targets), ('protected', protected)):
            rows = before.get(key)
            require(isinstance(rows, list) and len(rows) == len(ranges), 'Incomplete inspection preimage ranges')
            for row, expected in zip(rows, ranges):
                require(isinstance(row, dict) and set(row) == set(expected) | {'backup_sha256'}
                        and {k:v for k,v in row.items() if k != 'backup_sha256'} == expected
                        and all(type(row[k]) is type(v) for k,v in expected.items())
                        and isinstance(row.get('backup_sha256'), str)
                        and re.fullmatch('[0-9a-f]{64}', row['backup_sha256']) is not None,
                        'Inspection preimage range mismatch')
        image_sha256 = before['sha256']
    else:
        require(capture.get('format') == 1 and capture.get('action') == 'read-emmc'
            and capture.get('passed') is True and capture.get('partition_writes') is False
            and capture.get('bytes') == recovery.DISK_BYTES
            and capture.get('current_boot_verified') is True and capture.get('gpt_copies_checked') is True,
                'A passed full raw capture is required')
        require(capture.get('source_revision') == recovery.SOURCE_REVISION
            and capture.get('loader_sha256') == recovery.LOADER_SHA256
            and capture.get('wrapper_sha256') == recovery.digest(pathlib.Path(recovery.__file__).resolve()),
                'Capture runtime provenance mismatch')
        require(capture.get('gpt_sha256') == expected_gpt
            and re.fullmatch('[0-9a-f]{64}', capture.get('sha256', '')) is not None,
                'Capture digests do not match trusted inputs')
        image_sha256 = capture['sha256']
    require(offline.get('format') == 1 and offline.get('passed') is True
            and offline.get('partition_writes') is False and offline.get('image_write_requested') is False
            and offline.get('bytes') == recovery.DISK_BYTES
            and offline.get('image_unchanged_after_fs_checks') is True
            and offline.get('gpt_copies_checked') is True and offline.get('current_boot_verified') is True
            and offline.get('independent_readback_sha256') == image_sha256,
            'A passed independent check of this exact raw image is required')
    checks = offline.get('filesystems', {})
    require(isinstance(checks, dict) and set(checks) == {'rootfs', 'NSHSTATE'}, 'Both filesystem results are required')
    for name, result in checks.items():
        require(isinstance(result, dict) and result.get('read_only') is True and result.get('detached') is True
                and result.get('passed') is True and type(result.get('e2fsck_exit')) is int
                and result['e2fsck_exit'] == 0 and result.get('offset') == layout[name]['first_sector'] * 512
                and result.get('bytes') == layout[name]['bytes'], 'Raw filesystem result is not clean at the expected range')
    return image_sha256


def hash_coverage(descriptor, targets, protected):
    full = hashlib.sha256()
    cursor = 0
    for row in sorted(targets + protected, key=lambda p: p['offset']):
        require(row['offset'] == cursor, 'Restore/protected coverage has a gap or overlap')
        part_hash = hashlib.sha256()
        remaining = row['bytes']
        while remaining:
            data = os.pread(descriptor, min(4 * 1024 * 1024, remaining), cursor)
            require(bool(data), 'Short backup coverage read')
            part_hash.update(data)
            full.update(data)
            cursor += len(data)
            remaining -= len(data)
        row['backup_sha256'] = part_hash.hexdigest()
    require(cursor == recovery.DISK_BYTES, 'Restore/protected coverage is incomplete')
    return full.hexdigest()


def prepare(image, capture_path, offline_path, expected_gpt, output):
    require(os.geteuid() == 0, 'Run on the backup host as the local Root operator')
    require(re.fullmatch('[0-9a-f]{64}', expected_gpt) is not None, 'Trusted prior GPT digest required')
    require(output.is_absolute() and output.resolve() == output and not output.exists(),
            'Output must be new; preserve existing plans')
    recovery.private_directory(output.parent, 0)
    capture, capture_hash = private_json(capture_path)
    offline, offline_hash = private_json(offline_path)
    descriptor, before = backup.open_image(image)
    try:
        primary = backup.read_at(descriptor, 0, 34 * 512)
        tail = backup.read_at(descriptor, recovery.DISK_BYTES - 33 * 512, 33 * 512)
        require(hashlib.sha256(primary + tail).hexdigest() == expected_gpt, 'Backup GPT differs from trusted digest')
        layout = recovery.validate_gpt(primary, tail, sectors=recovery.SECTORS, partitions=recovery.PARTITIONS)
        targets, protected = geometry(layout)
        image_sha256 = check_evidence(capture, offline, expected_gpt, layout)
        require(backup.digest_fd(descriptor) == image_sha256, 'Backup differs from capture/check reports')
        boot = layout['boot']
        require(hashlib.sha256(backup.read_at(descriptor, boot['first_sector'] * 512, boot['bytes'])).hexdigest()
                == recovery.BOOT_SHA256, 'Backup boot mismatch')
        require(hash_coverage(descriptor, targets, protected) == image_sha256, 'Backup changed while computing target digests')
        if capture['action'] == 'inspect':
            require(targets == capture['current_before']['targets'] and protected == capture['current_before']['protected'],
                    'Actual preimage range digests differ from inspection evidence')
        require(backup.signature(os.fstat(descriptor)) == before == backup.signature(image.lstat()),
                'Backup path or metadata changed during planning')
        plan = {'format': 1, 'kind': 'fixed-system-restore-plan', 'passed': True,
                'plan_id': secrets.token_hex(16), 'board': 'ufi003', 'sector_bytes': 512,
                'disk_bytes': recovery.DISK_BYTES, 'source_image': str(image),
                'source_image_sha256': image_sha256, 'gpt_sha256': expected_gpt,
                'capture_action': capture['action'], 'capture_execution_passed': capture['passed'],
                'boot_sha256': recovery.BOOT_SHA256, 'write_targets': targets,
                'protected_ranges': protected,
                'protected_range_policy': 'retain-current-device-bytes-and-verify-before-after',
                'backup_protected_hashes_are_snapshot_only': True,
                'capture_report_sha256': capture_hash, 'offline_report_sha256': offline_hash,
                'planner_sha256': recovery.digest(pathlib.Path(__file__).resolve()),
                'edl_wrapper_sha256': recovery.digest(pathlib.Path(recovery.__file__).resolve()),
                'source_image_unchanged': True, 'usb_opened': False, 'partition_writes': False,
                'execution_authorized': False, 'recovery_verified': False, 'flash_allowed': False,
                'release_ready': False, 'board_boot_verified': False}
        # No output is created until all independent source/evidence checks pass.
        output.mkdir(mode=0o700)
        plan_fd = os.open(output / 'plan.json', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(plan_fd, 'w', encoding='utf-8') as stream:
            json.dump(plan, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        folder = os.open(output, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(folder)
        finally:
            os.close(folder)
        return plan
    finally:
        os.close(descriptor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True, type=pathlib.Path)
    parser.add_argument('--capture-report', required=True, type=pathlib.Path)
    parser.add_argument('--offline-report', required=True, type=pathlib.Path)
    parser.add_argument('--expected-gpt-sha256', required=True)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    args = parser.parse_args()
    os.umask(0o077)
    prepare(args.image, args.capture_report, args.offline_report, args.expected_gpt_sha256, args.output)
    print('Fixed rootfs+NSHSTATE restore plan saved privately; no hardware execution authorized or performed')


if __name__ == '__main__':
    main()
