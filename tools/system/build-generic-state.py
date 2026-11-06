#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build the generic NSHSTATE image of a flash pack. Never opens a device.

The image holds exactly: the publisher's maintenance PUBLIC key and the fixed first-boot
request. Firmware, NV, identity, host keys, USB serial and every other per-device value are
created on the device by rustsimhub-firmware-extract, rustsimhub-firstboot,
rustsimhub-ssh-prepare and rustsimhub-generic-install.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import stat
import subprocess
import sys

PARTITION_SECTORS = 264159  # rustsimhub-state-check requires exactly this partition size
BLOCK = 16384               # UsbTool writes only full blocks
IMAGE_BYTES = PARTITION_SECTORS * 512 // BLOCK * BLOCK
FS_BLOCK = 4096
REQUEST = b'RustSimHub.GenericProvision.v1\n'
UUID = '6e736866-7566-6930-3033-737461746500'
HASH_SEED = '6e736866-7566-6930-3033-737461746501'
EPOCH = 1735689600
PUBLIC_KEY = re.compile(rb'ssh-ed25519 [A-Za-z0-9+/=]{20,}( [\x20-\x7e]{1,128})?\n')
EXPECTED_TREE = {
    'lost+found', 'rustsimhub-v1', 'rustsimhub-v1/system', 'rustsimhub-v1/system/ssh-access',
    'rustsimhub-v1/system/ssh-access/authorized_keys', 'rustsimhub-v1/system/generic-provision.request',
}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def read_public_key(path, expected_sha256):
    """Only a single option-free ssh-ed25519 line passes; a private key can never match."""
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_size <= 1024, 'Maintenance key must be a small regular file')
    data = path.read_bytes()
    require(hashlib.sha256(data).hexdigest() == expected_sha256, 'Maintenance public key digest differs')
    require(PUBLIC_KEY.fullmatch(data), 'Expected exactly one ssh-ed25519 public key line')
    subprocess.run(['ssh-keygen', '-l', '-f', str(path)], check=True, stdout=subprocess.DEVNULL, timeout=10)
    return data


def stage(root, key):
    root.mkdir(mode=0o700)
    root.chmod(0o700)
    directory = root
    for name in ('rustsimhub-v1', 'system', 'ssh-access'):
        directory = directory/name
        directory.mkdir(mode=0o700)
        directory.chmod(0o700)
    for path, data in ((directory/'authorized_keys', key), (root/'rustsimhub-v1/system/generic-provision.request', REQUEST)):
        with path.open('xb') as stream:
            stream.write(data)
        path.chmod(0o600)
    for path in [root, *root.rglob('*')]:
        os.chown(path, 0, 0, follow_symlinks=False)
        os.utime(path, (EPOCH, EPOCH), follow_symlinks=False)


def tree(image, env):
    """Every entry with owner and mode, read back from the finished image."""
    rows = {}

    def walk(prefix):
        output = subprocess.check_output(['debugfs', '-R', 'ls -p /'+prefix, str(image)],
                                         stderr=subprocess.DEVNULL, text=True, timeout=30, env=env)
        for line in output.splitlines():
            fields = line.split('/')
            if len(fields) < 7 or fields[5] in ('', '.', '..'):
                continue
            mode, uid, gid, name = int(fields[2], 8), int(fields[3]), int(fields[4]), fields[5]
            path = (prefix+'/'+name).lstrip('/')
            rows[path] = {'mode': oct(stat.S_IMODE(mode)), 'uid': uid, 'gid': gid, 'directory': stat.S_ISDIR(mode)}
            if stat.S_ISDIR(mode):
                walk(path)
    walk('')
    return rows


def build(key_path, expected_sha256, out):
    require(os.geteuid() == 0 and sys.platform == 'linux', 'Linux root required for numeric owners')
    require(not out.exists() and not out.is_symlink(), 'Output must be new; prior evidence retained')
    key = read_public_key(key_path, expected_sha256)
    out.mkdir(mode=0o700)
    image = out/'NSHSTATE.img'
    report = {'format': 1, 'kind': 'generic-nshstate-image', 'passed': False, 'bytes': IMAGE_BYTES,
              'partition_sectors': PARTITION_SECTORS, 'filesystem_sectors': IMAGE_BYTES//512,
              'maintenance_public_sha256': expected_sha256, 'device_written': False,
              'contains_firmware': False, 'contains_identity': False, 'contains_local_volte': False,
              'contains_private_key': False, 'builder_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()}
    env = dict(os.environ, E2FSPROGS_FAKE_TIME=str(EPOCH), LC_ALL='C')
    try:
        stage(out/'stage', key)
        with image.open('xb') as stream:
            image.chmod(0o600)
            stream.truncate(IMAGE_BYTES)
        subprocess.run(['mke2fs', '-q', '-t', 'ext4', '-F', '-b', str(FS_BLOCK), '-L', 'NSHSTATE', '-U', UUID,
                        '-E', 'lazy_itable_init=0,lazy_journal_init=0,hash_seed='+HASH_SEED,
                        '-d', str(out/'stage'), str(image), str(IMAGE_BYTES//FS_BLOCK)],
                       check=True, timeout=120, env=env)
        subprocess.run(['e2fsck', '-f', '-n', str(image)], check=True, stdout=subprocess.DEVNULL, timeout=60, env=env)
        fields = dict(line.split('=', 1) for line in subprocess.check_output(
            ['blkid', '-p', '-o', 'export', str(image)], text=True, timeout=20).splitlines() if '=' in line)
        require(fields.get('TYPE') == 'ext4' and fields.get('LABEL') == 'NSHSTATE', 'Wrong volume identity')
        rows = tree(image, env)
        require(set(rows) == EXPECTED_TREE, 'Image holds an unexpected entry')
        for name, row in rows.items():
            require((row['uid'], row['gid']) == (0, 0), 'Every entry must be root-owned')
            require(row['mode'] == ('0o700' if row['directory'] else '0o600'), 'Every entry must be private')
        size = image.stat().st_size
        require(size == IMAGE_BYTES and size % BLOCK == 0 and size < PARTITION_SECTORS*512, 'Image length contract')
        digest = hashlib.sha256()
        with image.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1 << 20), b''):
                digest.update(chunk)
        report.update(passed=True, image_sha256=digest.hexdigest(), tree=rows, volume=fields)
        return report
    finally:
        (out/'GENERIC-STATE.json').write_text(json.dumps(report, indent=2)+'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--maintenance-public-key', type=pathlib.Path, required=True)
    parser.add_argument('--expected-public-sha256', required=True)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args()
    report = build(args.maintenance_public_key, args.expected_public_sha256, args.out.absolute())
    print('Generic NSHSTATE image built:', report['image_sha256'], report['bytes'])


if __name__ == '__main__':
    main()
