# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import zlib
from unittest.mock import patch

DIRECTORY=pathlib.Path(__file__).resolve().parents[1]/'recovery'
spec=importlib.util.spec_from_file_location('snapshot_state_edl',DIRECTORY/'edl_readonly.py')
recovery=importlib.util.module_from_spec(spec);spec.loader.exec_module(recovery)
with patch.dict(sys.modules,{'edl_readonly':recovery}):
    spec=importlib.util.spec_from_file_location('snapshot_state',DIRECTORY/'snapshot-app-state.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

KEY='b'*64
STATE_BYTES=4096*512
SECTORS=41+60+4096+40
PARTITIONS={12:('boot',7*512),14:('rootfs',60*512),15:('NSHSTATE',STATE_BYTES)}
LAYOUT={'boot':{'index':12,'first_sector':34,'bytes':7*512},
        'rootfs':{'index':14,'first_sector':41,'bytes':60*512},
        'NSHSTATE':{'index':15,'first_sector':101,'bytes':STATE_BYTES}}


def state(**changes):
    value={'format':1,'key_sha256':KEY,'floor':52,'confirmed':{'hash':'c'*64,'epoch':52},'candidate':None,'attempted':False}
    value.update(changes);return json.dumps(value).encode()


def image_data(nshstate):
    data=bytearray(SECTORS*512);entries=bytearray(16*128)
    for name,p in LAYOUT.items():
        offset=(p['index']-1)*128;entries[offset:offset+32]=bytes([p['index']])*32
        struct.pack_into('<2Q',entries,offset+32,p['first_sector'],p['first_sector']+p['bytes']//512-1)
        encoded=name.encode('utf-16-le');entries[offset+56:offset+56+len(encoded)]=encoded
    def header(current,peer,array):
        h=bytearray(512);h[:8]=b'EFI PART'
        struct.pack_into('<4I4Q16sQ3I',h,8,0x10000,92,0,0,current,peer,34,SECTORS-34,b'g'*16,array,16,128,zlib.crc32(entries))
        struct.pack_into('<I',h,16,zlib.crc32(h[:92]));return h
    data[512:1024]=header(1,SECTORS-1,2);data[1024:3072]=entries
    data[-33*512:-33*512+len(entries)]=entries;data[-512:]=header(SECTORS-1,1,SECTORS-33)
    start=LAYOUT['NSHSTATE']['first_sector']*512;data[start:start+STATE_BYTES]=nshstate
    return bytes(data)


class StateRuleTests(unittest.TestCase):
    def test_idle_store_at_expected_epoch_is_accepted(self):
        result=module.check_state(state(),KEY,52)
        self.assertEqual((result['confirmed_epoch'],result['floor'],result['candidate']),(52,52,None))

    def test_older_pending_foreign_or_malformed_snapshot_is_refused(self):
        cases=[state(confirmed={'hash':'c'*64,'epoch':50},floor=50),  # REC-01: image older than the device
               state(floor=53),
               state(candidate={'hash':'d'*64,'epoch':52},attempted=True),
               state(candidate={'hash':'d'*64,'epoch':52}),
               state(key_sha256='a'*64),
               state(confirmed=None),
               state(confirmed={'hash':'c'*64,'epoch':53}),
               state(confirmed={'hash':'C'*64,'epoch':52}),
               state(confirmed={'hash':'c'*64,'epoch':True}),
               state(attempted=True),
               state(format=2),
               state(extra=1),
               b'not json']
        for data in cases:
            with self.subTest(data=data[:80]),self.assertRaises((ValueError,json.JSONDecodeError)):
                module.check_state(data,KEY,52)


@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0 and shutil.which('mkfs.ext4') and shutil.which('debugfs'),
                     'Linux Root backup fixtures with e2fsprogs')
class SnapshotImageTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.base.chmod(0o700)
        self.patches=[patch.object(recovery,'DISK_BYTES',SECTORS*512),patch.object(recovery,'SECTORS',SECTORS),
                      patch.object(recovery,'PARTITIONS',PARTITIONS)]
        for p in self.patches:p.start()
    def tearDown(self):
        for p in reversed(self.patches):p.stop()
        self.tmp.cleanup()

    def build(self,content,as_directory=False):
        tree=self.base/'tree';app=tree/'rustsimhub-v1/system/app';app.mkdir(parents=True)
        (app/'state.json').mkdir() if as_directory else (app/'state.json').write_bytes(content)
        fs=self.base/'state.ext4';fs.write_bytes(bytes(STATE_BYTES))
        subprocess.run(['mkfs.ext4','-q','-F','-b','1024','-d',str(tree),str(fs)],check=True)
        data=image_data(fs.read_bytes());shutil.rmtree(tree);fs.unlink()
        self.image=self.base/'backup.img';self.image.write_bytes(data);self.image.chmod(0o600)
        self.sha=hashlib.sha256(data).hexdigest();self.gpt=hashlib.sha256(data[:34*512]+data[-33*512:]).hexdigest()
        self.offline=self.base/'offline.json';self.offline.write_text(json.dumps({'passed':True,'independent_readback_sha256':self.sha,
            'filesystems':{'rootfs':{'passed':True},'NSHSTATE':{'passed':True}}}));self.offline.chmod(0o600)
        return data

    def run_inspect(self,epoch=52,output='out'):
        return module.inspect(self.image,self.offline,self.base/output,self.sha,self.gpt,KEY,epoch)

    def test_reads_confirmed_epoch_without_changing_the_image_or_leaving_a_loop(self):
        before=self.build(state());loops=subprocess.check_output(['losetup','--list','--noheadings']).decode()
        report=self.run_inspect()
        self.assertTrue(report['passed']);self.assertEqual(report['application']['confirmed_epoch'],52)
        for key in ('partition_writes','image_write_requested','usb_opened','recovery_verified','flash_allowed'):
            self.assertIs(report[key],False)
        self.assertEqual(self.image.read_bytes(),before)
        for _ in range(20):  # the kernel releases a detached loop asynchronously
            if subprocess.check_output(['losetup','--list','--noheadings']).decode()==loops:break
            time.sleep(0.1)
        self.assertEqual(subprocess.check_output(['losetup','--list','--noheadings']).decode(),loops)
        self.assertTrue(json.loads((self.base/'out/report.json').read_text())['passed'])

    def test_stale_epoch_records_failure_and_output_is_never_reused(self):
        self.build(state(confirmed={'hash':'c'*64,'epoch':50},floor=50))
        with self.assertRaises(ValueError):self.run_inspect()
        saved=json.loads((self.base/'out/report.json').read_text())
        self.assertFalse(saved['passed']);self.assertIn('expected confirmed epoch',saved['error'])
        with self.assertRaises(ValueError):self.run_inspect()

    def test_missing_state_file_digest_mismatch_or_unverified_image_is_refused(self):
        self.build(b'',as_directory=True)
        with self.assertRaises(ValueError):self.run_inspect(output='dir')
        self.assertIn('regular file',json.loads((self.base/'dir/report.json').read_text())['error'])
        with self.assertRaises(ValueError):
            module.inspect(self.image,self.offline,self.base/'wrong',self.sha,'0'*64,KEY,52)
        self.assertIn('GPT',json.loads((self.base/'wrong/report.json').read_text())['error'])
        self.offline.write_text(json.dumps({'passed':False,'independent_readback_sha256':self.sha}))
        with self.assertRaises(ValueError):self.run_inspect(output='unverified')
        self.assertFalse((self.base/'unverified').exists())


if __name__=='__main__':
    unittest.main()
