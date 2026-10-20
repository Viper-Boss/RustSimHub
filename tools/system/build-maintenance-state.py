#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build a private NSHSTATE candidate from a verified backup; never opens USB or a live partition."""
import argparse
import base64
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import shutil
import signal
import stat
import subprocess
import sys

BASE=pathlib.Path(__file__).resolve().parent
RECOVERY=BASE if (BASE/'restore-system.py').is_file() else BASE.parent/'recovery'
sys.path.insert(0,str(RECOVERY))
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
executor=load('state_candidate_executor',RECOVERY/'restore-system.py')
seed=load('state_candidate_seed',BASE/'prepare-maintenance-seed.py')
require=executor.require

def snapshot_tree(root,exclude_namespace=False):
    """Never follow stored links, open devices/FIFOs, or traverse another mount."""
    device=root.stat().st_dev
    rows={}
    for directory,dirs,files in os.walk(root,topdown=True,followlinks=False):
        directory=pathlib.Path(directory)
        if directory==root and exclude_namespace:
            dirs[:]=[name for name in dirs if name!='rustsimhub-v1']
            files=[name for name in files if name!='rustsimhub-v1']
        for name in sorted(dirs+files):
            path=directory/name;info=path.lstat()
            require(info.st_dev==device,'Unexpected mounted subtree in offline state')
            row={'uid':info.st_uid,'gid':info.st_gid,'mode':stat.S_IMODE(info.st_mode),
                 'links':info.st_nlink,'mtime_ns':info.st_mtime_ns}
            if stat.S_ISLNK(info.st_mode):
                row.update(kind='symlink',target=os.readlink(path))
            elif stat.S_ISDIR(info.st_mode):
                row['kind']='directory'
            elif stat.S_ISREG(info.st_mode):
                fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
                try:
                    require(executor.backup.signature(os.fstat(fd))==executor.backup.signature(info),
                            'Offline legacy file changed while opening')
                    row.update(kind='file',bytes=info.st_size,sha256=executor.backup.digest_fd(fd))
                    require(executor.backup.signature(os.fstat(fd))==executor.backup.signature(info)
                            ==executor.backup.signature(path.lstat()),'Offline legacy file changed while hashing')
                finally:os.close(fd)
            else:raise ValueError('Special file in legacy state; preserve and inspect instead of opening it')
            row['xattrs']={name:base64.b64encode(os.getxattr(path,name,follow_symlinks=False)).decode()
                           for name in sorted(os.listxattr(path,follow_symlinks=False))}
            rows[str(path.relative_to(root))]=row
    return rows

