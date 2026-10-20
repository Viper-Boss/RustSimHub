#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed UFI003 restore: default local validation, explicit inspection/restore only."""
import argparse
import contextlib
import copy
import datetime
import fcntl
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import signal
import shutil
import stat
import time
import xml.etree.ElementTree as ET

import edl_readonly as recovery
import restore_protocol as protocol

spec = importlib.util.spec_from_file_location('restore_planner', pathlib.Path(__file__).with_name('plan-system-restore.py'))
planner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(planner)
backup = planner.backup
CONFIG_NAK_EXECUTOR_SHA256 = 'b51686a84d09e185b9b82b79fd5b2d3394de3727a797e75d052f887a673a2bae'
CONFIG_32K_XML_SHA256 = 'c0d85faf5377ac4ccc959532c23838bf4febda34e4ab17050552f2de378525b3'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def require_capture_space(directory, copies):
    required = copies * recovery.DISK_BYTES + 256 * 1024 * 1024
    available = shutil.disk_usage(directory).free
    require(available >= required, 'Insufficient space for retained full captures; no programming is permitted')
    return {'required_available_bytes':required,'available_bytes':available}


def save_json(path, value):
    pending = path.with_name(path.name + '.next')
    fd = os.open(pending, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    pending.replace(path)
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def inspect_image(path, targets, protected):
    fd, before = backup.open_image(path)
    try:
        primary = backup.read_at(fd, 0, 34 * 512)
        tail = backup.read_at(fd, recovery.DISK_BYTES - 33 * 512, 33 * 512)
        layout = recovery.validate_gpt(primary, tail, sectors=recovery.SECTORS, partitions=recovery.PARTITIONS)
        gpt = hashlib.sha256(primary + tail).hexdigest()
        boot = layout['boot']
        boot_hash = hashlib.sha256(backup.read_at(fd, boot['first_sector'] * 512, boot['bytes'])).hexdigest()
        actual_targets, actual_protected = planner.geometry(layout)
        for actual, expected in zip(actual_targets, targets):
            require(actual == {k:v for k,v in expected.items() if k != 'backup_sha256'}, 'Device target geometry mismatch')
        require(actual_protected == [{k:v for k,v in p.items() if k != 'backup_sha256'} for p in protected],
                'Device protected geometry mismatch')
        image_hash = planner.hash_coverage(fd, actual_targets, actual_protected)
        require(backup.signature(os.fstat(fd)) == before == backup.signature(path.lstat()), 'Captured file changed during inspection')
        return {'sha256': image_hash, 'gpt_sha256': gpt, 'boot_sha256': boot_hash,
                'targets': actual_targets, 'protected': actual_protected}
    finally:
        os.close(fd)


def range_results(rows):
    return [{'offset':r['offset'], 'bytes':r['bytes'], 'sha256':r['backup_sha256']} for r in rows]


class Source:
    def __init__(self, plan_path, expected_hash, capture_path, offline_path):
        self.fd = None
        self.plan, self.plan_hash = planner.private_json(plan_path)
        require(protocol.sha(expected_hash) and self.plan_hash == expected_hash, 'Plan file differs from the explicitly selected digest')
        plan = self.plan
        require(type(plan.get('format')) is int and plan['format'] == 1
                and plan.get('kind') == 'fixed-system-restore-plan' and plan.get('passed') is True
                and plan.get('board') == 'ufi003' and plan.get('sector_bytes') == 512
                and plan.get('disk_bytes') == recovery.DISK_BYTES
                and re.fullmatch('[0-9a-f]{32}', plan.get('plan_id','')) is not None,
                'Unsupported fixed restore plan')
        for flag in ('usb_opened','partition_writes','execution_authorized','recovery_verified','flash_allowed','release_ready','board_boot_verified'):
            require(plan.get(flag) is False, 'Plan is not the original non-executable readiness artifact')
        require(plan.get('source_image_unchanged') is True
                and plan.get('protected_range_policy') == 'retain-current-device-bytes-and-verify-before-after'
                and plan.get('backup_protected_hashes_are_snapshot_only') is True,
                'Unsupported protected-range policy')
        require(plan.get('planner_sha256') == recovery.digest(pathlib.Path(planner.__file__).resolve())
                and plan.get('edl_wrapper_sha256') == recovery.digest(pathlib.Path(recovery.__file__).resolve()),
                'Plan generator/runtime source mismatch')
        capture, capture_hash = planner.private_json(capture_path)
        offline, offline_hash = planner.private_json(offline_path)
        require(capture_hash == plan.get('capture_report_sha256') and offline_hash == plan.get('offline_report_sha256'),
                'Plan evidence files differ from the reviewed source')
        require(isinstance(plan.get('source_image'),str), 'Missing frozen backup path')
        self.path = pathlib.Path(plan['source_image'])
        self.fd, self.signature = backup.open_image(self.path)
        try:
            primary = backup.read_at(self.fd, 0, 34*512)
            tail = backup.read_at(self.fd, recovery.DISK_BYTES-33*512, 33*512)
            require(hashlib.sha256(primary+tail).hexdigest() == plan['gpt_sha256'], 'Source GPT mismatch')
            layout = recovery.validate_gpt(primary, tail, sectors=recovery.SECTORS, partitions=recovery.PARTITIONS)
            require(planner.check_evidence(capture, offline, plan['gpt_sha256'], layout) == plan['source_image_sha256'],
                    'Plan source differs from its independent evidence')
            self.targets, self.protected = planner.geometry(layout)
            require(planner.hash_coverage(self.fd, self.targets, self.protected) == plan['source_image_sha256'],
                    'Source image digest mismatch')
            require(self.targets == plan['write_targets'] and self.protected == plan['protected_ranges'],
                    'Source target/protected digests differ from plan')
            if capture['action'] == 'inspect':
                require(self.targets == capture['current_before']['targets']
                        and self.protected == capture['current_before']['protected'],
                        'Source ranges differ from inspection preimage evidence')
            boot = layout['boot']
            require(hashlib.sha256(backup.read_at(self.fd,boot['first_sector']*512,boot['bytes'])).hexdigest()
                    == plan['boot_sha256'] == recovery.BOOT_SHA256, 'Frozen source boot mismatch')
            self.ensure_frozen()
        except BaseException:
            self.close()
            raise

    def ensure_frozen(self):
        require(backup.signature(os.fstat(self.fd)) == self.signature == backup.signature(self.path.lstat()),
                'Frozen source file/path metadata changed')
        require(backup.digest_fd(self.fd) == self.plan['source_image_sha256'], 'Frozen source bytes changed')
        require(backup.signature(os.fstat(self.fd)) == self.signature == backup.signature(self.path.lstat()),
                'Frozen source changed while rechecking')

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


@contextlib.contextmanager
def tool_lock(tools):
    recovery.private_directory(tools,0)
    fd = os.open(tools/'restore.lock', os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
    try:
        info=os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == info.st_gid == 0
                and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1, 'Invalid exclusive runtime lock')
        fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        yield
    finally:
        os.close(fd)


def validate_config_retry(tools, source, path, expected_hash):
    """One explicitly selected continuation of the audited no-program failure.

    Never remove the original attempt or accept a payload/unknown failure.
    This is readiness validation, not operator authorization or an auto-retry.
    """
    failed, digest = planner.private_json(path)
    require(protocol.sha(expected_hash) and digest == expected_hash, 'Selected failed report digest mismatch')
    require(type(failed.get('format')) is int and failed['format'] == 1 and failed.get('action') == 'restore'
            and failed.get('passed') is False and failed.get('phase') == 'failed-inspect-current-mode'
            and failed.get('executor_sha256') == CONFIG_NAK_EXECUTOR_SHA256
            and failed.get('error_type') == 'ValueError'
            and failed.get('error') == 'Explicit ACK missing; no automatic retry or success inference',
            'Only the audited configuration refusal can be continued')
    require(failed.get('usb_opened') is True and failed.get('write_configuration_attempted') is True
            and failed.get('partition_writes_may_have_occurred') is False
            and failed.get('normal_reset_attempted') is False
            and failed.get('normal_reset_acknowledged') is False
            and all(key not in failed for key in ('programmed_targets','current_target','raw_payload_transport_bytes')),
            'A program, payload or uncertain failure cannot be retried')
    require(failed.get('plan_sha256') == source.plan_hash and failed.get('plan_id') == source.plan['plan_id']
            and failed.get('source_image_sha256') == source.plan['source_image_sha256'],
            'Continuation must retain the exact original source and plan')
    replies = failed.get('control_replies')
    require(isinstance(replies,list) and len(replies) == 1 and replies[0].get('operation') == 'configure'
            and replies[0].get('xml_sha256') == CONFIG_32K_XML_SHA256, 'Exactly the known rejected configuration required')
    response = replies[0].get('response',{})
    require(response.get('value') == 'NAK' and response.get('TargetName') == '8916'
            and response.get('MemoryName') == 'eMMC' and response.get('rawmode') in (None,'false')
            and response.get('MaxPayloadSizeToTargetInBytesSupported') == '16384'
            and response.get('MaxPayloadSizeToTargetInBytes') == '16384'
            and protocol.MAX_PAYLOAD == 16384, 'Unsupported programmer refusal or write limit')
    attempts = tools/'restore-attempts';marker = attempts/source.plan['plan_id']
    recovery.private_directory(attempts,0);recovery.private_directory(marker,0)
    require(failed.get('attempt_marker') == str(marker), 'Original consumed attempt differs')
    original,_ = planner.private_json(marker/'attempt.json')
    require(original.get('plan_sha256') == source.plan_hash and original.get('output') == str(path.parent)
            and protocol.sha(original.get('current_before_sha256'))
            and original.get('current_before_sha256') == failed.get('current_before',{}).get('sha256'),
            'Original attempt is not linked to the selected refusal')
    require(failed['current_before'].get('gpt_sha256') == source.plan['gpt_sha256']
            and failed['current_before'].get('boot_sha256') == source.plan['boot_sha256'],
            'Original board preimage differs from the selected plan')
    check,_ = planner.private_json(path.parent/'current-before-check/report.json')
    require(check.get('passed') is True and check.get('image_unchanged_after_fs_checks') is True
            and check.get('independent_readback_sha256') == original['current_before_sha256']
            and check.get('gpt_copies_checked') is True and check.get('current_boot_verified') is True,
            'Original clean full preimage evidence required')
    filesystems = check.get('filesystems',{})
    require(set(filesystems) == {'rootfs','NSHSTATE'}, 'Both original filesystem checks required')
    for part in source.targets:
        row = filesystems[part['name']]
        require(row.get('passed') is True and row.get('read_only') is True and row.get('detached') is True
                and type(row.get('e2fsck_exit')) is int and row['e2fsck_exit'] == 0
                and row.get('offset') == part['offset'] and row.get('bytes') == part['bytes'],
                'Original filesystem evidence incomplete')
    require(set(p.name for p in marker.iterdir()) == {'attempt.json'},
            'Continuation already consumed or original attempt has extra entries')
    return {'failed_report':str(path),'failed_report_sha256':digest,
            'original_attempt':str(marker),'marker':str(marker/'after-config-nak')}


def consume_once(tools, source, output, before, namespace='restore-attempts'):
    require(namespace in ('restore-attempts','install-attempts'),'Unsupported fixed attempt namespace')
    expected_kind='fixed-system-install-plan' if namespace=='install-attempts' else 'fixed-system-restore-plan'
    require(source.plan.get('kind')==expected_kind,'Attempt namespace differs from the selected operation')
    attempts=tools/namespace
    if not attempts.exists() and not attempts.is_symlink():
        attempts.mkdir(mode=0o700)
        fd=os.open(tools,os.O_RDONLY|os.O_DIRECTORY)
        try:os.fsync(fd)
        finally:os.close(fd)
    recovery.private_directory(attempts,0)
    marker=attempts/source.plan['plan_id']
    retry=getattr(source,'config_retry',None)
    if retry:
        require(namespace=='restore-attempts','Configuration continuation is not an installation')
        validated=validate_config_retry(tools,source,pathlib.Path(retry['failed_report']),retry['failed_report_sha256'])
        require(validated==retry,'Continuation evidence changed after validation')
        marker=pathlib.Path(retry['marker'])
    marker.mkdir(mode=0o700)  # atomic one-shot; unknown attempts cannot be overwritten/retried
    save_json(marker/'attempt.json', {'plan_sha256':source.plan_hash,'output':str(output),
              'current_before_sha256':before['sha256'],'started':datetime.datetime.now(datetime.timezone.utc).isoformat(),
              **({'explicit_config_retry':retry} if retry else {})})
    if retry:
        fd=os.open(marker.parent,os.O_RDONLY|os.O_DIRECTORY)
        try:os.fsync(fd)
        finally:os.close(fd)
    fd=os.open(attempts,os.O_RDONLY|os.O_DIRECTORY)
    try:os.fsync(fd)
    finally:os.close(fd)
    return str(marker)


def control_xml(name,attributes):
    root=ET.Element('data');ET.SubElement(root,name,{k:str(v) for k,v in attributes.items()})
    # Match the separately pinned programmer client's control envelope. Its
    # reset path uses this declaration and compact empty element spelling.
    return b'<?xml version="1.0" ?>'+ET.tostring(root,encoding='utf-8').replace(b' />',b'/>')


class Hardware:
    def __init__(self,client,policy,binding_check):
        self.client=client;self.policy=policy;self.binding_check=binding_check
        self.control_replies=[]
        self.fh=client.fh.firehose;self.cdc=client.cdc
        self.input=self.cdc.EP_IN;self.output=self.cdc.EP_OUT
        require(self.input.bEndpointAddress & 0x80 and not self.output.bEndpointAddress & 0x80
                and self.input.bmAttributes & 3 == self.output.bmAttributes & 3 == 2, 'Expected fixed bulk input/output endpoints')
        def strict_write(data,pktsize=None):
            try:
                protocol.write_bulk_once(self.output,data.encode() if isinstance(data,str) else data)
                return True
            except BaseException:
                policy.phase='failed';raise
        self.cdc.write=strict_write

    @classmethod
    def connect(cls,tools,port,policy):
        source=recovery.verify_tools(tools)
        binding=recovery.select_usb(port)
        recovery.validate_xml=policy.validate_xml
        client=recovery.load_runtime(source,tools/'prog_emmc_firehose_8916.mbn','gpt',port)
        try:
            import usb.util
            cdc=client.cdc
            require((cdc.device.bus,cdc.device.address)==binding and recovery.select_usb(port)==binding,
                    'USB binding changed during loader connection')
            interface=cdc.interface.bInterfaceNumber
            require(interface==0,'Unexpected programmer interface')
            usb.util.claim_interface(cdc.device,interface)  # upstream swallows claim failures; this must succeed
            def same_binding():require(recovery.select_usb(port)==binding,'Recorded USB connection changed; do not reconnect/retry')
            return cls(client,policy,same_binding)
        except BaseException:
            client.cdc.close(reset=False);raise

    def read_image(self,path):
        self.binding_check()
        require(not path.exists() and not path.is_symlink(),'Current capture output already exists')
        partial=path.with_name(path.name+'.partial')
        fd=os.open(partial,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600);os.close(fd)
        require(self.fh.cmd_read(0,0,recovery.SECTORS,str(partial),display=False) is True,'Full device read failed')
        fd,_=backup.open_image(partial)
        try:os.fsync(fd)
        finally:os.close(fd)
        partial.rename(path)

    def control(self,name,attributes):
        self.binding_check()
        data=control_xml(name,attributes)
        self.policy.validate_xml(data)
        try:
            protocol.write_bulk_once(self.output,data)
            reply=protocol.read_control_reply(self.input)
            self.control_replies.append({'operation':name,'xml_sha256':hashlib.sha256(data).hexdigest(),
                                         'response':copy.deepcopy(reply)})
            self.policy.acknowledge(reply)
            return reply
        except BaseException:
            self.policy.phase='failed';raise

    def payload(self,data):
        self.binding_check()
        self.policy.send_payload_chunk(self.output,data)

    def final_ack(self):
        try:
            reply=protocol.read_control_reply(self.input,timeout_seconds=30)
            self.control_replies.append({'operation':'program-final','target_index':self.policy.index,
                                         'response':copy.deepcopy(reply)})
            self.policy.acknowledge(reply)
        except BaseException:self.policy.phase='failed';raise

    def close(self):
        self.cdc.close(reset=False)  # release only; never reset uncertain/partially written storage


def _transaction(source,hardware,tools,output,action,report,save,consume_attempt):
    policy=hardware.policy
    report['phase']='reading-current-preimage';save()
    before_path=output/'current-before.img';hardware.read_image(before_path)
    before=inspect_image(before_path,source.targets,source.protected)
    require(before['gpt_sha256']==source.plan['gpt_sha256'] and before['boot_sha256']==source.plan['boot_sha256'],
            'Current device GPT/boot differs from the trusted plan')
    report['phase']='checking-current-preimage';report['current_before']=before;save()
    backup.verify(before_path,output/'current-before-check',before['sha256'],before['gpt_sha256'])
    source.ensure_frozen()
    proof=(before['gpt_sha256'],before['boot_sha256'],source.plan['source_image_sha256'],range_results(before['protected']))
    if action=='inspect':
        policy.finish_readonly_preflight(*proof)
        report['phase']='returning-after-readonly-preflight';save()
    else:
        report['post_capture_space_check']=require_capture_space(output,1);save()
        report['attempt_marker']=consume_attempt(tools,source,output,before);save()
        policy.confirm_preflight(*proof)
        report['phase']='write-configuration-attempted';report['write_configuration_attempted']=True;save()
        hardware.control('configure',{'MemoryName':'eMMC','Verbose':'0','AlwaysValidate':'0','MaxDigestTableSizeInBytes':'2048',
                         'MaxPayloadSizeToTargetInBytes':str(protocol.MAX_PAYLOAD),'ZLPAwareHost':'1','SkipStorageInit':'0','SkipWrite':'0'})
        for index,part in enumerate(source.targets):
            source.ensure_frozen()
            report['phase']='program-command-attempted';report['partition_writes_may_have_occurred']=True
            report['current_target']=part['name'];save()
            hardware.control('program',{'SECTOR_SIZE_IN_BYTES':'512','physical_partition_number':'0',
                             'start_sector':part['start_sector'],'num_partition_sectors':part['sector_count']})
            report.update(phase='sending-fixed-target-payload',raw_payload_transport_bytes=0);save()
            offset=part['offset'];remaining=part['bytes']
            next_progress=time.monotonic()+8
            while remaining:
                data=backup.read_at(source.fd,offset,min(protocol.MAX_PAYLOAD,remaining))
                hardware.payload(data);offset+=len(data);remaining-=len(data)
                if time.monotonic() >= next_progress:
                    report['raw_payload_transport_bytes']=policy.sent_bytes;save()
                    next_progress=time.monotonic()+8
            policy.finish_payload()
            report.update(phase='waiting-for-final-program-ack',raw_payload_transport_bytes=policy.sent_bytes);save()
            hardware.final_ack()
            report.setdefault('programmed_targets',[]).append({'name':part['name'],'bytes':part['bytes'],
                       'explicit_final_ack':True,'before_differs_from_source':before['targets'][index]['backup_sha256']!=part['backup_sha256']})
            save()
        report['phase']='reading-post-restore';save()
        after_path=output/'current-after.img';hardware.read_image(after_path)
        after=inspect_image(after_path,source.targets,source.protected);report['current_after']=after;save()
        policy.confirm_readback(after['gpt_sha256'],after['boot_sha256'],range_results(after['targets']),range_results(after['protected']))
        report['phase']='checking-post-restore-filesystems';save()
        backup.verify(after_path,output/'current-after-check',after['sha256'],after['gpt_sha256'])
        source.ensure_frozen()
        report['partition_restore_readback_verified']=True;save()
    report['phase']='normal-reset-attempted';report['normal_reset_attempted']=True;save()
    hardware.control('power',{'value':'reset'})
    report.update(phase='completed-reset-acknowledged',passed=True,normal_reset_acknowledged=True,policy_phase=policy.phase)
    report['control_replies']=copy.deepcopy(hardware.control_replies)
    save()


def transaction(source,hardware,tools,output,action,report,save,consume_attempt=None):
    try:
        _transaction(source,hardware,tools,output,action,report,save,consume_attempt or consume_once)
    except BaseException:
        hardware.policy.phase='failed'
        report['policy_phase']='failed'
        report['control_replies']=copy.deepcopy(hardware.control_replies)
        save()
        raise


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--action',choices=('validate','inspect','restore'),default='validate')
    parser.add_argument('--tools',required=True,type=pathlib.Path)
    parser.add_argument('--plan',required=True,type=pathlib.Path)
    parser.add_argument('--plan-sha256',required=True)
    parser.add_argument('--capture-report',required=True,type=pathlib.Path)
    parser.add_argument('--offline-report',required=True,type=pathlib.Path)
    parser.add_argument('--output',required=True,type=pathlib.Path)
    parser.add_argument('--usb-port')
    parser.add_argument('--confirm-restore-plan-sha256')
    parser.add_argument('--retry-after-config-nak-report',type=pathlib.Path)
    parser.add_argument('--retry-after-config-nak-report-sha256')
    args=parser.parse_args();os.umask(0o077)
    require(os.geteuid()==0,'Local Root operator required')
    require(bool(args.retry_after_config_nak_report)==bool(args.retry_after_config_nak_report_sha256),
            'Select both the exact failed report and its digest')
    require(not args.retry_after_config_nak_report or args.action in ('validate','restore'),
            'Configuration continuation is only available for explicit restore or validation')
    if args.action=='restore':
        require(args.confirm_restore_plan_sha256==args.plan_sha256 and protocol.sha(args.plan_sha256),
                'Explicit confirmation of this exact restore plan is required')
    else:require(args.confirm_restore_plan_sha256 is None,'No restore confirmation accepted for non-restore action')
    require(args.output.is_absolute() and args.output.resolve()==args.output and not args.output.exists(),'New private output required')
    recovery.private_directory(args.output.parent,0);recovery.private_directory(args.tools,0)
    capture_space = None
    if args.action != 'validate':
        capture_space = require_capture_space(args.output.parent,2 if args.action == 'restore' else 1)
    expected,_=planner.private_json(args.tools/'expected-device.json')
    if args.action!='validate':require(args.usb_port==expected.get('usb_port'),'Recorded physical USB port required')
    args.output.mkdir(mode=0o700)
    report={'format':1,'action':args.action,'passed':False,'phase':'validating-inputs','usb_opened':False,
            'partition_writes_may_have_occurred':False,'normal_reset_attempted':False,'normal_reset_acknowledged':False,
            'recovery_verified':False,'board_boot_verified':False,'release_ready':False,
            'executor_sha256':recovery.digest(pathlib.Path(__file__).resolve())}
    if capture_space:report['initial_capture_space_check']=capture_space
    source=hardware=None
    def save():save_json(args.output/'report.json',report)
    def stop(*_):raise InterruptedError('Bounded restore operation interrupted; inspect before any new attempt')
    old_handlers={sig:signal.getsignal(sig) for sig in (signal.SIGALRM,signal.SIGTERM,signal.SIGHUP)}
    for sig in old_handlers:signal.signal(sig,stop)
    signal.alarm(5400)
    try:
        save()
        source=Source(args.plan,args.plan_sha256,args.capture_report,args.offline_report)
        require(source.plan['gpt_sha256']==expected.get('gpt_sha256'),'Plan differs from trusted device GPT')
        if args.retry_after_config_nak_report:
            source.config_retry=validate_config_retry(args.tools,source,args.retry_after_config_nak_report,
                                                      args.retry_after_config_nak_report_sha256)
            report['explicit_config_retry']=source.config_retry;save()
        report.update(plan_sha256=source.plan_hash,plan_id=source.plan['plan_id'],source_image_sha256=source.plan['source_image_sha256']);save()
        recovery.verify_tools(args.tools)
        if args.action=='validate':
            report.update(passed=True,phase='local-validation-completed');save()
        else:
            with tool_lock(args.tools):
                if args.action=='restore':
                    marker=args.tools/'restore-attempts'/source.plan['plan_id']
                    if getattr(source,'config_retry',None):
                        validate_config_retry(args.tools,source,args.retry_after_config_nak_report,args.retry_after_config_nak_report_sha256)
                    else:
                        require(not marker.exists() and not marker.is_symlink(),'Restore attempt already consumed; inspect, never retry automatically')
                policy=protocol.RestorePolicy(source.plan,source.targets,source.protected)
                report.update(phase='connecting-recorded-9008',usb_opened=None,usb_open_attempted=True);save()
                hardware=Hardware.connect(args.tools,args.usb_port,policy)
                report['usb_opened']=True;save()
                transaction(source,hardware,args.tools,args.output,args.action,report,save)
        print('Fixed restore action finished; inspect private report and separately verify normal-system return')
    except BaseException as exc:
        report.update(passed=False,phase='failed-inspect-current-mode',error=str(exc),error_type=type(exc).__name__)
        if hardware:
            hardware.policy.phase='failed';report['policy_phase']='failed'
        save();raise
    finally:
        signal.alarm(0)
        for sig,handler in old_handlers.items():signal.signal(sig,handler)
        try:
            if hardware:hardware.close()
        finally:
            try:
                if source:source.close()
            finally:save()


if __name__=='__main__':main()
