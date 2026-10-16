# SPDX-License-Identifier: AGPL-3.0-only
"""Failure checks for the read-only recovery boundary; never opens USB."""
import pathlib
import struct
import tempfile
import unittest
import zlib

import edl_readonly as recovery


def xml(name, **attributes):
    return '<data><' + name + ' ' + ' '.join(f'{k}="{v}"' for k, v in attributes.items()) + '/></data>'


def gpt(overlap=False):
    sectors = 1024
    entries = bytearray(16 * 128)
    for index, (name, first, last) in enumerate([('boot', 34, 40), ('rootfs', 40 if overlap else 41, 100)]):
        offset = index * 128
        entries[offset:offset + 32] = bytes([index + 1]) * 32
        struct.pack_into('<2Q', entries, offset + 32, first, last)
        encoded = name.encode('utf-16-le')
        entries[offset + 56:offset + 56 + len(encoded)] = encoded
    crc = zlib.crc32(entries)

    def header(current, peer, array):
        data = bytearray(512)
        data[:8] = b'EFI PART'
        struct.pack_into('<4I4Q16sQ3I', data, 8, 0x10000, 92, 0, 0,
                         current, peer, 34, sectors - 34, b'g' * 16, array, 16, 128, crc)
        struct.pack_into('<I', data, 16, zlib.crc32(data[:92]))
        return data
    reserved = entries + bytes(16384 - len(entries))
    return bytes(512) + header(1, sectors - 1, 2) + reserved, reserved + header(sectors - 1, 1, sectors - 33)


class RecoveryTests(unittest.TestCase):
    def test_read_bounds_and_physical_storage_are_fixed(self):
        valid = dict(SECTOR_SIZE_IN_BYTES='512', physical_partition_number='0', start_sector='0', num_partition_sectors='34')
        recovery.validate_xml(xml('read', **valid))
        for changes in ({'physical_partition_number': '1'}, {'start_sector': '-1'}, {'num_partition_sectors': '0'},
                        {'start_sector': str(recovery.SECTORS - 1), 'num_partition_sectors': '2'}, {'SECTOR_SIZE_IN_BYTES': '4096'},
                        {'filename': 'unexpected'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                recovery.validate_xml(xml('read', **(valid | changes)))

    def test_mutating_xml_is_refused_including_multiple_commands_and_entities(self):
        for command in ('program', 'erase', 'patch', 'poke', 'setactiveslot', 'setbootablestoragedrive', 'writeIMEI'):
            with self.subTest(command=command), self.assertRaises(ValueError):
                recovery.validate_xml(xml(command))
        for data in ('<data><nop/><program/></data>', '<!DOCTYPE data><data><nop/></data>', '<data><nop><program/></nop></data>'):
            with self.subTest(data=data), self.assertRaises(ValueError):
                recovery.validate_xml(data)

    def test_configure_cannot_enable_writes_or_change_storage(self):
        valid = dict(MemoryName='eMMC', SkipWrite='1', MaxPayloadSizeToTargetInBytes='32768')
        recovery.validate_xml(xml('configure', **valid))
        for changes in ({'SkipWrite': '0'}, {'MemoryName': 'ufs'}, {'MaxPayloadSizeToTargetInBytes': '65536'}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                recovery.validate_xml(xml('configure', **(valid | changes)))

    def test_reset_is_explicit_normal_reset_only(self):
        recovery.validate_xml(xml('power', value='reset'), allow_reset=True)
        for data, allowed in ((xml('power', value='reset'), False), (xml('power', value='edl'), True),
                              (xml('power', value='off'), True), (xml('power', value='reset', extra='1'), True)):
            with self.subTest(data=data), self.assertRaises(ValueError):
                recovery.validate_xml(data, allowed)

    def test_gpt_both_crcs_and_layout_must_match(self):
        primary, backup = gpt()
        specs = {1: ('boot', 7 * 512), 2: ('rootfs', 60 * 512)}
        layout = recovery.validate_gpt(primary, backup, 1024, specs)
        self.assertEqual(layout['rootfs']['first_sector'], 41)
        for a, b in ((primary[:-1], backup), (primary[:528] + b'x' + primary[529:], backup),
                     (primary, b'x' + backup[1:]), (primary[:1040] + b'x' + primary[1041:], backup)):
            with self.subTest(lengths=(len(a), len(b))), self.assertRaises(ValueError):
                recovery.validate_gpt(a, b, 1024, specs)
        with self.assertRaises(ValueError):
            recovery.validate_gpt(primary, backup, 1024, {3: ('boot', 7 * 512)})

    def test_valid_crcs_do_not_allow_overlapping_partitions(self):
        with self.assertRaises(ValueError):
            recovery.validate_gpt(*gpt(overlap=True), sectors=1024, partitions={})

    def test_usb_must_be_sole_9008_at_expected_physical_port(self):
        with tempfile.TemporaryDirectory() as folder:
            root = pathlib.Path(folder)

            def device(name):
                path = root / name
                path.mkdir()
                for key, value in {'idVendor': '05c6', 'idProduct': '9008', 'busnum': '2', 'devnum': '43'}.items():
                    (path / key).write_text(value)
            device('2-3')
            self.assertEqual(recovery.select_usb('2-3', root), (2, 43))
            with self.assertRaises(ValueError):
                recovery.select_usb('2-4', root)
            device('2-5')
            with self.assertRaises(ValueError):
                recovery.select_usb('2-3', root)


if __name__ == '__main__':
    unittest.main()
