#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Offline GPT plan for porting a second-hand stock-Android UFI003 in place.

Reads the stick's own primary/backup GPT dumps, keeps every retained entry
byte-identical (same LBAs, type and unique GUIDs, attributes), drops the
Android system area and adds boot/rootfs/NSHSTATE/devinfo in the freed space.
Writes only new files in a new output directory: the primary GPT (protective
MBR + header + entries, 34 sectors), the backup GPT (entries + header,
33 sectors) and plan.json. Never opens a device.
"""
import argparse
import hashlib
import json
import pathlib
import struct
import sys
import uuid
import zlib

SECTOR = 512
DISK_SECTORS = 7634944
# NV/calibration and the boot chain: must keep exact LBAs and never be targets.
PROTECTED = ('modem', 'sbl1', 'sbl1bak', 'aboot', 'abootbak', 'rpm', 'rpmbak', 'tz', 'tzbak', 'hyp', 'hypbak',
             'pad', 'modemst1', 'modemst2', 'misc', 'fsc', 'ssd', 'splash', 'DDR', 'fsg', 'sec', 'persist')
DROPPED = ('boot', 'system', 'cache', 'recovery', 'userdata')
EXPECTED_STOCK = ('modem', 'sbl1', 'sbl1bak', 'aboot', 'abootbak', 'rpm', 'rpmbak', 'tz', 'tzbak', 'hyp', 'hypbak',
                  'pad', 'modemst1', 'modemst2', 'misc', 'fsc', 'ssd', 'splash', 'DDR', 'fsg', 'sec', 'boot',
                  'system', 'persist', 'cache', 'recovery', 'userdata')
LINUX_DATA = uuid.UUID('0fc63daf-8483-4772-8e79-3d69d8477de4')
DEVINFO_TYPE = uuid.UUID('65addcf4-0c5c-4d9a-ac2d-d90b5cbfcd03')
NAMESPACE = uuid.UUID('6e736866-7566-6930-3033-736563737469')
# name -> (first sector, sectors, type GUID or None to keep stock boot's type)
NEW = {
    'boot': (396384, 131072, None),
    'NSHSTATE': (527456, 264159, LINUX_DATA),
    'devinfo': (792576, 2048, DEVINFO_TYPE),
    'rootfs': (2134016, 5500888, LINUX_DATA),
}
# Final entry order; NSHSTATE must be partition 15 (rustsimhub-state-check).
ORDER = ('modem', 'sbl1', 'sbl1bak', 'aboot', 'abootbak', 'rpm', 'rpmbak', 'tz', 'tzbak', 'hyp', 'hypbak',
         'boot', 'modemst1', 'rootfs', 'NSHSTATE', 'modemst2', 'misc', 'fsc', 'ssd', 'splash', 'DDR', 'fsg',
         'sec', 'persist', 'pad', 'devinfo')


class PlanError(Exception):
    pass


def require(condition, reason):
    if not condition:
        raise PlanError(reason)


def parse_header(raw):
    require(raw[:8] == b'EFI PART', 'GPT signature missing')
    size, = struct.unpack('<I', raw[12:16])
    require(size == 92, 'Unexpected GPT header size')
    header = bytearray(raw[:92])
    stored, = struct.unpack('<I', header[16:20])
    header[16:20] = b'\0' * 4
    require(zlib.crc32(header) == stored, 'GPT header CRC mismatch')
    current, backup, first, last = struct.unpack('<QQQQ', raw[24:56])
    entries_lba, count, entry_size, entries_crc = struct.unpack('<QIII', raw[72:92])
    return dict(raw=bytes(raw[:92]), current=current, backup=backup, first=first, last=last,
                entries_lba=entries_lba, count=count, entry_size=entry_size, entries_crc=entries_crc)


def parse_entries(raw, header):
    require(header['entry_size'] == 128, 'Unexpected GPT entry size')
    table = raw[:header['count'] * 128]
    require(zlib.crc32(table) == header['entries_crc'], 'GPT entry array CRC mismatch')
    entries = []
    for index in range(header['count']):
        entry = table[index * 128:(index + 1) * 128]
        if entry[:16] == b'\0' * 16:
            continue
        first, last, attributes = struct.unpack('<QQQ', entry[32:56])
        name = entry[56:128].decode('utf-16le').rstrip('\0')
        entries.append(dict(index=index + 1, name=name, first=first, last=last, raw=bytes(entry),
                            type=str(uuid.UUID(bytes_le=entry[:16]))))
    return entries


def read_stock(primary, backup):
    require(len(primary) == 34 * SECTOR and len(backup) == 33 * SECTOR, 'GPT dumps have unexpected lengths')
    head = parse_header(primary[SECTOR:2 * SECTOR])
    tail = parse_header(backup[32 * SECTOR:])
    require((head['current'], head['backup'], head['entries_lba']) == (1, DISK_SECTORS - 1, 2),
            'Primary header does not describe the fixed disk')
    require((tail['current'], tail['backup'], tail['entries_lba']) == (DISK_SECTORS - 1, 1, DISK_SECTORS - 33),
            'Backup header does not describe the fixed disk')
    require(head['raw'][56:72] == tail['raw'][56:72], 'Primary and backup disk GUIDs differ')
    require(head['count'] * 128 <= 32 * SECTOR, 'GPT entry array too large')
    entries = parse_entries(primary[2 * SECTOR:], head)
    require(parse_entries(backup, tail) == entries, 'Primary and backup entries differ')
    require(tuple(e['name'] for e in entries) == EXPECTED_STOCK, 'Not the expected stock Android layout')
    return head, entries


def entry_bytes(name, first, sectors, type_guid, attributes=0):
    unique = uuid.uuid5(NAMESPACE, name)
    encoded = name.encode('utf-16le')
    require(len(encoded) <= 72, 'Partition name too long')
    return (type_guid.bytes_le + unique.bytes_le + struct.pack('<QQQ', first, first + sectors - 1, attributes)
            + encoded + b'\0' * (72 - len(encoded)))


def header_bytes(stock, current, backup, entries_lba, entries_crc):
    header = bytearray(stock['raw'])
    header[24:40] = struct.pack('<QQ', current, backup)
    header[72:80] = struct.pack('<Q', entries_lba)
    header[88:92] = struct.pack('<I', entries_crc)
    header[16:20] = b'\0' * 4
    header[16:20] = struct.pack('<I', zlib.crc32(header))
    return bytes(header) + b'\0' * (SECTOR - 92)


def build(primary, backup):
    head, stock = read_stock(primary, backup)
    by_name = {e['name']: e for e in stock}
    rows = {}
    for name in PROTECTED:
        rows[name] = by_name[name]['raw']
    boot_type = uuid.UUID(by_name['boot']['type'])
    for name, (first, sectors, type_guid) in NEW.items():
        rows[name] = entry_bytes(name, first, sectors, type_guid or boot_type)
    table = b''.join(rows[name] for name in ORDER)
    require(len(ORDER) <= head['count'], 'More partitions than the stock entry array holds')
    table += b'\0' * (head['count'] * 128 - len(table))
    crc = zlib.crc32(table)
    padded = table + b'\0' * (32 * SECTOR - len(table))
    new_primary = primary[:SECTOR] + header_bytes(head, 1, DISK_SECTORS - 1, 2, crc) + padded
    new_backup = padded + header_bytes(head, DISK_SECTORS - 1, 1, DISK_SECTORS - 33, crc)
    layout = check(new_primary, new_backup, stock)
    return new_primary, new_backup, layout


def check(new_primary, new_backup, stock):
    """Re-parse the generated tables and prove the safety properties."""
    head = parse_header(new_primary[SECTOR:2 * SECTOR])
    entries = parse_entries(new_primary[2 * SECTOR:], head)
    tail = parse_header(new_backup[32 * SECTOR:])
    require(parse_entries(new_backup, tail) == entries, 'Generated primary and backup differ')
    require(tuple(e['name'] for e in entries) == ORDER, 'Generated order wrong')
    by_index = {e['index']: e for e in entries}
    require(by_index[15]['name'] == 'NSHSTATE' and by_index[15]['last'] - by_index[15]['first'] + 1 == 264159,
            'NSHSTATE must be partition 15 with 264159 sectors')
    old = {e['name']: e for e in stock}
    new = {e['name']: e for e in entries}
    for name in PROTECTED:
        require(new[name]['raw'] == old[name]['raw'], f'{name}: retained entry changed')
    spans = sorted((e['first'], e['last'], e['name']) for e in entries)
    for (a_first, a_last, a_name), (b_first, _b_last, b_name) in zip(spans, spans[1:]):
        require(a_last < b_first, f'{a_name} overlaps {b_name}')
    require(spans[0][0] >= head['first'] and spans[-1][1] <= head['last'], 'Partition outside usable area')
    targets = []
    for name in NEW:
        e = new[name]
        for keep in PROTECTED:
            k = old[keep]
            require(e['last'] < k['first'] or e['first'] > k['last'], f'{name} would touch protected {keep}')
        targets.append(dict(name=name, index=e['index'], first_sector=e['first'],
                            sectors=e['last'] - e['first'] + 1, bytes=(e['last'] - e['first'] + 1) * SECTOR))
    protected = [dict(name=n, first_sector=old[n]['first'], sectors=old[n]['last'] - old[n]['first'] + 1)
                 for n in PROTECTED]
    return dict(partitions=[dict(index=e['index'], name=e['name'], first_sector=e['first'],
                                 sectors=e['last'] - e['first'] + 1) for e in entries],
                new_targets=targets, protected=protected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--primary', type=pathlib.Path, required=True)
    parser.add_argument('--backup', type=pathlib.Path, required=True)
    parser.add_argument('--out', type=pathlib.Path, required=True, help='new directory; must not exist')
    args = parser.parse_args()
    try:
        primary, backup = args.primary.read_bytes(), args.backup.read_bytes()
        new_primary, new_backup, layout = build(primary, backup)
        args.out.mkdir(mode=0o700)
        (args.out / 'gpt-primary.bin').write_bytes(new_primary)
        (args.out / 'gpt-backup.bin').write_bytes(new_backup)
        plan = dict(format=1, kind='second-board-gpt-plan', device_writes=0, execution_authorized=False,
                    stock_primary_sha256=hashlib.sha256(primary).hexdigest(),
                    stock_backup_sha256=hashlib.sha256(backup).hexdigest(),
                    primary=dict(first_sector=0, sectors=34, sha256=hashlib.sha256(new_primary).hexdigest()),
                    backup=dict(first_sector=DISK_SECTORS - 33, sectors=33,
                                sha256=hashlib.sha256(new_backup).hexdigest()),
                    **layout)
        (args.out / 'plan.json').write_text(json.dumps(plan, indent=1) + '\n')
    except (PlanError, OSError) as error:
        print(f'refused: {error}', file=sys.stderr)
        return 1
    print(f'GPT plan written to {args.out}; nothing was sent to any device')
    return 0


if __name__ == '__main__':
    sys.exit(main())
