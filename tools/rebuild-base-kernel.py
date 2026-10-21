#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Rebuild the fixed upstream base in a NEW output; never accesses a target or writes boot."""
import argparse
import datetime
import hashlib
import io
import json
import lzma
import os
import pathlib
import re
import stat
import struct
import subprocess
import tarfile
import time
import uuid
import zlib

ROOT=pathlib.Path(__file__).resolve().parents[1]
MAX_FILE=128*1024*1024
MAX_TOTAL=3*1024*1024*1024
MAX_ENTRIES=200000


def require(condition, reason):
    if not condition:raise ValueError(reason)


def regular_bytes(path, maximum=MAX_FILE):
    named_before=path.lstat()
    require(stat.S_ISREG(named_before.st_mode) and named_before.st_size<=maximum,'Expected bounded regular input before opening')
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
    try:
        before=os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_size<=maximum,'Expected bounded regular input')
        with os.fdopen(os.dup(fd),'rb') as file:data=file.read(maximum+1)
        after=os.fstat(fd);named=path.lstat()
        def identity(info):return info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns,info.st_ctime_ns
        require(len(data)==before.st_size and identity(named_before)==identity(before)==identity(after)==identity(named),'Input changed while reading')
        return data
    finally:os.close(fd)


def source_tree(directory):
    count=total=0
    def visit(path,depth=0):
        nonlocal count,total
        require(depth<=128,'Source nesting too deep');rows=[]
        for child in path.iterdir():
            count+=1;require(count<=MAX_ENTRIES,'Too many source entries')
            info=child.lstat();name=os.fsencode(child.name)
            if stat.S_ISDIR(info.st_mode):mode=b'40000';digest=visit(child,depth+1);key=name+b'/'
            else:
                if stat.S_ISLNK(info.st_mode):
                    mode=b'120000';data=os.fsencode(os.readlink(child));require(len(data)<=16384,'Source link too long')
                else:
                    require(stat.S_ISREG(info.st_mode),'Special source file refused')
                    mode=b'100755' if info.st_mode&0o111 else b'100644';data=regular_bytes(child)
                total+=len(data);require(total<=MAX_TOTAL,'Source too large')
                digest=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).digest();key=name
            rows.append((key,mode+b' '+name+b'\0'+digest))
        data=b''.join(value for _,value in sorted(rows))
        return hashlib.sha1(b'tree '+str(len(data)).encode()+b'\0'+data).digest()
    return visit(directory).hex()


def build_path(path, directory=True):
    require(path.is_absolute() and (path.resolve()==path if directory else path.parent.resolve()==path.parent),'Canonical absolute build path required')
    require(re.fullmatch(r'/[A-Za-z0-9_./+\-]+',str(path)),'Build paths must have no whitespace or shell syntax')
    require(path.is_dir() if directory else path.is_file(),'Build input is missing')
    return path


def metadata_makefile(profile):
    require(profile['temporary_version_timestamp']=='','Only the fixed empty weak timestamp is supported')
    epoch=profile['builtin_cpio_epoch'];require(type(epoch) is int and 0<=epoch<=0xffffffff,'Invalid cpio epoch')
    timestamp=datetime.datetime.fromtimestamp(epoch,datetime.timezone.utc).strftime('%a %b %d %H:%M:%S UTC %Y')
    return ('init/utsversion-tmp.h: private KBUILD_BUILD_TIMESTAMP =\n'
            'usr/initramfs_data.cpio: private KBUILD_BUILD_TIMESTAMP = '+timestamp+'\n')


def reference_kernel(path, lock):
    expected=next(row for row in lock['assets'] if row['name']=='boot.img')
    data=regular_bytes(path,64*1024*1024)
    require(len(data)==expected['bytes'] and hashlib.sha256(data).hexdigest()==expected['sha256'],'Wrong reference boot input')
    fields=struct.unpack_from('<8s10I',data)
    require(fields[0]==b'ANDROID!' and fields[8]>=512 and fields[8]+fields[1]<=len(data),'Invalid fixed Android boot layout')
    payload=data[fields[8]:fields[8]+fields[1]];decoder=zlib.decompressobj(31)
    image=decoder.decompress(payload,64*1024*1024)
    require(decoder.eof and not decoder.unconsumed_tail,'Invalid or oversized reference kernel')
    return image,payload[:len(payload)-len(decoder.unused_data)]


