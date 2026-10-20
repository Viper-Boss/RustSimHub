#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed private first-boot candidate and explicit UFI003 experimental installation."""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
import pathlib
import secrets
import shutil
import signal
import stat

BASE=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('install_executor',BASE/'restore-system.py')
executor=importlib.util.module_from_spec(spec);spec.loader.exec_module(executor)
require=executor.require
planner=executor.planner
backup=executor.backup
recovery=executor.recovery
protocol=executor.protocol

# This initial operator deliberately accepts only the already audited candidate.
ROOTFS_SHA256='3ac9bc81ceaaa4b0807217b49de4c1622040d11165373100e289d8975d656d88'
ROOTFS_TREE='c621fdbdedee685d0f6a930a949d36f2cb6dea29953d8240b4dda7bb17b95376'
STATE_BUILDER_SHA256='15f5a71f4bbbf9b2ba22a8b274cc8fcff36bb377f7108f02b5908614c79cae74'
FALSE_GATES=('flash_allowed','release_ready','recovery_verified','board_boot_verified')


def runtime_hashes():
    return {name:recovery.digest(BASE/name) for name in ('install-system.py','restore-system.py','restore_protocol.py',
            'plan-system-restore.py','verify-offline-backup.py','edl_readonly.py')}


def open_part(path,size,expected):
    require(path.is_absolute() and path.resolve()==path,'Canonical private partition image required')
    recovery.private_directory(path.parent,0)
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    try:
        info=os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_size==size and info.st_uid==info.st_gid==0
                and info.st_nlink==1 and stat.S_IMODE(info.st_mode)==0o600,'Invalid private partition image')
        signature=backup.signature(info)
        require(protocol.sha(expected) and backup.digest_fd(fd)==expected,'Partition image digest mismatch')
        require(signature==backup.signature(os.fstat(fd))==backup.signature(path.lstat()),'Partition image changed')
        return fd,signature
    except BaseException:os.close(fd);raise


def check_manifests(root_manifest,state_report,backup_plan_sha):
    require(root_manifest.get('kind')=='internal-unflashable-system-image'
            and root_manifest.get('passed') is True and all(root_manifest.get(k) is False for k in FALSE_GATES)
            and root_manifest.get('image_sha256')==ROOTFS_SHA256 and root_manifest.get('tree_sha256')==ROOTFS_TREE
            and root_manifest.get('bytes')==recovery.PARTITIONS[14][1]
            and root_manifest.get('private_data_included') is False
            and root_manifest.get('private_firmware_included') is False
            and root_manifest.get('device_state_initialized') is False
            and root_manifest.get('generic_audit_passed') is True
            and root_manifest.get('source_tree_unchanged') is True
            and root_manifest.get('filesystem_clean_readonly_check') is True,'Wrong or incomplete audited generic rootfs')
    require(state_report.get('kind')=='offline-maintenance-state-candidate' and state_report.get('passed') is True
            and state_report.get('builder_sha256')==STATE_BUILDER_SHA256
            and state_report.get('source_plan_sha256')==backup_plan_sha
            and state_report.get('bytes')==recovery.PARTITIONS[15][1]
            and all(state_report.get(k) is True for k in ('source_image_unchanged','private_mount_namespace',
                'loop_detached','legacy_entries_unchanged','volume_identity_preserved','filesystem_checks_passed'))
            and all(state_report.get(k) is False for k in (*FALSE_GATES,'device_installed','device_partition_writes'))
            and protocol.sha(state_report.get('candidate_state_sha256'))
            and protocol.sha(state_report.get('maintenance_seed_manifest_sha256'))
            and protocol.sha(state_report.get('maintenance_public_sha256')),'Wrong or incomplete matched private state')


