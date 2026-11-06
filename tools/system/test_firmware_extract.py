# SPDX-License-Identifier: AGPL-3.0-only
"""Run the real extractor on synthetic FAT/ext4 images; no device or vendor file is used."""
import hashlib
import os
import pathlib
import shutil
import struct
import subprocess
import tempfile
import unittest

OVERLAY = pathlib.Path(__file__).resolve().parents[2]/'firmware/system/overlay/usr/local/libexec'
SCRIPT = OVERLAY/'rustsimhub-firmware-extract'
REQUEST = 'RustSimHub.GenericProvision.v1\n'
HASH_FLAG = 2 << 24


def ehdr(phnum):
    return b'\x7fELF\x01\x01\x01' + bytes(9) + struct.pack('<HHIIIIIHHHHHH', 2, 40, 1, 0, 52, 0, 0, 52, 32, phnum, 40, 0, 0)


def phdr(offset, size, flags):
    return struct.pack('<IIIIIIII', 1, offset, 0, 0, size, size, flags, 4)


def split_image(seed):
    """NAME.mdt plus NAME.bNN: segment 1 is the hash table, segment 3 is empty."""
    body = [seed*97, b'', seed*4099]
    phnum = len(body)+2
    sizes = [52+32*phnum, 40+32*phnum]+[len(data) for data in body]
    flags = [5, HASH_FLAG, 5, 5, 5]
    offset = 4096
    rows = []
    for size, flag in zip(sizes, flags):
        rows.append(phdr(offset, size, flag))
        offset += size
    headers = ehdr(phnum)+b''.join(rows)
    digests = [hashlib.sha256(headers).digest(), bytes(32)]
    digests += [hashlib.sha256(data).digest() if data else bytes(32) for data in body]
    files = {'b00': headers, 'b01': bytes(40)+b''.join(digests)}
    for index, data in enumerate(body, 2):
        if data:
            files['b%02d' % index] = data
    return headers, files


def single_image(seed):
    code = seed*2048
    phnum = 3
    header_len = 52+32*phnum
    table_len = 40+32*phnum
    table_at = 4096
    code_at = table_at+table_len
    head = ehdr(phnum)+phdr(0, header_len, 5)+phdr(table_at, table_len, HASH_FLAG)+phdr(code_at, len(code), 5)
    table = bytes(40)+hashlib.sha256(head).digest()+bytes(32)+hashlib.sha256(code).digest()
    return head.ljust(table_at, b'\0')+table+code