def debian_data_member(data):
    require(data.startswith(b'!<arch>\n'),'Expected Debian ar package')
    offset=8;found=[]
    while offset<len(data):
        require(offset+60<=len(data),'Truncated ar header')
        header=data[offset:offset+60];require(header[58:]==b'`\n','Invalid ar header')
        name=header[:16].decode('ascii').strip().removesuffix('/')
        size_text=header[48:58].strip();require(size_text.isdigit(),'Invalid ar member length')
        size=int(size_text);start=offset+60;end=start+size
        require(end<=len(data),'Ar member outside package')
        if name.startswith('data.tar'):found.append((name,data[start:end]))
        offset=end+(size%2)
    require(offset==len(data),'Missing ar padding')
    require(len(found)==1 and found[0][0] in ('data.tar.xz','data.tar.gz','data.tar'),'Missing/duplicate/unsupported Debian data archive')
    return found[0][1]


def module_archive(data,release):
    rows={};total=0
    with tarfile.open(fileobj=io.BytesIO(data),mode='r|*') as archive:
        for count,member in enumerate(archive,1):
            total+=member.size
            require(count<=10000 and 0<=member.size<=MAX_FILE and total<=MAX_TOTAL,'Module package bounds exceeded')
            match=re.fullmatch(r'\./(?:usr/)?lib/modules/'+re.escape(release)+r'/kernel/([A-Za-z0-9_+./-]+\.ko)(\.xz)?',member.name)
            if not match:
                require(not member.name.endswith(('.ko','.ko.xz')),'Unexpected reference module pathname')
                continue
            relative=pathlib.PurePosixPath(match[1])
            require(not relative.is_absolute() and '..' not in relative.parts and str(relative)==match[1],'Unsafe module pathname')
            require(member.isfile() and match[1] not in rows,'Linked or duplicate module refused')
            payload=archive.extractfile(member).read(32*1024*1024+1)
            require(len(payload)<=32*1024*1024,'Module too large')
            if match[2]:
                decoder=lzma.LZMADecompressor(memlimit=128*1024*1024)
                payload=decoder.decompress(payload,max_length=32*1024*1024+1)
                require(decoder.eof and not decoder.unused_data and len(payload)<=32*1024*1024,'Invalid or oversized compressed module')
            require(payload[:6]==b'\x7fELF\x02\x01' and len(payload)>=64 and struct.unpack_from('<HH',payload,16)==(1,183),'Expected ARM64 relocatable module')
            rows[match[1]]={'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest()}
    require(rows,'No reference modules found')
    return rows


def reference_modules(path,lock,release):
    expected=next(row for row in lock['assets'] if row['name'].startswith('linux-image-'))
    data=regular_bytes(path)
    require(len(data)==expected['bytes'] and hashlib.sha256(data).hexdigest()==expected['sha256'],'Wrong reference module package')
    return module_archive(debian_data_member(data),release)


def save(path,value):
    temporary=path.with_name('.report-'+uuid.uuid4().hex)
    with temporary.open('x',encoding='utf-8') as file:
        json.dump(value,file,indent=2);file.flush();os.fsync(file.fileno())
    temporary.chmod(0o600);os.replace(temporary,path)


def rebuild(args):
    profile=json.loads((ROOT/'firmware/kernel/base-build-profile.json').read_text())
    lock=json.loads((ROOT/'firmware/kernel/upstream-source.lock.json').read_text())
    require(profile.get('format')==1 and profile.get('kind')=='fixed-upstream-base-kernel-reproduction','Unsupported reproduction profile')
    require(profile['source_commit']==lock['commit'] and profile['source_tree']==lock['commit_tree'],'Profile/source lock mismatch')
    source=build_path(args.source)
    require(source_tree(source)==profile['source_tree'],'Source differs from the fixed complete Git tree')
    config=regular_bytes(args.config,1024*1024)
    require(hashlib.sha256(config).hexdigest()==profile['configuration_sha256'],'Wrong fixed kernel configuration')
    original,compressed=reference_kernel(args.reference_boot,lock)
    require(len(original)==profile['image_bytes'] and len(compressed)==profile['compressed_bytes'] and hashlib.sha256(original).hexdigest()==profile['image_sha256'] and hashlib.sha256(compressed).hexdigest()==profile['compressed_sha256'],'Profile/reference kernel mismatch')
    modules=reference_modules(args.image_package,lock,profile['kernel_release'])
    require(len(modules)==profile['stock_module_count'],'Unexpected reference module count')
    for path in [args.toolchain_bin,args.sysroot,args.host_prefix,args.host_runtime,args.toolchain_runtime]:build_path(path)
    build_path(args.make,directory=False);build_path(args.host_cc,directory=False);build_path(args.host_cxx,directory=False)
    require(1<=args.jobs<=32,'Jobs must be between 1 and 32')
    require(args.output.is_absolute() and args.output.resolve()==args.output and not args.output.exists() and not args.output.is_symlink(),'New canonical output required')
    build_path(args.output.parent)
    require(re.fullmatch(r'/[A-Za-z0-9_./+\-]+',str(args.output)),'Unsafe output path for make')
    require(not args.output.is_relative_to(source) and not source.is_relative_to(args.output),'Source and output must be separate')
    args.output.mkdir(mode=0o700);args.output.chmod(0o700)
    report={'format':1,'kind':'independent-fixed-base-kernel-rebuild','passed':False,'phase':'preparing',
            'source_commit':lock['commit'],'source_tree':profile['source_tree'],'source_unchanged':False,
            'target_access':False,'network_access':False,'boot_image_generated':False,'partition_writes':False,
            'flash_allowed':False,'release_ready':False,'recovery_verified':False,'board_boot_verified':False,
            'profile_sha256':hashlib.sha256((ROOT/'firmware/kernel/base-build-profile.json').read_bytes()).hexdigest(),
            'builder_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
            'compiler_binary_identity_verified':False,'historical_metadata_used_for_reproduction_only':True}
    out=args.output/'BUILD-REPORT.json';started=time.monotonic()
    try:
        save(out,report);build=args.output/'build';build.mkdir(mode=0o700);(build/'.config').write_bytes(config)
        metadata=args.output/'metadata.mk';metadata.write_text(metadata_makefile(profile))
        helper=args.output/'host-helper-runtime';helper.mkdir(mode=0o700)
        for file in args.host_runtime.glob('libsigsegv.so*'):
            require(file.is_file(),'Missing host helper library');(helper/file.name).symlink_to(file.resolve())
        env={'PATH':str(args.toolchain_bin)+':'+str(args.host_prefix/'usr/bin')+':/usr/bin:/bin','LC_ALL':'C','LANG':'C','TZ':'UTC',
             'HOME':str(args.output),'PYTHONDONTWRITEBYTECODE':'1','MAKEFILES':str(metadata),
             'LD_LIBRARY_PATH':str(args.toolchain_runtime)+':'+str(helper),
             'PKG_CONFIG_SYSROOT_DIR':str(args.host_prefix),'PKG_CONFIG_LIBDIR':str(args.host_prefix/'usr/lib/x86_64-linux-gnu/pkgconfig'),
             'BISON_PKGDATADIR':str(args.host_prefix/'usr/share/bison'),'M4':str(args.host_prefix/'usr/bin/m4'),
             'KBUILD_BUILD_USER':profile['build_user'],'KBUILD_BUILD_HOST':profile['build_host'],
             'KBUILD_BUILD_VERSION':profile['build_version'],'KBUILD_BUILD_TIMESTAMP':profile['final_timestamp']}
        gcc=args.toolchain_bin/'aarch64-linux-gnu-gcc';ld=args.toolchain_bin/'aarch64-linux-gnu-ld';objcopy=args.toolchain_bin/'aarch64-linux-gnu-objcopy'
        for key,path,wanted in [('compiler_version',gcc,profile['compiler_version']),('linker_version',ld,profile['linker_version']),('objcopy_version',objcopy,profile['objcopy_version'])]:
            seen=subprocess.check_output([str(path),'--version'],env=env,text=True,timeout=15).splitlines()[0]
            report[key]=seen;require(seen==wanted,'Toolchain version differs from fixed profile')
        report['selected_executable_sha256']={name:hashlib.sha256(regular_bytes(path.resolve())).hexdigest()
            for name,path in [('compiler_driver',gcc),('linker',ld),('objcopy',objcopy),('make',args.make),('host_compiler',args.host_cc)]}
        report['host_compiler_version']=subprocess.check_output([str(args.host_cc),'--version'],env=env,text=True,timeout=15).splitlines()[0]
        command=[str(args.make),'-C',str(source),'O='+str(build),'ARCH=arm64','LOCALVERSION='+profile['localversion'],
                 'CROSS_COMPILE='+str(args.toolchain_bin/'aarch64-linux-gnu-'),'CC='+str(gcc)+' --sysroot='+str(args.sysroot),
                 'HOSTCC='+str(args.host_cc),'HOSTCXX='+str(args.host_cxx),
                 'HOSTCFLAGS=-O2 -std=gnu11 -isystem '+str(args.host_prefix/'usr/include')+' -isystem '+str(args.host_prefix/'usr/include/x86_64-linux-gnu'),
                 'HOSTLDFLAGS=-L'+str(args.host_prefix/'usr/lib/x86_64-linux-gnu')+' -Wl,-rpath,'+str(args.host_runtime),
                 'YACC='+str(args.host_prefix/'usr/bin/bison'),'LEX='+str(args.host_prefix/'usr/bin/flex')]
        report.update(phase='configuring',command=command,jobs=args.jobs);save(out,report)
        with (args.output/'build.log').open('x') as log:
            subprocess.run([*command,'olddefconfig'],env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=120)
            require((build/'.config').read_bytes()==config,'Configuration drift; refusing compilation')
            release=subprocess.check_output([*command,'--no-print-directory','-s','kernelrelease'],env=env,text=True,timeout=60).strip()
            require(release==profile['kernel_release'],'Kernel release changed')
            report.update(phase='compiling',kernel_release=release);save(out,report)
            subprocess.run([*command,'-j'+str(args.jobs),'Image.gz','modules','dtbs'],env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=3600)
        outputs={}
        for name in ['arch/arm64/boot/Image','arch/arm64/boot/Image.gz','vmlinux','Module.symvers']:
            data=regular_bytes(build/name)
            outputs[name]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
        report.update(outputs=outputs,stock_module_count=len(list(build.rglob('*.ko'))))
        report['image_byte_identical']=regular_bytes(build/'arch/arm64/boot/Image')==original
        report['compressed_kernel_byte_identical']=regular_bytes(build/'arch/arm64/boot/Image.gz')==compressed
        save(out,report)
        require(report['image_byte_identical'] and report['compressed_kernel_byte_identical'],'Kernel reproduction differs; retain failed output')
        require(report['stock_module_count']==profile['stock_module_count'],'Unexpected stock module count')
        require({str(path.relative_to(build)) for path in build.rglob('*.ko')}==set(modules),'Built/reference module path sets differ')
        packaged=args.output/'packaged-modules';packaged.mkdir(mode=0o700);module_rows=[]
        for relative,wanted in sorted(modules.items()):
            source_module=build/relative;before=regular_bytes(source_module)
            output_module=packaged/relative;output_module.parent.mkdir(parents=True,exist_ok=True)
            subprocess.run([str(objcopy),'--strip-debug',str(source_module),str(output_module)],env=env,check=True,timeout=15)
            data=regular_bytes(output_module)
            row={'path':relative,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
            row['byte_identical']=row['bytes']==wanted['bytes'] and row['sha256']==wanted['sha256']
            report['latest_module_packaging']=row;save(out,report)
            require(row['byte_identical'] and regular_bytes(source_module)==before,'Module packaging differs or raw output changed')
            module_rows.append(row)
        save(args.output/'PACKAGED-MODULES.json',{'passed':True,'raw_modules_unchanged':True,'modules':module_rows})
        report.update(packaged_modules_byte_identical=True,packaged_module_count=len(module_rows))
        require(source_tree(source)==profile['source_tree'],'Source changed during build')
        report.update(passed=True,phase='byte-identical-base-rebuild-no-boot-or-flash',source_unchanged=True)
        print('Fixed base kernel and packaged stock modules rebuilt byte-identically; no target access, boot image or flash operation')
    except BaseException as exc:
        report.update(passed=False,phase='failed-output-retained',error=str(exc));raise
    finally:
        try:report['source_unchanged']=source_tree(source)==profile['source_tree']
        except BaseException as exc:report.update(source_unchanged=False,source_check_error=str(exc))
        if not report['source_unchanged']:report.update(passed=False,phase='failed-source-changed')
        report['elapsed_seconds']=round(time.monotonic()-started,3);save(out,report)
    require(report['passed'],'Final source verification failed; output retained')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['source','config','reference-boot','image-package','toolchain-bin','sysroot','host-prefix','host-runtime','toolchain-runtime','make','output']:
        parser.add_argument('--'+name,type=pathlib.Path,required=True)
    parser.add_argument('--host-cc',type=pathlib.Path,default=pathlib.Path('/usr/bin/gcc'))
    parser.add_argument('--host-cxx',type=pathlib.Path,default=pathlib.Path('/usr/bin/g++'))
    parser.add_argument('--jobs',type=int,default=12)
    rebuild(parser.parse_args())


if __name__=='__main__':main()