def inputs(backup_plan,backup_plan_sha,capture,offline,root_image,root_manifest,state_image,state_report):
    rollback=executor.Source(backup_plan,backup_plan_sha,capture,offline)
    descriptors=[]
    try:
        root,root_hash=planner.private_json(root_manifest)
        state,state_hash=planner.private_json(state_report)
        check_manifests(root,state,backup_plan_sha)
        for path,size,digest in ((root_image,recovery.PARTITIONS[14][1],ROOTFS_SHA256),
                                 (state_image,recovery.PARTITIONS[15][1],state['candidate_state_sha256'])):
            fd,signature=open_part(path,size,digest);descriptors.append((fd,signature,path,digest))
        evidence={'backup_plan':str(backup_plan),'backup_plan_sha256':backup_plan_sha,
                  'capture_report':str(capture),'offline_report':str(offline),
                  'root_image':str(root_image),'root_manifest':str(root_manifest),'root_manifest_sha256':root_hash,
                  'state_image':str(state_image),'state_report':str(state_report),'state_report_sha256':state_hash,
                  'maintenance_seed_manifest_sha256':state['maintenance_seed_manifest_sha256'],
                  'maintenance_public_sha256':state['maintenance_public_sha256']}
        return rollback,descriptors,evidence
    except BaseException:
        for fd,_,_,_ in descriptors:os.close(fd)
        rollback.close();raise


def close_inputs(rollback,descriptors):
    for fd,_,_,_ in descriptors:os.close(fd)
    rollback.close()


def frozen_inputs(rollback,descriptors):
    rollback.ensure_frozen()
    for fd,signature,path,digest in descriptors:
        require(signature==backup.signature(os.fstat(fd))==backup.signature(path.lstat()),'Selected input metadata changed')
        require(backup.digest_fd(fd)==digest,'Selected input content changed')
        require(signature==backup.signature(os.fstat(fd))==backup.signature(path.lstat()),'Selected input changed while hashing')


