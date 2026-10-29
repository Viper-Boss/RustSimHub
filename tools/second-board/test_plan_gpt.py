#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Synthetic-GPT tests for plan_gpt.py; no device or private dump is used."""
import importlib.util
import pathlib
import struct
import sys
import tempfile
import unittest
import uuid
import zlib

spec = importlib.util.spec_from_file_location('plan_gpt', pathlib.Path(__file__).with_name('plan_gpt.py'))
plan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plan)

BASIC = uuid.UUID('ebd0a0a2-b9e5-4433-87c0-68b6b72699c7')
# Same LBAs as the observed stock Android layout.
STOCK = [('modem', 131072, 262143), ('sbl1', 262144, 263167), ('sbl1bak', 263168, 264191),
         ('aboot', 264192, 266239), ('abootbak', 266240, 268287), ('rpm', 268288, 269311),
         ('rpmbak', 269312, 270335), ('tz', 270336, 271359), ('tzbak', 271360, 272383),
         ('hyp', 272384, 273407), ('hypbak', 273408, 274431), ('pad', 274432, 276479),
         ('modemst1', 276480, 279551), ('modemst2', 279552, 282623), ('misc', 282624, 284671),
         ('fsc', 284672, 284673), ('ssd', 284674, 284689), ('splash', 284690, 305169),
         ('DDR', 393216, 393279), ('fsg', 393280, 396351), ('sec', 396352, 396383),
         ('boot', 396384, 429151), ('system', 429152, 2067551), ('persist', 2067552, 2133087),
         ('cache', 2133088, 2395231), ('recovery', 2395232, 2427999), ('userdata', 2428000, 7634910)]
N = plan.DISK_SECTORS


def stock(rows=STOCK, count=28, tamper_backup=False):
    table = b''
    for i, (name, first, last) in enumerate(rows):
        table += (BASIC.bytes_le + uuid.uuid4().bytes_le + struct.pack('<QQQ', first, last, i)
                  + name.encode('utf-16le').ljust(72, b'\0'))
    table = table.ljust(count * 128, b'\0')
    crc = zlib.crc32(table)
    padded = table.ljust(32 * 512, b'\0')
    disk = uuid.uuid4().bytes_le

    def header(current, backup, entries):
        h = bytearray(b'EFI PART' + struct.pack('<III', 0x10000, 92, 0) + b'\0' * 4
                      + struct.pack('<QQQQ', current, backup, 34, N - 34) + disk
                      + struct.pack('<QIII', entries, count, 128, crc))
        h[16:20] = struct.pack('<I', zlib.crc32(h))
        return bytes(h).ljust(512, b'\0')
    mbr = b'\0' * 510 + b'\x55\xaa'
    primary = mbr + header(1, N - 1, 2) + padded
    backup_table = bytearray(padded)
    if tamper_backup:
        backup_table[100] ^= 1
    return primary, bytes(backup_table) + header(N - 1, 1, N - 33)


