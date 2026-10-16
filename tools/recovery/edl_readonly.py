#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed UFI003 EDL capture on its USB host. No flash or arbitrary EDL commands.

Requires the operator's pinned, separately supplied bkerler/edl source and loader.
All output, including GPT/device identifiers, belongs in a private directory.
"""
import argparse
import hashlib
import importlib
import json
import os
import pathlib
import re
import signal
import stat
import struct
import sys
import tarfile
import xml.etree.ElementTree as ET
import zlib

SOURCE_REVISION = '1cda1a616748d608667b2d13b73a28d3197ceb45'
SOURCE_SHA256 = '2907e7f68b179c1cc08bbcbab5e240ce8efcca1c0673ce0bbd2e351e619cec7c'
LOADER_SHA256 = '959439aa5864685999b713c3ed12ad5fa408149648b670a9a9ef77bcc9dcab14'
DISK_BYTES = 3909091328
SECTORS = DISK_BYTES // 512
BOOT_SHA256 = 'd5790e07a8ee67c6d80ec24ea9b0cd4b5a78ab4440a3de2b8767fea58a03eaee'
PARTITIONS = {12: ('boot', 67108864), 14: ('rootfs', 3436183552), 15: ('NSHSTATE', 135249408)}
SOURCE_LINKS = {'edlclient/Tools/Config': '../Config', 'edlclient/Tools/Library': '../Library', 'edlclient/edl.py': '../edl.py'}


def require(condition, label):
    if not condition:
        raise ValueError(label)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for data in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            h.update(data)
    return h.hexdigest()


def private_directory(path, owner):
    require(path.is_absolute() and path.resolve() == path, 'Noncanonical private directory')
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == owner and stat.S_IMODE(info.st_mode) == 0o700,
            'Private directory owner/mode mismatch')


def verify_tools(base):
    require(base.is_absolute() and base.resolve() == base, 'Noncanonical tool directory')
    for name, expected in [('source.tar.gz', SOURCE_SHA256), ('prog_emmc_firehose_8916.mbn', LOADER_SHA256)]:
        file = base / name
        require(stat.S_ISREG(file.lstat().st_mode) and file.stat().st_nlink == 1, 'Linked tool input')
        require(digest(file) == expected, 'Pinned tool digest mismatch')
    source = base / 'source'
    require(source.resolve() == source and source.is_dir(), 'Linked source directory')
    with tarfile.open(base / 'source.tar.gz', 'r:gz') as archive:
        names = set()
        for member in archive:
            relative = pathlib.PurePosixPath(member.name)
            require(not relative.is_absolute() and '..' not in relative.parts and member.name not in names,
                    'Unsafe source archive entry')
            names.add(member.name)
            if member.isdir():
                continue
            if member.issym():
                actual = source / member.name
                require(SOURCE_LINKS.get(member.name) == member.linkname and actual.is_symlink()
                        and os.readlink(actual) == member.linkname and actual.resolve().is_relative_to(source),
                        'Unexpected or escaping source link')
                continue
            require(member.isfile(), 'Source archive contains a link or device')
            actual = source / member.name
            require(actual.resolve() == actual and stat.S_ISREG(actual.lstat().st_mode), 'Linked source file')
            require(actual.stat().st_nlink == 1 and actual.stat().st_size == member.size, 'Source file metadata mismatch')
            require(hashlib.sha256(actual.read_bytes()).digest() == hashlib.sha256(archive.extractfile(member).read()).digest(),
                    'Extracted source differs from pinned archive')
    # Unexpected Python files could shadow verified imports. Bytecode is disabled.
    require(all(p.relative_to(source).as_posix() in names for p in source.rglob('*.py')), 'Extra Python source')
    return source


def validate_xml(data, allow_reset=False):
    require(len(data) <= 8192, 'Oversize Firehose XML')
    require(b'<!' not in (data.encode() if isinstance(data, str) else data), 'XML declarations refused')
    root = ET.fromstring(data)
    require(root.tag == 'data' and not root.attrib and len(root) == 1, 'Expected one Firehose operation')
    command = root[0]
    require(not list(command), 'Nested Firehose command')
    if command.tag == 'configure':
        require(command.get('MemoryName', '').lower() == 'emmc', 'Only eMMC is supported')
        require(command.get('SkipWrite') == '1', 'SkipWrite must remain enabled')
        require(0 < int(command.get('MaxPayloadSizeToTargetInBytes', '0')) <= 32768, 'Unsafe payload size')
    elif command.tag == 'read':
        allowed = {'SECTOR_SIZE_IN_BYTES', 'num_partition_sectors', 'physical_partition_number', 'start_sector'}
        require(set(command.attrib) == allowed, 'Unexpected read attributes')
        require(command.get('SECTOR_SIZE_IN_BYTES') == '512' and command.get('physical_partition_number') == '0',
                'Only the fixed eMMC user area can be read')
        start = int(command.get('start_sector', '-1'))
        count = int(command.get('num_partition_sectors', '0'))
        require(0 <= start < SECTORS and 0 < count <= SECTORS - start, 'Read exceeds fixed disk bounds')
    elif command.tag == 'getstorageinfo':
        require(command.attrib in ({}, {'physical_partition_number': '0'}), 'Unsupported storage query')
    elif command.tag == 'nop':
        require(not command.attrib, 'Unsupported nop attributes')
    elif command.tag == 'power':
        require(allow_reset and command.attrib == {'value': 'reset'}, 'Only an explicitly requested normal reset is allowed')
    else:
        raise ValueError('Non-read-only Firehose operation refused')


def validate_gpt(primary, backup, sectors=SECTORS, partitions=PARTITIONS):
    require(len(primary) == 34 * 512 and len(backup) == 33 * 512, 'GPT capture length mismatch')

    def header(blob, expected_lba, peer_lba):
        require(blob[:8] == b'EFI PART', 'Missing GPT signature')
        revision, size, checksum, reserved = struct.unpack_from('<4I', blob, 8)
        require(revision == 0x10000 and size == 92 and reserved == 0, 'Unsupported GPT header')
        checked = bytearray(blob[:size])
        checked[16:20] = bytes(4)
        require(zlib.crc32(checked) == checksum, 'GPT header CRC mismatch')
        current, peer, first, last = struct.unpack_from('<4Q', blob, 24)
        require((current, peer) == (expected_lba, peer_lba) and 34 <= first <= last < sectors - 33,
                'GPT disk bounds mismatch')
        entries_lba, count, entry_size, entries_crc = struct.unpack_from('<Q3I', blob, 72)
        require(count == 16 and entry_size == 128, 'Unsupported GPT entry geometry')
        return first, last, blob[56:72], entries_lba, entries_crc

    a = header(primary[512:1024], 1, sectors - 1)
    b = header(backup[-512:], sectors - 1, 1)
    require(a[:3] == b[:3] and a[3] == 2 and b[3] == sectors - 33, 'GPT header copies disagree')
    entries = primary[1024:1024 + 16 * 128]
    require(entries == backup[:16 * 128] and zlib.crc32(entries) == a[4] == b[4], 'GPT entry copies or CRC disagree')
    result = {}
    ranges = []
    for index in range(16):
        row = entries[index * 128:(index + 1) * 128]
        if row[:16] == bytes(16):
            continue
        first, last = struct.unpack_from('<2Q', row, 32)
        name = row[56:128].decode('utf-16-le').split('\0', 1)[0]
        require(name and name not in result and a[0] <= first <= last <= a[1], 'Invalid GPT partition')
        require(all(last < start or first > end for start, end in ranges), 'Overlapping GPT partitions')
        ranges.append((first, last))
        result[name] = {'index': index + 1, 'first_sector': first, 'bytes': (last - first + 1) * 512}
    for index, (name, size) in partitions.items():
        require(name in result and result[name]['index'] == index and result[name]['bytes'] == size,
                'Observed board partition layout mismatch')
    return result


def select_usb(port, sysfs=pathlib.Path('/sys/bus/usb/devices')):
    require(re.fullmatch(r'[1-9][0-9]*-[1-9][0-9]*(?:\.[1-9][0-9]*)*', port) is not None, 'Invalid physical USB port')
    devices = []
    for item in sysfs.iterdir():
        if (item / 'idVendor').exists() and (item / 'idProduct').exists():
            if ((item / 'idVendor').read_text().strip(), (item / 'idProduct').read_text().strip()) == ('05c6', '9008'):
                devices.append(item)
    require(len(devices) == 1 and devices[0].name == port, 'Expected exactly one 9008 device on the recorded physical port')
    return int((devices[0] / 'busnum').read_text()), int((devices[0] / 'devnum').read_text())


def load_runtime(source, loader, action, port):
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(source))
    import usb.backend.libusb1
    import usb.core
    from edlclient.Library.Connection.usblib import usb_class
    from edlclient.Library.firehose import firehose
    import edlclient.Library.firehose_client
    import edlclient.Library.sahara
    import edlclient.Library.streaming_client
    backend = usb.backend.libusb1.get_backend(find_library=lambda _: 'libusb-1.0.so.0')
    require(backend is not None, 'Pinned libusb runtime unavailable')
    original_xmlsend = firehose.xmlsend

    def guarded_xmlsend(self, data, *args, **kwargs):
        validate_xml(data, action == 'reset')
        return original_xmlsend(self, data, *args, **kwargs)

    firehose.xmlsend = guarded_xmlsend

    def refused(*args, **kwargs):
        raise ValueError('Mutating or arbitrary Firehose method is disabled')

    for name in ('cmd_program', 'cmd_program_buffer', 'cmd_erase', 'cmd_patch', 'cmd_poke', 'cmd_memcpy',
                 'cmd_setactiveslot', 'cmd_setbootablestoragedrive', 'cmd_writeimei', 'cmd_send', 'cmd_xml', 'cmd_rawxml'):
        setattr(firehose, name, refused)
    if action == 'runtime-check':
        return None
    bus, address = select_usb(port)
    original_find = usb.core.find

    def fixed_find(find_all=False, **kwargs):
        require(select_usb(port) == (bus, address), 'Recorded USB device changed')
        kwargs.update(backend=backend, idVendor=0x05c6, idProduct=0x9008, bus=bus, address=address)
        return original_find(find_all=find_all, **kwargs)

    usb.core.find = fixed_find
    original_init = usb_class.__init__

    def fixed_init(self, *args, **kwargs):
        original_init(self, *args, **kwargs)
        self.backend = backend

    usb_class.__init__ = fixed_init
    old_argv = sys.argv
    try:
        # Import parses this fixed command; imported-client mode only connects.
        sys.argv = ['edl.py', 'printgpt', '--loader=' + str(loader), '--memory=emmc', '--vid=0x05c6', '--pid=0x9008']
        module = importlib.import_module('edl')
    finally:
        sys.argv = old_argv
    module.args.update({'--maxpayload': '32768', '--sectorsize': '512', '--memory': 'emmc', '--skipwrite': True})
    client = module.main(module.args)
    require(client.run() == 0 and client.fh is not None, 'EDL connection failed')
    require(client.cdc.vid == 0x05c6 and client.cdc.pid == 0x9008 and client.fh.firehose.cfg.MemoryName.lower() == 'emmc',
            'Unexpected EDL device or storage mode')
    return client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tools', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--action', required=True, choices=('runtime-check', 'gpt', 'read-emmc', 'reset'))
    parser.add_argument('--usb-port')
    parser.add_argument('--expected-gpt-sha256')
    args = parser.parse_args()
    os.umask(0o077)
    private_directory(args.output, os.geteuid())
    require(not list(args.output.iterdir()), 'Output must be new and empty; no retry or overwrite')
    if args.action != 'runtime-check':
        require(os.geteuid() == 0, 'EDL USB actions require the local Root operator')
        private_directory(args.tools, 0)
        require(args.usb_port is not None, 'Recorded physical USB port required')
        if args.action != 'reset':
            require(re.fullmatch('[0-9a-f]{64}', args.expected_gpt_sha256 or '') is not None, 'Trusted prior GPT digest required')
    report = {'format': 1, 'action': args.action, 'partition_writes': False, 'passed': False,
              'source_revision': SOURCE_REVISION, 'loader_sha256': LOADER_SHA256, 'boot_areas_captured': False,
              'wrapper_sha256': digest(pathlib.Path(__file__).resolve())}
    client = None
    signal.signal(signal.SIGALRM, lambda *_: (_ for _ in ()).throw(TimeoutError('Bounded EDL operation expired')))
    signal.alarm(1800 if args.action == 'read-emmc' else 120)
    try:
        source = verify_tools(args.tools)
        os.chdir(args.output)
        sys.pycache_prefix = str(args.output / 'unused-bytecode')
        client = load_runtime(source, args.tools / 'prog_emmc_firehose_8916.mbn', args.action, args.usb_port)
        if args.action == 'reset':
            require(client.fh.firehose.cmd_reset('reset') is True, 'Reset acknowledgement missing; inspect before retry')
        elif args.action in ('gpt', 'read-emmc'):
            fh = client.fh.firehose
            primary = args.output / 'gpt-primary.bin'
            backup = args.output / 'gpt-backup.bin'
            require(fh.cmd_read(0, 0, 34, str(primary), display=False) is True, 'Primary GPT read failed')
            require(fh.cmd_read(0, SECTORS - 33, 33, str(backup), display=False) is True, 'Backup GPT read failed')
            primary_data, backup_data = primary.read_bytes(), backup.read_bytes()
            layout = validate_gpt(primary_data, backup_data)
            gpt_hash = hashlib.sha256(primary_data + backup_data).hexdigest()
            require(gpt_hash == args.expected_gpt_sha256, 'EDL GPT differs from trusted live-disk capture')
            report['gpt_sha256'] = gpt_hash
            if args.action == 'read-emmc':
                partial = args.output / 'mmcblk0.img.partial'
                require(fh.cmd_read(0, 0, SECTORS, str(partial), display=False) is True, 'Full eMMC read failed')
                require(partial.stat().st_size == DISK_BYTES, 'Short eMMC capture')
                with partial.open('rb') as stream:
                    require(stream.read(34 * 512) == primary_data, 'Captured primary GPT changed')
                    stream.seek(DISK_BYTES - 33 * 512)
                    require(stream.read() == backup_data, 'Captured backup GPT changed')
                    stream.seek(layout['boot']['first_sector'] * 512)
                    require(hashlib.sha256(stream.read(layout['boot']['bytes'])).hexdigest() == BOOT_SHA256, 'Current boot digest mismatch')
                    os.fsync(stream.fileno())
                report.update(bytes=DISK_BYTES, sha256=digest(partial), gpt_copies_checked=True, current_boot_verified=True)
                partial.rename(args.output / 'mmcblk0.img')
                folder = os.open(args.output, os.O_RDONLY | os.O_DIRECTORY)
                try:
                    os.fsync(folder)
                finally:
                    os.close(folder)
        report['passed'] = True
        print('Fixed EDL operation passed; private evidence saved')
    finally:
        signal.alarm(0)
        if client is not None and client.cdc is not None:
            client.cdc.close()
        with (args.output / 'report.json').open('x', encoding='utf-8') as stream:
            json.dump(report, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())


if __name__ == '__main__':
    main()