def install_namespace(mount,source_seed,expected_public_sha256):
    seed.validate_seed(source_seed,expected_public_sha256)
    namespace=mount/'rustsimhub-v1'
    require(not namespace.exists() and not namespace.is_symlink(),'Existing Rust namespace must never be replaced')
    original=mount.stat()
    require((original.st_uid,original.st_gid,stat.S_IMODE(original.st_mode))==(101,103,0o700),
            'Expected the preserved first-board legacy mount root')
    before=snapshot_tree(mount)
    shutil.copytree(source_seed/'rustsimhub-v1',namespace,copy_function=shutil.copy2)
    for source in (source_seed/'rustsimhub-v1').rglob('*'):
        target=namespace/source.relative_to(source_seed/'rustsimhub-v1')
        if source.is_dir():seed.private_dir(target)
        else:
            require(seed.read_file(target,4096)==seed.read_file(source,4096),'Selected seed bytes changed during copy')
            fd=os.open(target,os.O_RDONLY|os.O_NOFOLLOW)
            try:os.fsync(fd)
            finally:os.close(fd)
    seed.private_dir(namespace)
    os.chown(mount,0,0);os.chmod(mount,0o700)
    after=snapshot_tree(mount,exclude_namespace=True)
    require(before==after,'Existing legacy state contents or metadata changed')
    for directory in sorted((p for p in namespace.rglob('*') if p.is_dir()),key=lambda p:len(p.parts),reverse=True):
        seed.sync_dir(directory)
    seed.sync_dir(namespace);seed.sync_dir(mount)
    return {'legacy_entry_count':len(before),'legacy_tree_sha256':hashlib.sha256(
                json.dumps(before,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
            'legacy_entries_unchanged':True,'mount_root_before':{'uid':101,'gid':103,'mode':'0700'},
            'mount_root_after':{'uid':0,'gid':0,'mode':'0700'},'selected_seed_bytes_verified':True}

def check_fs(image,log):
    with log.open('xb') as stream:
        result=subprocess.run(['e2fsck','-f','-n',str(image)],stdout=stream,stderr=subprocess.STDOUT,timeout=180)
    require(result.returncode==0,'Candidate state filesystem is not clean; no repair is automatic')

def volume(image):
    raw=subprocess.check_output(['blkid','-p','-o','export',str(image)],text=True,timeout=20)
    fields=dict(line.split('=',1) for line in raw.splitlines() if '=' in line)
    require(fields.get('TYPE')=='ext4' and fields.get('LABEL')=='NSHSTATE','Expected fixed NSHSTATE ext4 volume')
    return {key:fields[key] for key in ('TYPE','LABEL','UUID')}

def prepare(args):
    require(os.geteuid()==0,'Local Root operator required')
    require(os.stat('/proc/self/ns/mnt').st_ino!=os.stat('/proc/1/ns/mnt').st_ino,
            'Run in a private mount namespace: unshare --mount --propagation private')
    os.umask(0o077)
    seed.private_dir(args.output.parent)
    require(args.output.is_absolute() and args.output.resolve()==args.output,'Canonical new output required')
    manifest,_,_=seed.validate_seed(args.seed_directory,args.expected_public_sha256)
    args.output.mkdir(mode=0o700)  # conflict is outside the report writer: never touch an existing attempt
    source=None
    loop=None;mounted=False;image_fd=None
    report={'format':1,'kind':'offline-maintenance-state-candidate','passed':False,
            'phase':'validating-source','source_image_unchanged':False,'device_partition_writes':False,
            'device_installed':False,'recovery_verified':False,'board_boot_verified':False,'flash_allowed':False,
            'release_ready':False,'private_mount_namespace':True,'loop_detached':False,
            'maintenance_seed_manifest_sha256':hashlib.sha256((args.seed_directory/'manifest.json').read_bytes()).hexdigest(),
            'maintenance_public_sha256':manifest['maintenance_public_sha256'],
            'host_fingerprint':manifest['host_fingerprint'],'source_plan_sha256':args.plan_sha256,
            'builder_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()}
    try:
        def save():executor.save_json(args.output/'report.json',report)
        save()
        source=executor.Source(args.plan,args.plan_sha256,args.capture_report,args.offline_report)
        part=source.targets[1]
        require(part['index']==15 and part['name']=='NSHSTATE','Only the fixed state partition can be copied')
        require(shutil.disk_usage(args.output).free >= part['bytes']*2+32*1024*1024,
                'Insufficient space for a retained private state candidate')
        partial=args.output/'NSHSTATE.img.partial'
        fd=os.open(partial,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
        digest=hashlib.sha256();remaining=part['bytes'];offset=part['offset']
        with os.fdopen(fd,'wb') as stream:
            while remaining:
                data=executor.backup.read_at(source.fd,offset,min(4*1024*1024,remaining))
                require(stream.write(data)==len(data),'Short state image copy')
                digest.update(data);remaining-=len(data);offset+=len(data)
            stream.flush();os.fsync(stream.fileno())
        require(digest.hexdigest()==part['backup_sha256'],'State copy differs from the frozen backup')
        image=args.output/'NSHSTATE.img';partial.rename(image);seed.sync_dir(args.output)
        report.update(phase='checking-original-copy',source_state_sha256=digest.hexdigest(),bytes=part['bytes']);save()
        check_fs(image,args.output/'before-e2fsck.log');before_volume=volume(image)
        mount=args.output/'mount';mount.mkdir(mode=0o700)
        image_fd=os.open(image,os.O_RDWR|os.O_NOFOLLOW)
        info=os.fstat(image_fd)
        require(stat.S_ISREG(info.st_mode) and info.st_uid==info.st_gid==0 and info.st_nlink==1
                and stat.S_IMODE(info.st_mode)==0o600 and info.st_size==part['bytes'],'New private image metadata mismatch')
        loop=subprocess.check_output(['losetup','--find','--show','/proc/self/fd/'+str(image_fd)],
                                     pass_fds=(image_fd,),text=True,timeout=20).strip()
        require(re.fullmatch('/dev/loop[0-9]+',loop),'Unexpected candidate loop device')
        row=json.loads(subprocess.check_output(['losetup','--list','--json','--output',
                       'NAME,BACK-FILE,OFFSET,SIZELIMIT,RO',loop],text=True,timeout=20))['loopdevices']
        require(len(row)==1 and row[0]['name']==loop and row[0]['ro'] in (False,0,'0')
                and int(row[0]['offset'])==0 and int(row[0]['sizelimit'])==0
                and pathlib.Path(row[0]['back-file']).stat().st_ino==info.st_ino
                and pathlib.Path(row[0]['back-file']).stat().st_dev==info.st_dev,'Loop must map only the new regular image')
        report.update(phase='installing-selected-seed-in-offline-copy',loop=loop);save()
        mounted=True
        subprocess.run(['mount','-t','ext4','-o','rw,nodev,nosuid,noexec,noatime',loop,str(mount)],check=True,timeout=20)
        mapping=subprocess.check_output(['findmnt','-rn','-M',str(mount),'-o','SOURCE,FSTYPE'],text=True,timeout=10).split()
        require(mapping==[loop,'ext4'],'Unexpected offline state mount')
        report.update(install_namespace(mount,args.seed_directory,args.expected_public_sha256));save()
        subprocess.run(['umount',str(mount)],check=True,timeout=30);mounted=False
        subprocess.run(['losetup','--detach',loop],check=True,timeout=20);loop=None
        report.update(loop_detached=True,phase='checking-completed-copy');save()
        os.fsync(image_fd)
        check_fs(image,args.output/'after-e2fsck.log')
        require(volume(image)==before_volume,'Filesystem label/type/UUID changed')
        require(image.stat().st_size==part['bytes'],'Candidate image length changed')
        report['candidate_state_sha256']=executor.backup.digest_fd(image_fd)
        source.ensure_frozen()
        report.update(passed=True,phase='offline-state-candidate-completed',source_image_unchanged=True,
                      volume_identity_preserved=True,filesystem_checks_passed=True)
        save()
        return report
    except BaseException as exc:
        report.update(passed=False,phase='failed-preserve-private-copy',error=str(exc),error_type=type(exc).__name__)
        if args.output.exists():executor.save_json(args.output/'report.json',report)
        raise
    finally:
        try:
            if mounted:
                subprocess.run(['umount',str(args.output/'mount')],check=True,timeout=30)
                mounted=False
            if loop:
                subprocess.run(['losetup','--detach',loop],check=True,timeout=20);loop=None
            report['loop_detached']=not loop and not mounted
        finally:
            if image_fd is not None:os.close(image_fd)
            if source:source.close()
            if args.output.exists():executor.save_json(args.output/'report.json',report)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('plan','capture-report','offline-report','seed-directory','output'):
        p.add_argument('--'+name,type=pathlib.Path,required=True)
    p.add_argument('--plan-sha256',required=True);p.add_argument('--expected-public-sha256',required=True)
    args=p.parse_args()
    def stop(*_):raise TimeoutError('Bounded offline state preparation interrupted; preserve output')
    signal.signal(signal.SIGALRM,stop);signal.signal(signal.SIGTERM,stop);signal.alarm(1800)
    try:prepare(args)
    finally:signal.alarm(0)
    print('Private offline NSHSTATE candidate prepared; source/legacy retained; no device installed or partition written')

if __name__=='__main__':main()
