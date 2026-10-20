# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import importlib.util
import json
import os
import pathlib
import sys
import tempfile
import types
import unittest
import xml.etree.ElementTree as ET
from unittest.mock import patch

from test_restore_plan import image_data,LAYOUT,PARTITIONS,SECTORS

DIRECTORY=pathlib.Path(__file__).resolve().parents[1]/'recovery'
def load(name,file):
    spec=importlib.util.spec_from_file_location(name,DIRECTORY/file)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
recovery=load('executor_edl','edl_readonly.py')
with patch.dict(sys.modules,{'edl_readonly':recovery}):
    protocol=load('executor_protocol','restore_protocol.py')
    with patch.dict(sys.modules,{'restore_protocol':protocol}):
        module=load('restore_executor','restore-system.py')


class MemoryWire:
    """Only a test model. Exercises the real endpoint writer/parser and transaction."""
    def __init__(self,data,policy,fault=None):
        self.data=bytearray(data);self.policy=policy;self.fault=fault;self.queue=[]
        self.commands=[];self.position=None;self.remaining=0;self.reads=0;self.resets=0;self.closed=[]
        self.input=types.SimpleNamespace(bEndpointAddress=0x81,bmAttributes=2,read=self.read)
        self.output=types.SimpleNamespace(bEndpointAddress=1,bmAttributes=2,write=self.write)
    def ack(self,mode=None,value='ACK'):
        attrs={'value':value}
        if mode is not None:attrs['rawmode']=mode
        self.queue.append(module.control_xml('response',attrs))
    def write(self,data,timeout):
        if self.position is not None:
            if not data:
                if not self.remaining:
                    self.position=None
                    if self.fault!='missing-final':self.ack('false')
                return 0
            count=len(data)-1 if self.fault=='short-payload' else len(data)
            self.data[self.position:self.position+count]=data[:count];self.position+=count;self.remaining-=count
            return count
        root=ET.fromstring(data);command=root[0];self.commands.append(command.tag)
        if command.tag=='configure':self.ack()
        elif command.tag=='program':
            if self.fault=='negative-program':self.ack('false','NAK')
            else:
                self.position=int(command.get('start_sector'))*512
                self.remaining=int(command.get('num_partition_sectors'))*512;self.ack('true')
        elif command.tag=='power':self.resets+=1;self.ack()
        else:raise AssertionError('Unexpected control command')
        return len(data)
    def read(self,size,timeout):
        if not self.queue:raise IOError('No ACK received')
        return self.queue.pop(0)
    def cmd_read(self,physical,start,sectors,path,display=False):
        self.reads+=1
        self.policy.validate_xml(module.control_xml('read',{'SECTOR_SIZE_IN_BYTES':'512',
             'physical_partition_number':'0','start_sector':str(start),'num_partition_sectors':str(sectors)}))
        if self.reads==2 and self.fault=='readback-failure':return False
        data=bytearray(self.data)
        if self.reads==2 and self.fault=='protected-corruption':data[200*512]^=1
        if self.reads==2 and self.fault=='target-corruption':data[41*512]^=1
        pathlib.Path(path).write_bytes(data)
        return True
    def client(self):
        cdc=types.SimpleNamespace(EP_IN=self.input,EP_OUT=self.output,close=lambda reset=False:self.closed.append(reset))
        return types.SimpleNamespace(cdc=cdc,fh=types.SimpleNamespace(firehose=types.SimpleNamespace(cmd_read=self.cmd_read)))