def prepare(args):
    require(not args.output.exists() and not args.output.is_symlink(),'New output required; preserve all existing attempts')
    require(args.output.is_absolute() and args.output.resolve()==args.output,'Canonical private output required')
    recovery.private_directory(args.output.parent,0)
    require(shutil.disk_usage(args.output.parent).free>=recovery.DISK_BYTES+256*1024*1024,'Insufficient candidate space')
    rollback,descriptors,evidence=inputs(args.backup_plan,args.backup_plan_sha256,args.capture_report,args.offline_report,
        args.root_image,args.root_manifest,args.state_image,args.state_report)
    try:
        args.output.mkdir(mode=0o700)
        image=args.output/'candidate.img'
        out=os.open(image,os.O_RDWR|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
        try:
            offset=0
            while offset<recovery.DISK_BYTES:
                data=backup.read_at(rollback.fd,offset,min(4*1024*1024,recovery.DISK_BYTES-offset))
                require(os.write(out,data)==len(data),'Short candidate copy');offset+=len(data)
            for part,(fd,_,_,_) in zip(rollback.targets,descriptors):
                offset=0
                while offset<part['bytes']:
                    data=backup.read_at(fd,offset,min(4*1024*1024,part['bytes']-offset))
                    require(os.pwrite(out,data,part['offset']+offset)==len(data),'Short candidate partition copy');offset+=len(data)
            os.fsync(out)
        finally:os.close(out)
        result=executor.inspect_image(image,rollback.targets,rollback.protected)
        require(result['protected']==rollback.protected,'Candidate changed protected backup bytes')
        require(result['targets'][0]['backup_sha256']==ROOTFS_SHA256
                and result['targets'][1]['backup_sha256']==descriptors[1][3],'Candidate partition digest mismatch')
        require(result['gpt_sha256']==rollback.plan['gpt_sha256']
                and result['boot_sha256']==rollback.plan['boot_sha256'],'Candidate GPT or boot changed')
        backup.verify(image,args.output/'filesystem-check',result['sha256'],result['gpt_sha256'])
        frozen_inputs(rollback,descriptors)
        plan={'format':1,'kind':'fixed-system-install-plan','passed':True,'plan_id':secrets.token_hex(16),
              'installer_sha256':recovery.digest(pathlib.Path(__file__).resolve()),'board':'ufi003',
              'runtime_sha256':runtime_hashes(),
              'source_image':str(image),'source_image_sha256':result['sha256'],
              'write_targets':result['targets'],'protected_ranges':result['protected'],
              'gpt_sha256':result['gpt_sha256'],'boot_sha256':result['boot_sha256'],
              'protected_range_policy':'retain-current-device-bytes-and-verify-before-after',
              'backup_protected_hashes_are_snapshot_only':True,'source_image_unchanged':True,
              'execution_authorized':False,'usb_opened':False,'partition_writes':False,
              'experimental_cold_boot_only':True,'restore_rehearsal_verified':False,
              **{k:False for k in FALSE_GATES},'evidence':evidence}
        executor.save_json(args.output/'plan.json',plan)
        return plan
    finally:close_inputs(rollback,descriptors)


class Source:
    def __init__(self,path,expected_hash):
        self.fd=None;self.rollback=None;self.descriptors=[]
        self.plan,self.plan_hash=planner.private_json(path)
        p=self.plan
        require(protocol.sha(expected_hash) and self.plan_hash==expected_hash,'Exact selected install plan required')
        require(type(p.get('format')) is int and p['format']==1 and p.get('kind')=='fixed-system-install-plan'
                and p.get('passed') is True and p.get('board')=='ufi003'
                and isinstance(p.get('plan_id'),str) and len(p['plan_id'])==32
                and all(c in '0123456789abcdef' for c in p['plan_id'])
                and p.get('installer_sha256')==recovery.digest(pathlib.Path(__file__).resolve())
                and p.get('runtime_sha256')==runtime_hashes(),'Unsupported frozen installer plan')
        require(all(p.get(k) is False for k in (*FALSE_GATES,'execution_authorized','usb_opened','partition_writes',
                    'restore_rehearsal_verified')) and p.get('experimental_cold_boot_only') is True
                and p.get('source_image_unchanged') is True
                and p.get('protected_range_policy')=='retain-current-device-bytes-and-verify-before-after'
                and p.get('backup_protected_hashes_are_snapshot_only') is True,'Unexpected install readiness gates')
        e=p['evidence']
        try:
            self.rollback,self.descriptors,actual=inputs(pathlib.Path(e['backup_plan']),e['backup_plan_sha256'],
                pathlib.Path(e['capture_report']),pathlib.Path(e['offline_report']),pathlib.Path(e['root_image']),
                pathlib.Path(e['root_manifest']),pathlib.Path(e['state_image']),pathlib.Path(e['state_report']))
            require(actual==e,'Frozen install provenance changed')
            self.path=pathlib.Path(p['source_image']);self.fd,self.signature=backup.open_image(self.path)
            actual=executor.inspect_image(self.path,self.rollback.targets,self.rollback.protected)
            require(actual['sha256']==p['source_image_sha256'] and actual['targets']==p['write_targets']
                    and actual['protected']==p['protected_ranges']==self.rollback.protected
                    and actual['gpt_sha256']==p['gpt_sha256']==self.rollback.plan['gpt_sha256']
                    and actual['boot_sha256']==p['boot_sha256']==recovery.BOOT_SHA256,'Install candidate differs from its plan')
            require(actual['targets'][0]['backup_sha256']==self.descriptors[0][3]
                    and actual['targets'][1]['backup_sha256']==self.descriptors[1][3],'Install targets differ from verified inputs')
            self.targets=actual['targets'];self.protected=actual['protected'];self.ensure_frozen()
        except BaseException:self.close();raise

    def ensure_frozen(self):
        require(self.signature==backup.signature(os.fstat(self.fd))==backup.signature(self.path.lstat()),'Candidate metadata changed')
        require(backup.digest_fd(self.fd)==self.plan['source_image_sha256'],'Candidate bytes changed')
        require(self.signature==backup.signature(os.fstat(self.fd))==backup.signature(self.path.lstat()),'Candidate changed during hash')
        frozen_inputs(self.rollback,self.descriptors)

    def close(self):
        if self.fd is not None:os.close(self.fd);self.fd=None
        if self.rollback is not None:close_inputs(self.rollback,self.descriptors);self.rollback=None;self.descriptors=[]


def consume_install(tools,source,output,before):
    require(source.plan['kind']=='fixed-system-install-plan','An explicit installation cannot consume a restore attempt')
    return executor.consume_once(tools,source,output,before,namespace='install-attempts')


def execute(args):
    require(args.action!='install' or (args.accept_unrehearsed_restore
            and args.confirm_install_plan_sha256==args.plan_sha256 and protocol.sha(args.plan_sha256)),
            'Explicit exact experimental install confirmation and unrehearsed-restore acknowledgement required')
    require(args.action=='install' or (args.confirm_install_plan_sha256 is None and not args.accept_unrehearsed_restore),
            'No installation acknowledgement accepted for validation')
    require(args.output.is_absolute() and args.output.resolve()==args.output and not args.output.exists(),
            'New private install output required')
    recovery.private_directory(args.output.parent,0);recovery.private_directory(args.tools,0)
    if args.action=='install':executor.require_capture_space(args.output.parent,2)
    expected,_=planner.private_json(args.tools/'expected-device.json')
    if args.action=='install':require(args.usb_port==expected.get('usb_port'),'Exact recorded physical USB port required')
    args.output.mkdir(mode=0o700)
    report={'format':1,'action':args.action,'passed':False,'phase':'validating-inputs','usb_opened':False,
            'partition_writes_may_have_occurred':False,'normal_reset_attempted':False,'normal_reset_acknowledged':False,
            'new_os_installed':False,'restore_rehearsal_verified':False,**{k:False for k in FALSE_GATES},
            'installer_sha256':recovery.digest(pathlib.Path(__file__).resolve())}
    def save():executor.save_json(args.output/'report.json',report)
    source=hardware=None
    try:
        save();source=Source(args.plan,args.plan_sha256)
        require(source.plan['gpt_sha256']==expected.get('gpt_sha256'),'Installation differs from trusted board GPT')
        report.update(plan_sha256=source.plan_hash,plan_id=source.plan['plan_id']);save()
        recovery.verify_tools(args.tools)
        if args.action=='validate':report.update(passed=True,phase='local-validation-completed');save()
        else:
            with executor.tool_lock(args.tools):
                marker=args.tools/'install-attempts'/source.plan['plan_id']
                require(not marker.exists() and not marker.is_symlink(),'Installation attempt consumed; no retry')
                policy=protocol.RestorePolicy(source.plan,source.targets,source.protected)
                report.update(phase='connecting-recorded-9008',usb_opened=None,usb_open_attempted=True);save()
                hardware=executor.Hardware.connect(args.tools,args.usb_port,policy);report['usb_opened']=True;save()
                executor.transaction(source,hardware,args.tools,args.output,'install',report,save,consume_attempt=consume_install)
                report['partition_install_readback_verified']=report.pop('partition_restore_readback_verified')
                report['new_os_partition_payloads_installed']=True
                # Normal boot/login/health remains a separate independent check.
                save()
    except BaseException as exc:
        report.update(passed=False,phase='failed-inspect-current-mode',error=str(exc),error_type=type(exc).__name__)
        if hardware:report['control_replies']=copy.deepcopy(hardware.control_replies)
        save();raise
    finally:
        try:
            if hardware:hardware.close()
        finally:
            if source:source.close()
    return report


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--action',choices=('prepare','validate','install'),default='validate')
    p.add_argument('--output',type=pathlib.Path,required=True)
    for name in ('backup-plan','capture-report','offline-report','root-image','root-manifest','state-image','state-report','tools','plan'):
        p.add_argument('--'+name,type=pathlib.Path)
    for name in ('backup-plan-sha256','plan-sha256','usb-port','confirm-install-plan-sha256'):p.add_argument('--'+name)
    p.add_argument('--accept-unrehearsed-restore',action='store_true')
    args=p.parse_args();os.umask(0o077)
    require(os.geteuid()==0,'Local Root operator required')
    required=('backup_plan','backup_plan_sha256','capture_report','offline_report','root_image','root_manifest','state_image','state_report') if args.action=='prepare' else ('tools','plan','plan_sha256')
    require(all(getattr(args,k) is not None for k in required),'Missing fixed preparation/installation inputs')
    def stop(*_):raise InterruptedError('Bounded installation interrupted; inspect before any new action')
    old={sig:signal.getsignal(sig) for sig in (signal.SIGALRM,signal.SIGTERM,signal.SIGHUP)}
    for sig in old:signal.signal(sig,stop)
    signal.alarm(5400)
    try:
        if args.action=='prepare':prepare(args)
        else:execute(args)
    finally:
        signal.alarm(0)
        for sig,handler in old.items():signal.signal(sig,handler)
    print('Fixed experimental install action finished; inspect private evidence, separately verify normal boot and maintenance')


if __name__=='__main__':main()