def fat16(files, sectors=32768):
    """Minimal FAT16 volume with image/<8.3 file>; keeps the test free of dosfstools."""
    cluster = 1024
    fat_sectors, root_entries = 72, 512
    data_start = 1+2*fat_sectors+root_entries*32//512
    boot = bytearray(512)
    boot[0:3] = b'\xeb\x3c\x90'
    boot[3:11] = b'RUSTTEST'
    struct.pack_into('<HBHBHHBHHHII', boot, 11, 512, cluster//512, 1, 2, root_entries, sectors, 0xf8, fat_sectors, 32, 64, 0, 0)
    boot[38] = 0x29
    boot[43:54] = b'NO NAME    '
    boot[54:62] = b'FAT16   '
    boot[510:512] = b'\x55\xaa'
    volume = bytearray(sectors*512)
    volume[:512] = boot
    fat = [0xfff8, 0xffff]

    def store(data):
        count = max(1, (len(data)+cluster-1)//cluster)
        first = len(fat)
        fat.extend(list(range(first+1, first+count))+[0xffff])
        offset = data_start*512+(first-2)*cluster
        volume[offset:offset+len(data)] = data
        return first

    def entry(name, attr, first, size):
        stem, _, ext = name.upper().partition('.')
        return stem.ljust(8).encode()+ext.ljust(3).encode()+bytes([attr])+bytes(14)+struct.pack('<HI', first, size)

    directory = len(fat)
    rows = b'.          \x10'+bytes(14)+struct.pack('<HI', directory, 0)
    rows += b'..         \x10'+bytes(14)+struct.pack('<HI', 0, 0)
    # The directory takes the first clusters; file data follows it.
    reserved = (32*(len(files)+2)+cluster-1)//cluster
    fat.extend(list(range(directory+1, directory+reserved))+[0xffff])
    for name, data in files.items():
        rows += entry(name, 0x20, store(data), len(data))
    offset = data_start*512+(directory-2)*cluster
    volume[offset:offset+len(rows)] = rows
    root = 512*(1+2*fat_sectors)
    volume[root:root+32] = entry('image', 0x10, directory, 0)
    table = b''.join(struct.pack('<H', value) for value in fat)
    for copy in range(2):
        start = 512*(1+copy*fat_sectors)
        volume[start:start+len(table)] = table
    return bytes(volume)


@unittest.skipUnless(os.geteuid() == 0 and shutil.which('debugfs'), 'needs root, loop devices and debugfs')
class FirmwareExtractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with tempfile.TemporaryDirectory() as temporary:
            image = pathlib.Path(temporary)/'fat'
            image.write_bytes(fat16({'probe.bin': b'x'}))
            result = subprocess.run(['mount', '-t', 'vfat', '-o', 'loop,ro', str(image), temporary],
                                    capture_output=True, text=True)
            if result.returncode:
                raise unittest.SkipTest('kernel cannot mount vfat loop images here: '+result.stderr.strip())
            subprocess.run(['umount', temporary], check=True)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.tmp.name)
        self.state = self.root/'state'
        self.system = self.state/'rustsimhub-v1/system'
        self.system.mkdir(parents=True, mode=0o700)
        (self.system/'generic-provision.request').write_text(REQUEST)
        (self.system/'generic-provision.request').chmod(0o600)
        self.sys = self.root/'sys'
        self.dev = self.root/'dev'
        self.dev.mkdir()
        self.modem = self.dev/'mmcblk0p1'
        self.persist = self.dev/'mmcblk0p24'
        self.sysfs('mmcblk0p1', 'modem')
        self.sysfs('mmcblk0p24', 'persist')
        self.sysfs('mmcblk0p2', 'modemst1')
        self.files = {}
        for prefix, seed in (('modem', b'M'), ('wcnss', b'W')):
            headers, parts = split_image(seed)
            self.files[prefix+'.mdt'] = headers
            for suffix, data in parts.items():
                self.files[prefix+'.'+suffix] = data
        self.files['mba.mbn'] = single_image(b'B')
        self.files['widevine.mdt'] = b'not ours'
        self.make_modem()
        self.make_persist()
        text = SCRIPT.read_text()
        for old, new in (('/var/lib/rustsimhub', str(self.state)), ('/sys/class/block', str(self.sys)),
                         ('"/dev/$found"', '"%s/$found"' % self.dev), ('/usr/local/libexec/rustsimhub-state-check', 'true')):
            self.assertIn(old, text)
            text = text.replace(old, new)
        self.script = self.root/'extract'
        self.script.write_text(text)

    def tearDown(self):
        self.tmp.cleanup()

    def sysfs(self, name, label):
        row = self.sys/name
        row.mkdir(parents=True)
        (row/'uevent').write_text('MAJOR=179\nPARTNAME=%s\n' % label)

    def make_modem(self):
        self.modem.write_bytes(fat16(self.files))

    def make_persist(self, name='WCNSS_qcom_wlan_nv.bin', data=b'own-nv'*100):
        self.persist.unlink(missing_ok=True)
        stage = self.root/'pstage'
        shutil.rmtree(stage, ignore_errors=True)
        stage.mkdir()
        (stage/name).write_bytes(data)
        with self.persist.open('xb') as stream:
            stream.truncate(8*1024*1024)
        subprocess.run(['mke2fs', '-q', '-t', 'ext4', '-F', '-d', str(stage), str(self.persist)], check=True)

    def run_it(self):
        return subprocess.run(['sh', str(self.script)], capture_output=True, text=True)

    def digests(self):
        return {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in (self.modem, self.persist)}

    def assert_refused(self):
        self.assertNotEqual(self.run_it().returncode, 0)
        self.assertFalse((self.state/'.private-firmware').exists())
        self.assertFalse((self.state/'.private-firmware.pending').exists())
        self.assertEqual(subprocess.check_output(['losetup', '-j', str(self.modem)]), b'')

    def test_extract_publishes_only_allowed_files_and_never_changes_sources(self):
        before = self.digests()
        result = self.run_it()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.digests(), before)
        base = self.state/'.private-firmware'
        names = sorted(str(p.relative_to(base)) for p in base.rglob('*') if p.is_file())
        expected = sorted(n for n in self.files if n != 'widevine.mdt')+['SHA256SUMS', 'wlan/prima/WCNSS_qcom_wlan_nv.bin']
        self.assertEqual(names, sorted(expected))
        self.assertEqual((base/'wlan/prima/WCNSS_qcom_wlan_nv.bin').read_bytes(), b'own-nv'*100)
        for path in base.rglob('*'):
            self.assertEqual(path.stat().st_mode & 0o777, 0o700 if path.is_dir() else 0o600)
        subprocess.run(['sha256sum', '--status', '-c', 'SHA256SUMS'], cwd=base, check=True)
        self.assertEqual(subprocess.check_output(['losetup', '-j', str(self.modem)]), b'')

    def test_existing_bundle_is_never_replaced(self):
        (self.state/'.private-firmware').mkdir(mode=0o700)
        (self.state/'.private-firmware/mark').write_bytes(b'owner')
        self.assertEqual(self.run_it().returncode, 0)
        self.assertEqual((self.state/'.private-firmware/mark').read_bytes(), b'owner')

    def test_missing_or_wrong_request_changes_nothing(self):
        (self.system/'generic-provision.request').write_text('other\n')
        self.assert_refused()
        (self.system/'generic-provision.request').unlink()
        self.assert_refused()

    def test_corrupt_segment_fails_closed(self):
        self.files['modem.b02'] = b'Z'*97
        self.make_modem()
        self.assert_refused()

    def test_wrong_segment_length_or_missing_segment_fails_closed(self):
        self.files['wcnss.b04'] = self.files['wcnss.b04']+b'x'
        self.make_modem()
        self.assert_refused()
        self.files['wcnss.b04'] = self.files['wcnss.b04'][:-1]
        del self.files['wcnss.b02']
        self.make_modem()
        self.assert_refused()

    def test_corrupt_single_file_firmware_fails_closed(self):
        data = bytearray(self.files['mba.mbn'])
        data[-1] ^= 1
        self.files['mba.mbn'] = bytes(data)
        self.make_modem()
        self.assert_refused()

    def test_missing_nv_fails_closed(self):
        self.make_persist(name='other.bin')
        self.assert_refused()

    def test_duplicate_partition_name_is_refused(self):
        self.sysfs('mmcblk0p9', 'modem')
        self.assert_refused()

    def test_only_modem_and_persist_are_opened(self):
        text = SCRIPT.read_text()
        for name in ('modemst1', 'modemst2', 'fsg', 'fsc', 'local-volte', 'mke2fs', 'dd '):
            self.assertNotIn(name, text)
        self.assertNotIn('debugfs -w', text)
        self.assertIn('--read-only', text)

    def test_interrupted_pending_is_redone(self):
        pending = self.state/'.private-firmware.pending'
        pending.mkdir(mode=0o700)
        (pending/'stale').write_bytes(b'stale')
        result = self.run_it()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(pending.exists())
        self.assertFalse((self.state/'.private-firmware/stale').exists())


if __name__ == '__main__':
    unittest.main()