class PlanGptTest(unittest.TestCase):
    def test_plan_keeps_protected_entries_and_fixed_state_slot(self):
        primary, backup = stock()
        new_primary, new_backup, layout = plan.build(primary, backup)
        self.assertEqual((len(new_primary), len(new_backup)), (34 * 512, 33 * 512))
        self.assertEqual(new_primary[:512], primary[:512])
        by_name = {p['name']: p for p in layout['partitions']}
        self.assertEqual((by_name['NSHSTATE']['index'], by_name['NSHSTATE']['sectors']), (15, 264159))
        self.assertEqual((by_name['boot']['index'], by_name['boot']['sectors']), (12, 131072))
        self.assertEqual(by_name['rootfs']['index'], 14)
        for name in ('system', 'cache', 'recovery', 'userdata'):
            self.assertNotIn(name, by_name)
        old = {e['name']: e['raw'] for e in plan.read_stock(primary, backup)[1]}
        head = plan.parse_header(new_primary[512:1024])
        new = {e['name']: e['raw'] for e in plan.parse_entries(new_primary[1024:], head)}
        for name in plan.PROTECTED:
            self.assertEqual(new[name], old[name], name)
        self.assertLessEqual(by_name['rootfs']['first_sector'] + by_name['rootfs']['sectors'] - 1, N - 34)

    def test_tz_moves_but_keeps_its_identity(self):
        primary, backup = stock()
        new_primary = plan.build(primary, backup)[0]
        old = {e['name']: e for e in plan.read_stock(primary, backup)[1]}
        head = plan.parse_header(new_primary[512:1024])
        new = {e['name']: e for e in plan.parse_entries(new_primary[1024:], head)}
        self.assertEqual(new['tz']['raw'][:32], old['tz']['raw'][:32])
        self.assertEqual(new['tz']['raw'][48:], old['tz']['raw'][48:])
        self.assertEqual((new['tz']['first'], new['tz']['last']), (1513984, 1513984 + 1023))
        self.assertGreater(new['tz']['first'], 270827)
        self.assertEqual(new['tzbak']['raw'], old['tzbak']['raw'])

    def test_unstable_sector_inside_a_target_is_refused(self):
        layout = plan.build(*stock())[2]
        plan.check_unstable(layout, {270826, 270827, 1116554})
        for sector in (1116672, 1247744 + 5, 1513984, 7634903):
            with self.assertRaisesRegex(plan.PlanError, 'unstable'):
                plan.check_unstable(layout, {sector})

    def test_unstable_list_parser(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / 'm.txt'
            path.write_text('# header\n12\n34\n', encoding='utf-8')
            self.assertEqual(plan.read_unstable(path), {12, 34})
            path.write_text('12\nx\n', encoding='utf-8')
            with self.assertRaises(plan.PlanError):
                plan.read_unstable(path)

    def test_output_is_deterministic(self):
        primary, backup = stock()
        self.assertEqual(plan.build(primary, backup)[:2], plan.build(primary, backup)[:2])

    def test_refuses_mismatched_backup(self):
        with self.assertRaisesRegex(plan.PlanError, 'CRC|differ'):
            plan.build(*stock(tamper_backup=True))

    def test_refuses_corrupt_header(self):
        primary, backup = stock()
        primary = bytearray(primary)
        primary[512 + 40] ^= 1
        with self.assertRaisesRegex(plan.PlanError, 'header CRC'):
            plan.build(bytes(primary), backup)

    def test_refuses_unexpected_layout(self):
        rows = [row for row in STOCK if row[0] != 'recovery']
        with self.assertRaisesRegex(plan.PlanError, 'stock Android layout'):
            plan.build(*stock(rows))

    def test_refuses_moved_nv_partition(self):
        rows = [(n, 4790000, 4790000 + 65535) if n == 'persist' else (n, f, l) for n, f, l in STOCK]
        with self.assertRaisesRegex(plan.PlanError, 'protected persist|overlaps'):
            plan.build(*stock(rows))

    def test_refuses_short_entry_array(self):
        with self.assertRaisesRegex(plan.PlanError, 'stock Android layout|More partitions'):
            plan.build(*stock(STOCK[:25], count=25))

    def test_cli_refuses_existing_output(self):
        primary, backup = stock()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = pathlib.Path(tmp)
            (tmp / 'p').write_bytes(primary)
            (tmp / 'b').write_bytes(backup)
            (tmp / 'out').mkdir()
            sys.argv = ['plan_gpt.py', '--primary', str(tmp / 'p'), '--backup', str(tmp / 'b'),
                        '--out', str(tmp / 'out')]
            self.assertEqual(plan.main(), 1)
            sys.argv[-1] = str(tmp / 'fresh')
            self.assertEqual(plan.main(), 0)
            self.assertTrue((tmp / 'fresh' / 'plan.json').is_file())


if __name__ == '__main__':
    unittest.main()