@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Linux Root isolated backup fixtures')
class RestoreExecutorTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.base=pathlib.Path(self.tmp.name);self.base.chmod(0o700)
        data=image_data();self.original=data;self.image=self.base/'backup.img';self.image.write_bytes(data);self.image.chmod(0o600)
        self.gpt=hashlib.sha256(data[:34*512]+data[-33*512:]).hexdigest()
        boot=hashlib.sha256(data[34*512:41*512]).hexdigest()
        self.patches=[patch.object(recovery,'DISK_BYTES',len(data)),patch.object(recovery,'SECTORS',SECTORS),
                      patch.object(recovery,'PARTITIONS',PARTITIONS),patch.object(recovery,'BOOT_SHA256',boot)]
        for p in self.patches:p.start()
        capture={'format':1,'action':'read-emmc','passed':True,'partition_writes':False,'bytes':len(data),
                 'current_boot_verified':True,'gpt_copies_checked':True,'sha256':hashlib.sha256(data).hexdigest(),
                 'gpt_sha256':self.gpt,'source_revision':recovery.SOURCE_REVISION,'loader_sha256':recovery.LOADER_SHA256,
                 'wrapper_sha256':recovery.digest(pathlib.Path(recovery.__file__))}
        offline={'format':1,'passed':True,'partition_writes':False,'image_write_requested':False,'bytes':len(data),
                 'image_unchanged_after_fs_checks':True,'gpt_copies_checked':True,'current_boot_verified':True,
                 'independent_readback_sha256':capture['sha256'],'filesystems':{name:{'read_only':True,'detached':True,
                  'passed':True,'e2fsck_exit':0,'offset':LAYOUT[name]['first_sector']*512,'bytes':LAYOUT[name]['bytes']}
                  for name in ('rootfs','NSHSTATE')}}
        self.capture=self.base/'capture.json';self.offline=self.base/'offline.json'
        self.write_json(self.capture,capture);self.write_json(self.offline,offline)
        module.planner.prepare(self.image,self.capture,self.offline,self.gpt,self.base/'plan')
        self.plan=self.base/'plan/plan.json';self.plan_hash=recovery.digest(self.plan)
        self.source=module.Source(self.plan,self.plan_hash,self.capture,self.offline)
        self.tools=self.base/'tools';self.tools.mkdir(mode=0o700)
        self.write_json(self.tools/'expected-device.json',{'usb_port':'2-1','gpt_sha256':self.gpt})
    def tearDown(self):
        self.source.close()
        for p in reversed(self.patches):p.stop()
        self.tmp.cleanup()
    def write_json(self,path,value):
        path.write_text(json.dumps(value));path.chmod(0o600)
    def test_source_loads_verified_preimage_plan_without_relabeling_failed_inspector(self):
        report={'format':1,'action':'inspect','passed':False,'usb_opened':True,
                'partition_writes_may_have_occurred':False,'normal_reset_attempted':True,
                'normal_reset_acknowledged':False,'executor_sha256':module.planner.PREIMAGE_INSPECTOR_SHA256,
                'phase':'failed-inspect-current-mode','error_type':'ValueError',
                'error':'Explicit ACK missing; no automatic retry or success inference',
                'current_before':{'sha256':self.source.plan['source_image_sha256'],'gpt_sha256':self.gpt,
                                  'boot_sha256':self.source.plan['boot_sha256'],
                                  'targets':self.source.targets,'protected':self.source.protected}}
        self.write_json(self.capture,report)
        directory=self.base/'preimage-plan'
        module.planner.prepare(self.image,self.capture,self.offline,self.gpt,directory)
        source=module.Source(directory/'plan.json',recovery.digest(directory/'plan.json'),self.capture,self.offline)
        try:
            self.assertEqual(source.targets,self.source.targets)
            self.assertFalse(source.plan['capture_execution_passed'])
            self.assertFalse(json.loads(self.capture.read_text())['passed'])
        finally:source.close()
    def test_insufficient_retained_readback_space_refuses_before_attempt_or_write_configuration(self):
        hardware,wire,before=self.hardware()
        output=self.base/'full-disk';output.mkdir(mode=0o700);report={}
        def save():module.save_json(output/'report.json',report)
        with patch.object(module.backup,'verify'),patch.object(module.shutil,'disk_usage',return_value=types.SimpleNamespace(free=0)):
            with self.assertRaisesRegex(ValueError,'Insufficient space'):
                module.transaction(self.source,hardware,self.tools,output,'restore',report,save)
        self.assertEqual(wire.commands,[])
        self.assertFalse((self.tools/'restore-attempts').exists())
        self.assertFalse(report.get('partition_writes_may_have_occurred',False))
        self.assertEqual(wire.resets,0)
        hardware.close()
    def test_control_envelope_matches_pinned_reset_and_negative_reply_is_preserved(self):
        self.assertEqual(module.control_xml('power',{'value':'reset'}),
                         b'<?xml version="1.0" ?><data><power value="reset"/></data>')
        hardware,wire,before=self.hardware('negative-program')
        output=self.base/'negative-reply';output.mkdir(mode=0o700);report={}
        def save():module.save_json(output/'report.json',report)
        with patch.object(module.backup,'verify'):
            with self.assertRaises(ValueError):
                module.transaction(self.source,hardware,self.tools,output,'restore',report,save)
        self.assertEqual(report['control_replies'][-1]['response']['value'],'NAK')
        self.assertEqual(report['control_replies'][-1]['operation'],'program')
        self.assertEqual(wire.resets,0)
        self.assertEqual(report['policy_phase'],'failed')
        hardware.close()
    def hardware(self,fault=None):
        current=bytearray(self.original)
        for name in ('rootfs','NSHSTATE'):
            p=LAYOUT[name];current[p['first_sector']*512:p['first_sector']*512+p['bytes']]=b'X'*p['bytes']
        current[200*512]=99  # current protected bytes differ legitimately from snapshot
        policy=protocol.RestorePolicy(self.source.plan,self.source.targets,self.source.protected)
        wire=MemoryWire(current,policy,fault)
        return module.Hardware(wire.client(),policy,lambda:None),wire,bytes(current)
    def run_transaction(self,action='restore',fault=None,check=None):
        hardware,wire,before=self.hardware(fault);output=self.base/'out';output.mkdir(mode=0o700)
        report={'passed':False,'partition_writes_may_have_occurred':False}
        def save():module.save_json(output/'report.json',report)
        # The synthetic media are not ext4. Filesystem checks have separate real
        # image evidence; here they are a controlled boundary/failure injection.
        with patch.object(module.backup,'verify',side_effect=check):
            try:module.transaction(self.source,hardware,self.tools,output,action,report,save)
            finally:hardware.close()
        return hardware,wire,before,output,report

    def test_real_endpoint_transaction_restores_changed_targets_retains_current_protected_bytes_and_captures_preimage(self):
        hardware,wire,before,output,report=self.run_transaction()
        self.assertEqual((output/'current-before.img').read_bytes(),before)
        expected=bytearray(before)
        for p in self.source.targets:expected[p['offset']:p['offset']+p['bytes']]=self.original[p['offset']:p['offset']+p['bytes']]
        self.assertEqual(bytes(wire.data),bytes(expected))
        self.assertEqual((output/'current-after.img').read_bytes(),bytes(expected))
        self.assertEqual(wire.commands,['configure','program','program','power'])
        self.assertTrue(report['passed']);self.assertTrue(report['partition_restore_readback_verified'])
        self.assertTrue(all(p['before_differs_from_source'] for p in report['programmed_targets']))
        self.assertEqual(wire.resets,1);self.assertEqual(wire.closed,[False])
        with self.assertRaises(FileExistsError):module.consume_once(self.tools,self.source,output,report['current_before'])

    def test_readonly_inspection_never_arms_programming_or_consumes_attempt(self):
        hardware,wire,before,output,report=self.run_transaction(action='inspect')
        self.assertEqual(bytes(wire.data),before);self.assertEqual(wire.commands,['power'])
        self.assertFalse((self.tools/'restore-attempts').exists())
        self.assertFalse(report['partition_writes_may_have_occurred']);self.assertTrue(report['passed'])

    def test_short_write_missing_ack_or_readback_failure_never_resets_or_retries(self):
        for fault in ('short-payload','negative-program','missing-final','readback-failure','protected-corruption','target-corruption'):
            with self.subTest(fault=fault):
                hardware,wire,before=self.hardware(fault);output=self.base/fault;output.mkdir(mode=0o700)
                # Independent tool roots give each model its own consumed attempt.
                tools=self.base/(fault+'-tools');tools.mkdir(mode=0o700);report={'passed':False}
                with patch.object(module.backup,'verify'):
                    with self.assertRaises((ValueError,IOError)):
                        module.transaction(self.source,hardware,tools,output,'restore',report,lambda:None)
                self.assertEqual(wire.resets,0);self.assertLessEqual(wire.commands.count('program'),2)
                self.assertEqual(hardware.policy.phase,'failed');self.assertFalse(report['passed'])
                self.assertTrue((tools/'restore-attempts'/self.source.plan['plan_id']).is_dir())
                hardware.close();self.assertEqual(wire.closed,[False])

    def test_preimage_check_failure_stops_before_attempt_consumption_or_write_configuration(self):
        hardware,wire,before=self.hardware();out=self.base/'failed-precheck';out.mkdir(mode=0o700);report={'passed':False}
        with patch.object(module.backup,'verify',side_effect=ValueError('dirty preimage')):
            with self.assertRaises(ValueError):module.transaction(self.source,hardware,self.tools,out,'restore',report,lambda:None)
        self.assertEqual(wire.commands,[]);self.assertEqual(wire.resets,0);self.assertEqual(bytes(wire.data),before)
        self.assertFalse((self.tools/'restore-attempts').exists());self.assertEqual(hardware.policy.phase,'failed')

    def test_source_or_evidence_changes_and_wrong_confirmation_stop_before_usb(self):
        bad='0'*64
        with self.assertRaisesRegex(ValueError,'selected digest'):module.Source(self.plan,bad,self.capture,self.offline)
        args=['restore-system','--action','restore','--tools',str(self.tools),'--plan',str(self.plan),'--plan-sha256',self.plan_hash,
              '--capture-report',str(self.capture),'--offline-report',str(self.offline),'--output',str(self.base/'bad-output')]
        with patch.object(sys,'argv',args),patch.object(module.Hardware,'connect',side_effect=AssertionError('USB forbidden')):
            with self.assertRaisesRegex(ValueError,'Explicit confirmation'):module.main()
        self.assertFalse((self.base/'bad-output').exists())
        self.offline.write_bytes(b'changed evidence')
        with self.assertRaises((ValueError,json.JSONDecodeError)):module.Source(self.plan,self.plan_hash,self.capture,self.offline)

    def test_default_validate_never_loads_usb_or_consumes_plan_and_existing_output_is_preserved(self):
        output=self.base/'validated'
        args=['restore-system','--tools',str(self.tools),'--plan',str(self.plan),'--plan-sha256',self.plan_hash,
              '--capture-report',str(self.capture),'--offline-report',str(self.offline),'--output',str(output)]
        with patch.object(sys,'argv',args),patch.object(recovery,'verify_tools'), \
                patch.object(module.Hardware,'connect',side_effect=AssertionError('USB forbidden')):
            module.main()
            report=json.loads((output/'report.json').read_text());self.assertTrue(report['passed']);self.assertFalse(report['usb_opened'])
            with self.assertRaisesRegex(ValueError,'New private output'):module.main()
            self.assertEqual(json.loads((output/'report.json').read_text()),report)
        self.assertFalse((self.tools/'restore-attempts').exists())

    def test_exclusive_lock_refuses_concurrent_operator_and_untrusted_lock_file(self):
        with module.tool_lock(self.tools):
            with self.assertRaises(BlockingIOError):
                with module.tool_lock(self.tools):self.fail('Concurrent lock accepted')
        (self.tools/'restore.lock').chmod(0o666)
        with self.assertRaises(ValueError):
            with module.tool_lock(self.tools):self.fail('Untrusted lock accepted')
