# SPDX-License-Identifier: AGPL-3.0-only
import copy
import hashlib
import importlib.util
import pathlib
import sys
import unittest
from unittest.mock import patch

DIRECTORY=pathlib.Path(__file__).resolve().parents[1]/'recovery'
spec=importlib.util.spec_from_file_location('restore_wire_edl',DIRECTORY/'edl_readonly.py')
recovery=importlib.util.module_from_spec(spec);spec.loader.exec_module(recovery)
with patch.dict(sys.modules,{'edl_readonly':recovery}):
    spec=importlib.util.spec_from_file_location('restore_protocol',DIRECTORY/'restore_protocol.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


def xml(name,attributes):return '<data><'+name+' '+' '.join(k+'="'+str(v)+'"' for k,v in attributes.items())+'/></data>'
CONFIG={'MemoryName':'eMMC','Verbose':'0','AlwaysValidate':'0','MaxDigestTableSizeInBytes':'2048',
        'MaxPayloadSizeToTargetInBytes':'32768','ZLPAwareHost':'1','SkipStorageInit':'0','SkipWrite':'0'}


class Endpoint:
    def __init__(self,fail=None):self.calls=[];self.fail=fail
    def write(self,data,timeout):
        self.calls.append((bytes(data),timeout))
        if self.fail and len(self.calls)==self.fail[0]:
            value=self.fail[1]
            if isinstance(value,Exception):raise value
            return value
        return len(data)


class RestoreProtocolTests(unittest.TestCase):
    def setUp(self):
        self.patches=[patch.object(recovery,'DISK_BYTES',1024*512),
                      patch.object(recovery,'PARTITIONS',{14:('rootfs',512),15:('NSHSTATE',512)})]
        for p in self.patches:p.start()
        self.data=[b'A'*512,b'B'*512]
        self.targets=[{'index':i,'name':name,'offset':sector*512,'bytes':512,'start_sector':sector,
                       'sector_count':1,'physical_partition_number':0,'backup_sha256':hashlib.sha256(data).hexdigest()}
                      for i,name,sector,data in [(14,'rootfs',64,self.data[0]),(15,'NSHSTATE',65,self.data[1])]]
        self.protected=[{'offset':0,'bytes':64*512,'backup_sha256':'8'*64},
                        {'offset':66*512,'bytes':(1024-66)*512,'backup_sha256':'8'*64}]
        self.plan={'kind':'fixed-system-restore-plan','passed':True,'write_targets':self.targets,'protected_ranges':self.protected,
                   'gpt_sha256':'1'*64,'boot_sha256':recovery.BOOT_SHA256,'source_image_sha256':'3'*64}
        # Current NV bytes may legitimately differ from the backup snapshot.
        self.current=[{'offset':p['offset'],'bytes':p['bytes'],'sha256':'9'*64} for p in self.protected]
    def tearDown(self):
        for p in reversed(self.patches):p.stop()
    def policy(self):return module.RestorePolicy(self.plan,self.targets,self.protected)
    def arm(self,p):
        p.confirm_preflight(self.plan['gpt_sha256'],self.plan['boot_sha256'],self.plan['source_image_sha256'],self.current)
        p.validate_xml(xml('configure',CONFIG));p.acknowledge({'value':'ACK'})
    def program(self,index):
        p=self.targets[index]
        return xml('program',{'SECTOR_SIZE_IN_BYTES':'512','physical_partition_number':'0',
                              'start_sector':p['start_sector'],'num_partition_sectors':p['sector_count']})
    def payload_ready(self):
        p=self.policy();self.arm(p);p.validate_xml(self.program(0));p.acknowledge({'value':'ACK','rawmode':'true'});return p
    def programmed(self):
        p=self.policy();self.arm(p)
        for i in range(2):
            p.validate_xml(self.program(i));p.acknowledge({'value':'ACK','rawmode':'true'})
            p.send_payload_chunk(Endpoint(),self.data[i]);p.finish_payload();p.acknowledge({'value':'ACK','rawmode':'false'})
        return p
    def restored(self):return [{'offset':p['offset'],'bytes':p['bytes'],'sha256':p['backup_sha256']} for p in self.targets]

    def test_reset_requires_both_exact_program_acks_and_full_readback_and_preserves_current_nv(self):
        p=self.programmed();self.assertEqual(p.phase,'post-readback')
        p.confirm_readback(self.plan['gpt_sha256'],self.plan['boot_sha256'],self.restored(),self.current)
        self.assertEqual(p.phase,'verified');p.validate_xml(xml('power',{'value':'reset'}))
        p.acknowledge({'value':'ACK'});self.assertEqual(p.phase,'reset-acknowledged')
        with self.assertRaises(ValueError):p.validate_xml(xml('power',{'value':'reset'}))
        self.assertEqual(p.phase,'failed')

    def test_no_preflight_wrong_geometry_extra_attributes_or_repeated_command_can_program(self):
        for command in (self.program(0),xml('configure',CONFIG),xml('power',{'value':'reset'})):
            p=self.policy()
            with self.subTest(command=command),self.assertRaises(ValueError):p.validate_xml(command)
            self.assertEqual(p.phase,'failed')
        for command in (self.program(1),self.program(0).replace('start_sector="64"','start_sector="0"'),
                        self.program(0).replace('physical_partition_number="0"','physical_partition_number="1"'),
                        self.program(0).replace('num_partition_sectors="1"','num_partition_sectors="2"'),
                        self.program(0).replace('/>',' filename="other"/>'),
                        '<data><nop/><program/></data>','<!DOCTYPE data><data><nop/></data>'):
            p=self.policy();self.arm(p)
            with self.subTest(command=command),self.assertRaises(ValueError):p.validate_xml(command)
            self.assertEqual(p.phase,'failed')
        p=self.policy();self.arm(p);p.validate_xml(self.program(0))
        with self.assertRaises(ValueError):p.validate_xml(self.program(0))

    def test_truthy_nak_missing_or_wrong_mode_ack_never_advances(self):
        for value in (True,{}, {'value':'NAK'},{'value':'ACK','rawmode':'false'},{'value':'ACK'}):
            p=self.policy();self.arm(p);p.validate_xml(self.program(0))
            with self.subTest(value=value),self.assertRaises(ValueError):p.acknowledge(value)
            self.assertEqual(p.phase,'failed')
        p=self.payload_ready();p.send_payload_chunk(Endpoint(),self.data[0]);p.finish_payload()
        with self.assertRaises(ValueError):p.acknowledge({'value':'ACK'})

    def test_wrong_short_oversize_or_unaligned_payload_cannot_complete(self):
        for data in (b'',b'x',b'x'*1024,b'x'*(module.MAX_PAYLOAD+512)):
            p=self.payload_ready();endpoint=Endpoint()
            with self.subTest(length=len(data)),self.assertRaises(ValueError):p.send_payload_chunk(endpoint,data)
            self.assertEqual(endpoint.calls,[])
        p=self.payload_ready()
        with self.assertRaises(ValueError):p.finish_payload()
        p=self.payload_ready();p.send_payload_chunk(Endpoint(),b'Z'*512)
        with self.assertRaises(ValueError):p.finish_payload()

    def test_changed_protected_bytes_wrong_target_and_incomplete_readback_block_reset(self):
        for which in ('protected','restored','missing','gpt'):
            p=self.programmed();protected=copy.deepcopy(self.current);restored=self.restored();gpt=self.plan['gpt_sha256']
            if which=='protected':protected[0]['sha256']='a'*64
            if which=='restored':restored[1]['sha256']='b'*64
            if which=='missing':restored.pop()
            if which=='gpt':gpt='c'*64
            with self.subTest(which=which),self.assertRaises(ValueError):p.confirm_readback(gpt,self.plan['boot_sha256'],restored,protected)
            with self.assertRaises(ValueError):p.validate_xml(xml('power',{'value':'reset'}),True)

    def test_usb_short_exception_unknown_or_zlp_failure_is_terminal_without_retry(self):
        for failure in ((1,511),(1,0),(1,True),(1,None),(1,IOError('timeout')),(2,1),(2,IOError('timeout'))):
            p=self.payload_ready();endpoint=Endpoint(failure)
            with self.subTest(failure=failure),self.assertRaises((IOError,ValueError)):p.send_payload_chunk(endpoint,self.data[0])
            self.assertEqual(p.phase,'failed');self.assertEqual(len(endpoint.calls),failure[0])
            with self.assertRaises(ValueError):p.send_payload_chunk(endpoint,self.data[0])
            self.assertEqual(len(endpoint.calls),failure[0])
        endpoint=Endpoint();data=b'x'*(module.MAX_PAYLOAD*2+512)
        self.assertEqual(module.write_bulk_once(endpoint,data),len(data))
        self.assertEqual([len(c[0]) for c in endpoint.calls],[32768,32768,512])
        self.assertTrue(all(c[1]==5000 for c in endpoint.calls))

    def test_reconfiguration_and_false_preflight_proof_cannot_enable_write(self):
        p=self.policy()
        with self.assertRaises(ValueError):p.confirm_preflight('0'*64,self.plan['boot_sha256'],self.plan['source_image_sha256'],self.current)
        for changes in ({'SkipWrite':'1'},{'MemoryName':'ufs'},{'MaxPayloadSizeToTargetInBytes':'65536'},{'unexpected':'1'}):
            p=self.policy();p.confirm_preflight(self.plan['gpt_sha256'],self.plan['boot_sha256'],self.plan['source_image_sha256'],self.current)
            with self.subTest(changes=changes),self.assertRaises(ValueError):p.validate_xml(xml('configure',{**CONFIG,**changes}))
        p=self.policy();self.arm(p)
        with self.assertRaises(ValueError):p.validate_xml(xml('configure',CONFIG))

    def test_readonly_hook_keeps_original_validator_without_recursion(self):
        p=self.policy()
        read=xml('read',{'SECTOR_SIZE_IN_BYTES':'512','physical_partition_number':'0','start_sector':'0','num_partition_sectors':'34'})
        with patch.object(recovery,'validate_xml',p.validate_xml):p.validate_xml(read)
        self.assertEqual(p.phase,'preflight')

    def test_explicit_reply_parser_refuses_ambiguous_nested_entity_or_log_only_ack(self):
        good=b'<?xml version="1.0"?><data><log value="status"/></data><data><response value="ACK" rawmode="true"/></data>'
        self.assertEqual(module.decode_control_reply(good),{'value':'ACK','rawmode':'true'})
        self.assertIsNone(module.decode_control_reply(b'<data><log value="ACK"/></data>'))
        for raw in (b'<data><response value="ACK"/><response value="NAK"/></data>',
                    b'<data><response value="ACK"><log value="other"/></response></data>',
                    b'<!DOCTYPE data><data><response value="ACK"/></data>',
                    b'<data><response value="ACK" rawmode="TRUE"/></data>',
                    b'<other><response value="ACK"/></other>',
                    b'<data><response value="true"/></data>',b'garbage<data><response value="ACK"/></data>'):
            with self.subTest(raw=raw),self.assertRaises(ValueError):module.decode_control_reply(raw)

    def test_ack_reader_handles_fragmentation_and_bounded_timeout_without_resending(self):
        class Input:
            def __init__(self,parts):self.parts=iter(parts);self.calls=[]
            def read(self,size,timeout):
                self.calls.append((size,timeout))
                item=next(self.parts,TimeoutError('USB receive timeout'))
                if isinstance(item,Exception):raise item
                return item
        endpoint=Input([b'<data><log value="waiting"/></data>',b'<data><response value="ACK"',b' rawmode="false"/></data>'])
        self.assertEqual(module.read_control_reply(endpoint),{'value':'ACK','rawmode':'false'})
        self.assertEqual(endpoint.calls,[(4096,1000)]*3)
        times=iter([0,0,1,2,3,4]);endpoint=Input([TimeoutError('USB timeout')]*5)
        with self.assertRaisesRegex(TimeoutError,'do not resend'):
            module.read_control_reply(endpoint,timeout_seconds=3,clock=lambda:next(times))
        self.assertLessEqual(len(endpoint.calls),3)
        endpoint=Input([IOError('disconnect')])
        with self.assertRaises(IOError):module.read_control_reply(endpoint)
        self.assertEqual(len(endpoint.calls),1)
