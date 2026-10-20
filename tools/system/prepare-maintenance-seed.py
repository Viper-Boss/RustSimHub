#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Prepare private, offline per-device SSH/USB material. Never touches a live state mount."""
import argparse
import base64
import hashlib
import json
import os
import pathlib
import re
import secrets
import stat
import struct
import subprocess


def require(condition, message):
    if not condition:
        raise ValueError(message)


def private_dir(path):
    require(path.is_absolute() and path.resolve() == path, 'Canonical private directory required')
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == info.st_gid == 0
            and stat.S_IMODE(info.st_mode) == 0o700, 'Root0700 directory required')


def read_file(path, maximum, modes=(0o600,)):
    require(path.is_absolute() and path.resolve() == path, 'Canonical regular file required')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == info.st_gid == 0
                and stat.S_IMODE(info.st_mode) in modes and info.st_nlink == 1
                and 0 < info.st_size <= maximum, 'Invalid private file metadata')
        with os.fdopen(os.dup(fd), 'rb') as stream:
            data = stream.read(maximum + 1)
        signature = lambda s: (s.st_dev, s.st_ino, s.st_mode, s.st_uid, s.st_gid,
                               s.st_nlink, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
        require(len(data) == info.st_size and signature(info) == signature(os.fstat(fd))
                == signature(path.lstat()), 'Private input changed while reading')
        return data
    finally:
        os.close(fd)


def public_key(data):
    require(len(data) <= 1024 and re.fullmatch(rb'ssh-ed25519 [A-Za-z0-9+/]+={0,2}(?: [\x20-\x7e]*)?\n', data),
            'One plain Ed25519 public key required; options are not accepted')
    token = data.split(b' ')[1].split(b'\n')[0]
    blob = base64.b64decode(token, validate=True)
    require(base64.b64encode(blob) == token and len(blob) == 51
            and blob[:4] == struct.pack('>I', 11) and blob[4:15] == b'ssh-ed25519'
            and blob[15:19] == struct.pack('>I', 32), 'Invalid Ed25519 SSH wire key')
    canonical = b'ssh-ed25519 ' + token
    fingerprint = 'SHA256:' + base64.b64encode(hashlib.sha256(blob).digest()).decode().rstrip('=')
    return canonical, fingerprint


def write_file(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def validate_seed(output, expected_public_sha256, complete=True):
    private_dir(output)
    root = output / 'rustsimhub-v1'
    system = root / 'system'
    access = system / 'ssh-access'
    host = system / 'ssh-host'
    for directory in (root, system, access, host):
        private_dir(directory)
    expected_entries = {'rustsimhub-v1', 'manifest.json'}
    if complete:
        expected_entries.add('complete.marker')
        require(read_file(output/'complete.marker', 128) == b'RustSimHub.OfflineMaintenanceSeed.v1\n',
                'Incomplete offline maintenance seed')
    require(set(p.name for p in output.iterdir()) == expected_entries, 'Unexpected seed root entry')
    require(set(p.name for p in root.iterdir()) == {'system'}, 'Unexpected namespace entry')
    require(set(p.name for p in system.iterdir()) == {'ssh-access', 'ssh-host', 'usb.serial'}, 'Unexpected system entry')
    require(set(p.name for p in access.iterdir()) == {'authorized_keys'}, 'Unexpected access entry')
    require(set(p.name for p in host.iterdir()) == {'key', 'key.pub'}, 'Unexpected host identity entry')
    pub = read_file(access/'authorized_keys', 1024)
    require(hashlib.sha256(pub).hexdigest() == expected_public_sha256, 'Maintenance public file digest changed')
    canonical, fingerprint = public_key(pub)
    read_file(host/'key', 2048)
    host_public, host_fingerprint = public_key(read_file(host/'key.pub', 1024))
    derived = subprocess.run(['/usr/bin/ssh-keygen', '-y', '-P', '', '-f', str(host/'key')],
                             check=True, capture_output=True, timeout=10).stdout
    require(public_key(derived)[0] == host_public, 'SSH host public/private mismatch')
    serial = read_file(system/'usb.serial', 33)
    require(re.fullmatch(rb'[0-9a-f]{32}\n', serial), 'Invalid stable USB identity')
    suffix = hashlib.sha256(serial.rstrip(b'\n')).hexdigest()[:10]
    mac = lambda prefix: prefix + ':' + ':'.join(suffix[i:i+2] for i in range(0, 10, 2))
    manifest = json.loads(read_file(output/'manifest.json', 16384))
    require(type(manifest.get('format')) is int and manifest['format'] == 1
            and manifest.get('kind') == 'offline-maintenance-seed'
            and manifest.get('maintenance_public_sha256') == expected_public_sha256
            and manifest.get('maintenance_fingerprint') == fingerprint
            and manifest.get('host_fingerprint') == host_fingerprint
            and manifest.get('usb_serial_sha256') == hashlib.sha256(serial).hexdigest()
            and manifest.get('usb_device_mac') == mac('02') and manifest.get('usb_host_mac') == mac('06')
            and all(manifest.get(flag) is False for flag in ('device_installed', 'live_state_changed',
                    'client_private_key_supplied', 'recovery_verified', 'board_boot_verified', 'release_ready')),
            'Seed manifest mismatch')
    return manifest, canonical, host_public


def prepare(public_path, expected_public_sha256, output):
    require(os.geteuid() == 0, 'Local Root operator required')
    os.umask(0o077)
    require(re.fullmatch('[0-9a-f]{64}', expected_public_sha256), 'Exact public file SHA256 required')
    private_dir(public_path.parent)
    pub = read_file(public_path, 1024)
    require(hashlib.sha256(pub).hexdigest() == expected_public_sha256, 'Maintenance public file digest mismatch')
    _, fingerprint = public_key(pub)
    private_dir(output.parent)
    require(output.is_absolute() and output.resolve() == output, 'Canonical new output required')
    output.mkdir(mode=0o700)  # exclusive: failed/partial attempts are preserved, never regenerated
    root = output/'rustsimhub-v1'
    system = root/'system'
    access = system/'ssh-access'
    host = system/'ssh-host'
    for directory in (root, system, access, host):
        directory.mkdir(mode=0o700)
    write_file(access/'authorized_keys', pub)
    subprocess.run(['/usr/bin/ssh-keygen', '-q', '-t', 'ed25519', '-N', '',
                    '-C', 'RustSimHub.Maintenance', '-f', str(host/'key')],
                   check=True, capture_output=True, timeout=15)
    (host/'key.pub').chmod(0o600)
    host_pub = read_file(host/'key.pub', 1024)
    _, host_fingerprint = public_key(host_pub)
    serial = secrets.token_hex(16).encode() + b'\n'
    write_file(system/'usb.serial', serial)
    suffix = hashlib.sha256(serial.rstrip(b'\n')).hexdigest()[:10]
    mac = lambda prefix: prefix + ':' + ':'.join(suffix[i:i+2] for i in range(0, 10, 2))
    manifest = {'format': 1, 'kind': 'offline-maintenance-seed',
                'maintenance_public_sha256': expected_public_sha256, 'maintenance_fingerprint': fingerprint,
                'host_fingerprint': host_fingerprint, 'usb_serial_sha256': hashlib.sha256(serial).hexdigest(),
                'usb_device_mac': mac('02'), 'usb_host_mac': mac('06'),
                'device_installed': False, 'live_state_changed': False, 'client_private_key_supplied': False,
                'recovery_verified': False, 'board_boot_verified': False, 'release_ready': False}
    write_file(output/'manifest.json', json.dumps(manifest, indent=2).encode())
    for path in (host/'key', host/'key.pub'):
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    for directory in (host, access, system, root, output, output.parent):
        sync_dir(directory)
    validate_seed(output, expected_public_sha256, complete=False)
    write_file(output/'complete.marker', b'RustSimHub.OfflineMaintenanceSeed.v1\n')
    sync_dir(output)
    validate_seed(output, expected_public_sha256)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--maintenance-public-key', required=True, type=pathlib.Path)
    parser.add_argument('--expected-public-sha256', required=True)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    args = parser.parse_args()
    prepare(args.maintenance_public_key, args.expected_public_sha256, args.output)
    print('Offline maintenance seed prepared privately; no device or live state changed')


if __name__ == '__main__':
    main()
