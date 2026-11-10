#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Reproduce the isolated production chain; never operates a host installation.

Native mode has a cfg(test)-only architecture/PID1/cgroup adapter. ARM VM mode
uses formal ARM updater/Core and actual guest kernel process/cgroup facts.
Both modes use synthetic MM/PID1, temporary fixture signing keys/data, fixed
production dispatch, original Store/Auth/migrations and confirmation ledger.
"""
from pathlib import Path
import argparse
import gzip
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import uuid

ROOT=Path(__file__).resolve().parents[2]
FIXTURES=Path(__file__).with_name('fixtures')
PROGRAMS=('rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa',
          'rustsimhub-forwarder','rustsimhub-core-launcher','rustsimhub-update',
          'rustsimhub-ble-recovery','rustsimhub-migrate','nkn-root','nkn-sdk-engine')


def run(command,**kwargs):
    return subprocess.run(command,check=True,timeout=kwargs.pop('timeout',300),**kwargs)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy(source,destination,mode=0o755):
    source=source.resolve(strict=True)
    if not source.is_file():raise ValueError('regular input required')
    destination.parent.mkdir(parents=True,exist_ok=True)
    if destination.exists():destination.chmod(0o755)
    shutil.copyfile(source,destination);destination.chmod(mode)


def inputs(args,stage):
    if not re.fullmatch('[0-9a-f]{40}',args.revision):raise ValueError('full source revision required')
    archive=subprocess.check_output(['git','archive','--format=tar',args.revision],cwd=ROOT,timeout=10)
    source=hashlib.sha256(archive).hexdigest()
    programs=stage/'fixture/programs'
    names=PROGRAMS if args.scenario in ('product','email') else tuple(n for n in PROGRAMS if n not in ('nkn-root','nkn-sdk-engine','rustsimhub-ble-recovery'))
    for name in names:copy(args.programs/name,programs/name)
    copy(ROOT/'licenses/Opus-COPYING.txt',programs/'Opus-COPYING.txt',0o444)
    copy(args.test_driver,stage/'fixture/update-tests')
    for name in ['systemctl.py','entry.py']:copy(FIXTURES/name,stage/('usr/bin/systemctl' if name=='systemctl.py' else 'fixture/entry.py'))
    if args.scenario=='email':
        if args.legacy_driver is None:raise ValueError('separate original format3 fixture required')
        copy(args.legacy_driver,stage/'fixture/legacy-outbox')
    marker=stage/'fixture/WEB4_ISOLATED';marker.write_bytes(b'private-production-wiring-only\n');marker.chmod(0o444)
    (stage/'fixture/source-commit').write_text(args.revision+'\n')
    (stage/'fixture/source-archive').write_text(source+'\n')
    (stage/'fixture/scenario').write_text(args.scenario+'\n')
    result={'revision':args.revision,'source_archive_sha256':source,
            'programs':{name:{'bytes':(programs/name).stat().st_size,'sha256':sha(programs/name)} for name in names},
            'driver_sha256':sha(args.test_driver),'hardware_access':False,'production_keys':False,
            'production_epoch_assigned':False,'network':False}
    if args.scenario=='email':result['legacy_driver_sha256']=sha(args.legacy_driver)
    (stage/'fixture/inputs.json').write_text(json.dumps(result,sort_keys=True,indent=2)+'\n')
    return result


def native(args,work):
    if not re.fullmatch('sha256:[0-9a-f]{64}',args.runtime_image or ''):raise ValueError('existing immutable Python runtime image required')
    stage=work/'context/rootfs';stage.mkdir(parents=True)
    if shutil.disk_usage(ROOT).free < 1024**3:raise ValueError('at least 1GiB of free workspace required before isolated native staging')
    report=inputs(args,stage)
    (work/'inputs.json').write_text(json.dumps(report,sort_keys=True,indent=2)+'\n')
    env=dict(os.environ)
    for name in ['DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH']:env.pop(name,None)
    docker=['docker','--host=unix:///var/run/docker.sock']
    base_tag='rsh-web4-runtime-'+uuid.uuid4().hex
    run(docker+['image','inspect',args.runtime_image],env=env,stdout=subprocess.DEVNULL)
    # BuildKit treats a bare sha256 image ID in FROM as a registry name. Give
    # the already-present immutable image a private local tag; never pull.
    run(docker+['image','tag',args.runtime_image,base_tag],env=env)
    report['runtime_image']=args.runtime_image
    sources=[args.programs/name for name in report['programs'] if name not in ['nkn-root','nkn-sdk-engine']]
    sources += [args.test_driver,Path('/usr/bin/setpriv'),Path('/usr/bin/getent'),Path('/usr/bin/dbus-daemon')]
    for source in sources:
        result=run(['ldd',str(source)],capture_output=True,text=True)
        for lib in re.findall(r'(?:=>\s+)?(/[^\s]+)\s+\(',result.stdout):
            copy(Path(lib),stage/lib.lstrip('/'))
            if not lib.startswith(('/lib/','/lib64/','/usr/lib/')):
                copy(Path(lib),stage/'usr/lib/x86_64-linux-gnu'/Path(lib).name)
    for source,destination in [('/usr/bin/setpriv','fixture/setpriv'),('/usr/bin/getent','usr/bin/getent'),('/usr/bin/dbus-daemon','usr/bin/dbus-daemon')]:copy(Path(source),stage/destination)
    python=stage/'usr/bin/python3';python.parent.mkdir(parents=True,exist_ok=True);python.symlink_to('/usr/local/bin/python3')
    (stage/'usr/lib/x86_64-linux-gnu').mkdir(parents=True,exist_ok=True)
    for p in [stage,*stage.rglob('*')]:
        if p.is_dir():p.chmod(0o755)
    fix='import os; os.chmod("/usr/lib",0o755); os.chmod("/usr/lib/x86_64-linux-gnu",0o755)'
    (work/'context/Dockerfile').write_text('\n'.join(['FROM '+base_tag,'COPY --chmod=0755 rootfs/ /',
        'RUN '+json.dumps(['/usr/local/bin/python3','-c',fix]),'ENV PATH=/usr/local/bin:/usr/bin:/bin',
        'ENV RSH_WEB4_PRIVATE_CONTAINER=yes','ENTRYPOINT '+json.dumps(['/usr/local/bin/python3','/fixture/entry.py'])])+'\n')
    tag='rustsimhub-web4-chain-'+uuid.uuid4().hex
    volumes=['rsh-web4-'+uuid.uuid4().hex for _ in range(2)]
    image=None
    container='rsh-web4-'+uuid.uuid4().hex
    try:
        run(docker+['build','--pull=false','--network=none','-t',tag,str(work/'context')],env=env,stdout=(work/'image-build.log').open('wb'),stderr=subprocess.STDOUT)
        image=subprocess.check_output(docker+['image','inspect','--format={{.Id}}',tag],env=env,text=True).strip();report['image']=image
        for volume in volumes:run(docker+['volume','create',volume],env=env,stdout=subprocess.DEVNULL)
        mounts=[]
        for volume,paths in [(volumes[0],['/var/lib/rustsimhub/rustsimhub-v1/system/app/versions','/opt/rustsimhub/versions']),
                             (volumes[1],['/var/lib/rustsimhub/rustsimhub-v1/core','/var/lib/rustsimhub-core'])]:
            for path in paths:mounts+=['--mount',f'type=volume,source={volume},target={path}']
        command=docker+['run','--rm','--name',container,'--pull=never','--network=none','--memory=768m','--cpus=2','--pids-limit=128','--cap-add=SYS_PTRACE',
            '--tmpfs=/tmp:rw,nosuid,nodev,size=64m,mode=1777',*mounts,image]
        result=subprocess.run(command,env=env,stdout=(work/'chain.log').open('wb'),stderr=subprocess.STDOUT,timeout=240)
        report.update(exit=result.returncode,passed=result.returncode==0 and {'product':'NATIVE_PRODUCTION_CHAIN_PASSED','default':'NATIVE_DEFAULT_CHAIN_PASSED','email':'NATIVE_EMAIL_CHAIN_PASSED'}[args.scenario] in (work/'chain.log').read_text(errors='replace'),native_architecture_adapter=True,synthetic_pid1_cgroup=True)
    finally:
        subprocess.run(docker+['rm','--force',container],env=env,capture_output=True,timeout=15)
        for volume in volumes:subprocess.run(docker+['volume','rm',volume],env=env,capture_output=True,timeout=15)
        if image:subprocess.run(docker+['image','rm','--no-prune',tag],env=env,capture_output=True,timeout=15)
        tags=json.loads(subprocess.check_output(docker+['image','inspect','--format={{json .RepoTags}}',args.runtime_image],env=env,text=True)) or []
        if len(tags)>1:subprocess.run(docker+['image','rm','--no-prune',base_tag],env=env,capture_output=True,timeout=15)
    return report


def cpio(root,destination):
    def emit(stream,name,mode,data,major=0,minor=0,ino=1):
        name=name.encode()+b'\0';fields=[ino,mode,0,0,1,0,len(data),0,0,major,minor,len(name),0]
        header=b'070701'+b''.join(f'{value:08x}'.encode() for value in fields)
        stream.write(header+name);stream.write(b'\0'*((-len(header+name))%4));stream.write(data);stream.write(b'\0'*((-len(data))%4))
    with gzip.open(destination,'wb',compresslevel=1) as stream:
        for index,path in enumerate(sorted(root.rglob('*')),1):
            relative=path.relative_to(root)
            if relative.parts[0]=='boot':continue
            metadata=path.lstat();data=b''
            if stat.S_ISLNK(metadata.st_mode):data=os.readlink(path).encode()
            elif stat.S_ISREG(metadata.st_mode):data=path.read_bytes()
            elif not stat.S_ISDIR(metadata.st_mode):raise ValueError('unexpected guest input node')
            emit(stream,str(relative),metadata.st_mode,data,ino=index)
        emit(stream,'dev/console',stat.S_IFCHR|0o600,b'',5,1,100000)
        emit(stream,'dev/null',stat.S_IFCHR|0o666,b'',1,3,100001)
        emit(stream,'TRAILER!!!',0,b'',ino=100002)


def arm_vm(args,work):
    if not all([args.guest_root,args.kernel,args.qemu]):raise ValueError('owned ARM dependency root, fixture kernel and QEMU required')
    # The dependency root must be a new owned staging tree, never / or a host
    # installation. No source path from this tree is mounted into the VM.
    root=args.guest_root.resolve(strict=True)
    if root in [Path('/'),Path('/usr'),Path('/var')] or not root.is_relative_to(ROOT/'.local'):raise ValueError('private guest staging root required')
    report=inputs(args,root)
    for p in ['proc','sys','dev','tmp','run','etc','var','usr/bin','usr/local/bin','opt/rustsimhub','sys/fs/cgroup']:(root/p).mkdir(parents=True,exist_ok=True)
    for name,target in [('bin','usr/bin'),('sbin','usr/sbin'),('lib','usr/lib')]:
        if not (root/name).exists():(root/name).symlink_to(target)
    if not (root/'usr/bin/python3').exists():(root/'usr/bin/python3').symlink_to('python3.13')
    copy(root/'usr/bin/setpriv',root/'fixture/setpriv')
    init='''#!/bin/busybox sh
set -eu
/bin/busybox mount -t proc proc /proc
/bin/busybox mount -t sysfs sysfs /sys
/bin/busybox mount -t devtmpfs devtmpfs /dev
/bin/busybox mount -t cgroup2 none /sys/fs/cgroup
/bin/busybox ip link set lo up
/bin/busybox mkdir -p /sys/fs/cgroup/system.slice /var/lib/rustsimhub/rustsimhub-v1/core /var/lib/rustsimhub-core /var/lib/rustsimhub/rustsimhub-v1/system/app/versions /opt/rustsimhub/versions
/bin/busybox mount -o bind /var/lib/rustsimhub/rustsimhub-v1/core /var/lib/rustsimhub-core
/bin/busybox mount -o bind /var/lib/rustsimhub/rustsimhub-v1/system/app/versions /opt/rustsimhub/versions
export PATH=/usr/bin:/bin:/usr/local/bin RSH_WEB4_PRIVATE_CONTAINER=yes RSH_WEB4_FORMAL_CLI=yes
/usr/bin/python3 /fixture/entry.py || /bin/busybox echo RSH_WEB4_ARM_VM_FAILED
/bin/busybox poweroff -f
'''
    (root/'init').write_text(init);(root/'init').chmod(0o755)
    for path in [root,*root.rglob('*')]:
        if path.is_dir() and not path.is_symlink():path.chmod(0o755)
    (root/'tmp').chmod(0o1777)
    image=work/'fixture.cpio.gz';cpio(root,image)
    command=[str(args.qemu),'-accel','tcg,thread=multi','-machine','virt,gic-version=3','-cpu','max','-smp','2','-m','1024M',
        '-nographic','-monitor','none','-serial','stdio','-nic','none','-kernel',str(args.kernel),'-initrd',str(image),
        '-append','console=ttyAMA0 rdinit=/init panic=-1 random.trust_cpu=on']
    result=subprocess.run(command,stdout=(work/'chain.log').open('wb'),stderr=subprocess.STDOUT,timeout=420)
    log=(work/'chain.log').read_text(errors='replace')
    report.update(exit=result.returncode,passed=result.returncode==0 and ('FORMAL_ARM_PRODUCTION_CHAIN_PASSED' if args.scenario=='product' else 'FORMAL_ARM_DEFAULT_CHAIN_PASSED') in log,
        kernel_sha256=sha(args.kernel),initramfs_sha256=sha(image),native_architecture_adapter=False,
        generic_fixture_kernel=True,production_firmware_bootable=False,synthetic_pid1=True,actual_guest_cgroups=True)
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode',choices=['native','arm-vm'],required=True)
    parser.add_argument('--legacy-driver',type=Path)
    parser.add_argument('--programs',type=Path,required=True);parser.add_argument('--test-driver',type=Path,required=True)
    parser.add_argument('--scenario',choices=['product','default','email'],default='product')
    parser.add_argument('--revision',required=True);parser.add_argument('--runtime-image')
    for name in ['guest-root','kernel','qemu']:parser.add_argument('--'+name,type=Path)
    args=parser.parse_args()
    if args.mode=='arm-vm' and args.scenario=='email':parser.error('email currently uses the isolated native API driver; ARM default/product use formal CLI')
    work=ROOT/'.local'/'production-chain'/uuid.uuid4().hex;work.mkdir(parents=True,mode=0o700)
    try:report=native(args,work) if args.mode=='native' else arm_vm(args,work)
    except (ValueError,OSError,subprocess.SubprocessError) as error:
        report={'passed':False,'failure':type(error).__name__,'detail':str(error),'network':False,'hardware_access':False}
    if args.mode=='native' and (work/'context/rootfs').exists():shutil.rmtree(work/'context/rootfs')
    (work/'result.json').write_text(json.dumps(report,sort_keys=True,indent=2)+'\n')
    print(json.dumps({'passed':report['passed'],'evidence':str(work.relative_to(ROOT))},sort_keys=True))
    raise SystemExit(0 if report['passed'] else 1)


if __name__=='__main__':main()
