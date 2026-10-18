# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import importlib.util
import json
import os
import pathlib
import stat
import struct
import sys
import tempfile
import unittest
import zlib
from unittest.mock import patch

DIRECTORY=pathlib.Path(__file__).resolve().parents[1]/'recovery'
spec=importlib.util.spec_from_file_location('restore_plan_edl',DIRECTORY/'edl_readonly.py')
recovery=importlib.util.module_from_spec(spec);spec.loader.exec_module(recovery)
with patch.dict(sys.modules,{'edl_readonly':recovery}):
    spec=importlib.util.spec_from_file_location('restore_plan',DIRECTORY/'plan-system-restore.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

SECTORS=1024
PARTITIONS={12:('boot',7*512),14:('rootfs',60*512),15:('NSHSTATE',20*512)}
LAYOUT={'boot':{'index':12,'first_sector':34,'bytes':7*512},
        'rootfs':{'index':14,'first_sector':41,'bytes':60*512},
        'NSHSTATE':{'index':15,'first_sector':101,'bytes':20*512}}


def image_data():
    data=bytearray(SECTORS*512);entries=bytearray(16*128)
    for name,p in LAYOUT.items():
        offset=(p['index']-1)*128;entries[offset:offset+32]=bytes([p['index']])*32
        struct.pack_into('<2Q',entries,offset+32,p['first_sector'],p['first_sector']+p['bytes']//512-1)
        encoded=name.encode('utf-16-le');entries[offset+56:offset+56+len(encoded)]=encoded
        start=p['first_sector']*512;data[start:start+p['bytes']]=bytes([p['index']])*p['bytes']
    def header(current,peer,array):
        h=bytearray(512);h[:8]=b'EFI PART'
        struct.pack_into('<4I4Q16sQ3I',h,8,0x10000,92,0,0,current,peer,34,SECTORS-34,b'g'*16,array,16,128,zlib.crc32(entries))
        struct.pack_into('<I',h,16,zlib.crc32(h[:92]));return h
    data[512:1024]=header(1,SECTORS-1,2);data[1024:3072]=entries
    data[-33*512:-33*512+len(entries)]=entries;data[-512:]=header(SECTORS-1,1,SECTORS-33)
    return bytes(data)


@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Linux Root backup fixtures')
class RestorePlanTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.base.chmod(0o700)
        data=image_data();self.image=self.base/'backup.img';self.image.write_bytes(data);self.image.chmod(0o600)
        self.gpt=hashlib.sha256(data[:34*512]+data[-33*512:]).hexdigest()
        self.boot=hashlib.sha256(data[34*512:41*512]).hexdigest()
        self.patches=[patch.object(recovery,'DISK_BYTES',len(data)),patch.object(recovery,'SECTORS',SECTORS),
                      patch.object(recovery,'PARTITIONS',PARTITIONS),patch.object(recovery,'BOOT_SHA256',self.boot)]
        for p in self.patches:p.start()
        self.capture={'format':1,'action':'read-emmc','passed':True,'partition_writes':False,'bytes':len(data),
            'current_boot_verified':True,'gpt_copies_checked':True,'sha256':hashlib.sha256(data).hexdigest(),
            'gpt_sha256':self.gpt,'source_revision':recovery.SOURCE_REVISION,'loader_sha256':recovery.LOADER_SHA256,
            'wrapper_sha256':recovery.digest(pathlib.Path(recovery.__file__))}
        self.offline={'format':1,'passed':True,'partition_writes':False,'image_write_requested':False,'bytes':len(data),
            'image_unchanged_after_fs_checks':True,'gpt_copies_checked':True,'current_boot_verified':True,
            'independent_readback_sha256':self.capture['sha256'],'filesystems':{name:{'read_only':True,'detached':True,
             'passed':True,'e2fsck_exit':0,'offset':LAYOUT[name]['first_sector']*512,'bytes':LAYOUT[name]['bytes']}
             for name in ('rootfs','NSHSTATE')}}
        self.cp=self.base/'capture.json';self.op=self.base/'offline.json';self.write_reports()
    def tearDown(self):
        for p in reversed(self.patches):p.stop()
        self.tmp.cleanup()
    def write_reports(self):
        for path,data in ((self.cp,self.capture),(self.op,self.offline)):
            path.write_text(json.dumps(data));path.chmod(0o600)
    def prepare(self):return module.prepare(self.image,self.cp,self.op,self.gpt,self.base/'plan')

    def test_real_gpt_and_file_cover_every_byte_but_only_two_targets_are_writable(self):
        before=self.image.read_bytes();result=self.prepare()
        self.assertEqual([(p['index'],p['name']) for p in result['write_targets']],[(14,'rootfs'),(15,'NSHSTATE')])
        for p in result['write_targets']+result['protected_ranges']:
            self.assertEqual(p['backup_sha256'],hashlib.sha256(before[p['offset']:p['offset']+p['bytes']]).hexdigest())
        ranges=sorted(result['write_targets']+result['protected_ranges'],key=lambda p:p['offset'])
        self.assertEqual([p['offset'] for p in ranges],[0,41*512,101*512,121*512])
        self.assertEqual(sum(p['bytes'] for p in ranges),len(before))
        self.assertEqual(self.image.read_bytes(),before)
        self.assertEqual(stat.S_IMODE((self.base/'plan/plan.json').stat().st_mode),0o600)
        for key in ('usb_opened','partition_writes','execution_authorized','recovery_verified','flash_allowed','release_ready','board_boot_verified'):
            self.assertIs(result[key],False)
        self.assertEqual(result['protected_range_policy'],'retain-current-device-bytes-and-verify-before-after')

    def test_dirty_missing_wrong_range_or_wrong_provenance_report_creates_no_plan(self):
        original=json.loads(json.dumps(self.offline))
        changes=[lambda: self.offline['filesystems']['rootfs'].update(e2fsck_exit=4),
                 lambda: self.offline['filesystems']['NSHSTATE'].update(e2fsck_exit=False),
                 lambda: self.offline['filesystems']['NSHSTATE'].update(offset=0),
                 lambda: self.offline['filesystems']['rootfs'].update(detached=False),
                 lambda: self.offline['filesystems'].pop('NSHSTATE'),
                 lambda: self.offline.update(independent_readback_sha256='0'*64),
                 lambda: self.offline.update(passed='true')]
        for change in changes:
            self.offline=json.loads(json.dumps(original));change();self.write_reports()
            with self.subTest(change=change),patch.object(module,'hash_coverage',side_effect=AssertionError('No planning from failed evidence')):
                with self.assertRaises(ValueError):self.prepare()
                self.assertFalse((self.base/'plan').exists())
        self.offline=original;self.capture['loader_sha256']='0'*64;self.write_reports()
        with self.assertRaisesRegex(ValueError,'provenance'):self.prepare()

    def test_valid_full_digest_cannot_hide_a_different_boot_and_old_digest_cannot_hide_changed_rootfs(self):
        data=bytearray(self.image.read_bytes());data[41*512]^=1;self.image.write_bytes(data)
        with self.assertRaisesRegex(ValueError,'differs'):self.prepare()
        data[34*512]^=1;self.image.write_bytes(data)
        self.capture['sha256']=hashlib.sha256(data).hexdigest();self.offline['independent_readback_sha256']=self.capture['sha256'];self.write_reports()
        with self.assertRaisesRegex(ValueError,'boot mismatch'):self.prepare()
        self.assertFalse((self.base/'plan').exists())

    def test_linked_reports_existing_output_and_path_replacement_are_preserved_or_rejected(self):
        os.link(self.cp,self.base/'linked-report')
        with self.assertRaisesRegex(ValueError,'metadata'):self.prepare()
        (self.base/'linked-report').unlink()
        original=module.hash_coverage
        def replace(descriptor,targets,protected):
            digest=original(descriptor,targets,protected)
            data=self.image.read_bytes();self.image.rename(self.base/'original');self.image.write_bytes(data);self.image.chmod(0o600)
            return digest
        with patch.object(module,'hash_coverage',side_effect=replace):
            with self.assertRaisesRegex(ValueError,'metadata'):self.prepare()
        out=self.base/'plan';out.mkdir();(out/'evidence').write_bytes(b'preserve')
        with self.assertRaisesRegex(ValueError,'preserve'):self.prepare()
        self.assertEqual((out/'evidence').read_bytes(),b'preserve')

    def test_geometry_rejects_protected_boot_overlap_wrong_slot_and_ranges_crossing_gpt(self):
        for name,changes in [('rootfs',{'index':12}),('rootfs',{'first_sector':34}),
                             ('NSHSTATE',{'first_sector':70}),('NSHSTATE',{'first_sector':1000}),
                             ('rootfs',{'bytes':True}),('rootfs',{'first_sector':1})]:
            layout=json.loads(json.dumps(LAYOUT));layout[name].update(changes)
            with self.subTest(changes=changes),self.assertRaises(ValueError):module.geometry(layout)
