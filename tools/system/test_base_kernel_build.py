# SPDX-License-Identifier: AGPL-3.0-only
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
import pathlib
import shutil
import struct
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('base_rebuild',ROOT/'tools/rebuild-base-kernel.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


def tar_modules(rows):
    output=io.BytesIO()
    with tarfile.open(fileobj=output,mode='w') as archive:
        for name,payload,kind in rows:
            member=tarfile.TarInfo(name);member.size=len(payload)
            if kind=='link':member.type=tarfile.SYMTYPE;member.linkname='outside';member.size=0
            archive.addfile(member,None if kind=='link' else io.BytesIO(payload))
    return output.getvalue()


def debian_member(name,payload):
    header=((name+'/').ljust(16)+'0'.ljust(12)+'0'.ljust(6)+'0'.ljust(6)+'100644'.ljust(8)+str(len(payload)).ljust(10)+'`\n').encode()
    return header+payload+(b'\n' if len(payload)%2 else b'')


@unittest.skipIf(os.name=='nt','Unix source modes, symlinks and build paths')
class BaseKernelBuildTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='rsh-base-build-');self.base=pathlib.Path(self.temp.name)
        self.source=self.base/'source';self.source.mkdir();(self.source/'source.c').write_bytes(b'synthetic source\n')
        self.repo=self.base/'repo';(self.repo/'firmware/kernel').mkdir(parents=True)
        self.config=self.base/'config';self.config.write_bytes(b'CONFIG_SYNTHETIC=y\n')
        image=b'bounded synthetic reference';compressed=gzip.compress(image,mtime=0)
        fields=(b'ANDROID!',len(compressed),0,0,0,0,0,0,2048,0,0)
        self.boot=self.base/'boot';self.boot.write_bytes(struct.pack('<8s10I',*fields).ljust(2048,b'\0')+compressed)
        self.profile=json.loads((ROOT/'firmware/kernel/base-build-profile.json').read_text())
        self.profile.update(source_tree=module.source_tree(self.source),configuration_sha256=hashlib.sha256(self.config.read_bytes()).hexdigest(),
                            image_bytes=len(image),compressed_bytes=len(compressed),
                            image_sha256=hashlib.sha256(image).hexdigest(),compressed_sha256=hashlib.sha256(compressed).hexdigest())
        self.module=bytearray(64);self.module[:6]=b'\x7fELF\x02\x01';struct.pack_into('<HH',self.module,16,1,183)
        self.module=bytes(self.module);self.module_name='./lib/modules/'+self.profile['kernel_release']+'/kernel/synthetic.ko'
        package=b'!<arch>\n'+debian_member('data.tar',tar_modules([(self.module_name,self.module,'file')]))
        self.package=self.base/'modules.deb';self.package.write_bytes(package);self.profile['stock_module_count']=1
        self.lock={'commit':self.profile['source_commit'],'commit_tree':self.profile['source_tree'],
                   'assets':[{'name':'boot.img','bytes':self.boot.stat().st_size,'sha256':hashlib.sha256(self.boot.read_bytes()).hexdigest()},
                             {'name':'linux-image-synthetic.deb','bytes':len(package),'sha256':hashlib.sha256(package).hexdigest()}]}
        (self.repo/'firmware/kernel/base-build-profile.json').write_text(json.dumps(self.profile))
        (self.repo/'firmware/kernel/upstream-source.lock.json').write_text(json.dumps(self.lock))
        self.bin=self.base/'bin';self.bin.mkdir();self.make=self.bin/'make';self.canary=self.base/'make-called'
        self.make.write_text('#!/bin/sh\ntouch '+str(self.canary)+'\nexit 9\n');self.make.chmod(0o755)
        self.cc=self.bin/'aarch64-linux-gnu-gcc';self.cc.write_text('#!/bin/sh\necho wrong-compiler\n');self.cc.chmod(0o755)
        self.args=argparse.Namespace(source=self.source,config=self.config,reference_boot=self.boot,toolchain_bin=self.bin,
              image_package=self.package,
              sysroot=self.base,host_prefix=self.base,host_runtime=self.base,toolchain_runtime=self.base,
              make=self.make,host_cc=self.make,host_cxx=self.make,output=self.base/'output',jobs=2)

    def tearDown(self):self.temp.cleanup()

    def run_build(self):
        with patch.object(module,'ROOT',self.repo):return module.rebuild(self.args)

    def test_source_hash_matches_real_git_including_executable_and_link_modes(self):
        if not shutil.which('git'):self.skipTest('git required')
        (self.source/'script').write_text('#!/bin/sh\n');(self.source/'script').chmod(0o755)
        (self.source/'link').symlink_to('source.c')
        expected=module.source_tree(self.source)
        subprocess.run(['git','init','--quiet',str(self.source)],check=True)
        subprocess.run(['git','-C',str(self.source),'add','.'],check=True)
        actual=subprocess.check_output(['git','-C',str(self.source),'write-tree'],text=True).strip()
        self.assertEqual(expected,actual)

    def test_bad_source_config_or_reference_never_creates_output_or_executes_make(self):
        for file,data in [(self.source/'source.c',b'changed'),(self.config,b'wrong'),(self.boot,b'wrong'),(self.package,b'wrong')]:
            original=file.read_bytes();file.write_bytes(data)
            with self.assertRaises(ValueError):self.run_build()
            self.assertFalse(self.args.output.exists());self.assertFalse(self.canary.exists())
            file.write_bytes(original)

    def test_existing_output_and_source_nested_output_are_refused_without_mutation(self):
        self.args.output.mkdir();marker=self.args.output/'keep';marker.write_text('keep')
        with self.assertRaises(ValueError):self.run_build()
        self.assertEqual(marker.read_text(),'keep');self.assertFalse(self.canary.exists())
        self.args.output=self.source/'nested'
        with self.assertRaises(ValueError):self.run_build()
        self.assertFalse(self.args.output.exists());self.assertFalse(self.canary.exists())

    def test_wrong_toolchain_retains_failed_report_and_never_compiles(self):
        with self.assertRaisesRegex(ValueError,'Toolchain version'):self.run_build()
        report=json.loads((self.args.output/'BUILD-REPORT.json').read_text())
        self.assertFalse(report['passed']);self.assertTrue(report['source_unchanged'])
        self.assertEqual(report['phase'],'failed-output-retained');self.assertFalse(self.canary.exists())
        self.assertFalse(any(report[key] for key in ['target_access','network_access','partition_writes','flash_allowed','release_ready']))

    def test_compiler_failure_records_source_mutation_without_rewriting_source_or_retrying(self):
        self.cc.write_text('#!/bin/sh\nprintf "changed" >> '+str(self.source/'source.c')+'\nprintf "%s\\n" '+repr(self.profile['compiler_version'])+'\n')
        ld=self.bin/'aarch64-linux-gnu-ld';ld.write_text('#!/bin/sh\nprintf "%s\\n" '+repr(self.profile['linker_version'])+'\n');ld.chmod(0o755)
        objcopy=self.bin/'aarch64-linux-gnu-objcopy';objcopy.write_text('#!/bin/sh\nprintf "%s\\n" '+repr(self.profile['objcopy_version'])+'\n');objcopy.chmod(0o755)
        host=self.bin/'host';host.write_text('#!/bin/sh\necho synthetic-host\n');host.chmod(0o755)
        self.args.host_cc=self.args.host_cxx=host
        with self.assertRaises(subprocess.CalledProcessError):self.run_build()
        report=json.loads((self.args.output/'BUILD-REPORT.json').read_text())
        self.assertFalse(report['passed']);self.assertFalse(report['source_unchanged'])
        self.assertEqual(report['phase'],'failed-source-changed');self.assertTrue(self.canary.exists())
        self.assertTrue((self.source/'source.c').read_bytes().endswith(b'changed'))

    def test_symlink_inputs_special_sources_and_shell_paths_are_refused(self):
        link=self.base/'link';link.symlink_to(self.config)
        with self.assertRaises(ValueError):module.regular_bytes(link)
        fifo=self.source/'fifo';os.mkfifo(fifo)
        with patch.object(module.os,'open',side_effect=AssertionError('Nonregular input must not be opened')):
            with self.assertRaises(ValueError):module.regular_bytes(fifo)
        with self.assertRaises(ValueError):module.source_tree(self.source)
        unsafe=self.base/'shell;syntax';unsafe.mkdir()
        with self.assertRaises(ValueError):module.build_path(unsafe)
        with patch.object(module,'MAX_ENTRIES',0):
            with self.assertRaises(ValueError):module.source_tree(self.source)

    def test_ar_module_reader_rejects_duplicate_linked_foreign_and_unsafe_members_without_extracting(self):
        valid=(self.module_name,self.module,'file');release=self.profile['kernel_release']
        rows=module.module_archive(tar_modules([valid]),release)
        self.assertEqual(rows['synthetic.ko']['sha256'],hashlib.sha256(self.module).hexdigest())
        foreign=bytearray(self.module);struct.pack_into('<H',foreign,18,62)
        cases=[[valid,valid],[(self.module_name,b'','link')],[(self.module_name.replace('synthetic.ko','../escape.ko'),self.module,'file')],
               [(self.module_name.replace(release,'other-release'),self.module,'file')],[(self.module_name,b'not-elf','file')]]
        cases.append([(self.module_name,bytes(foreign),'file')])
        for case in cases:
            with self.subTest(case=case[0][0]):
                with self.assertRaises(ValueError):module.module_archive(tar_modules(case),release)
        archive=tar_modules([valid]);one=debian_member('data.tar',archive)
        self.assertEqual(module.debian_data_member(b'!<arch>\n'+one),archive)
        for broken in [b'!<arch>\n'+one+one,b'!<arch>\n'+one[:-1],b'not-debian']:
            with self.assertRaises(ValueError):module.debian_data_member(broken)

    @unittest.skipUnless(shutil.which('make'),'GNU make required for actual metadata-scoping regression')
    def test_metadata_only_affects_temporary_header_and_builtin_cpio_targets(self):
        metadata=self.base/'metadata.mk';metadata.write_text(module.metadata_makefile(self.profile))
        recipe=self.base/'Makefile'
        recipe.write_text('all: init/utsversion-tmp.h usr/initramfs_data.cpio final\n'
                          'init/utsversion-tmp.h:\n\t@echo "weak|$(KBUILD_BUILD_TIMESTAMP)"\n'
                          'usr/initramfs_data.cpio:\n\t@echo "cpio|$(KBUILD_BUILD_TIMESTAMP)"\n'
                          'final:\n\t@echo "final|$(KBUILD_BUILD_TIMESTAMP)"\n')
        lines=subprocess.check_output(['make','--no-print-directory','-f',str(recipe),'all'],cwd=self.base,
            env={**os.environ,'MAKEFILES':str(metadata),'KBUILD_BUILD_TIMESTAMP':self.profile['final_timestamp']},text=True).splitlines()
        self.assertEqual(lines[0],'weak|')
        self.assertEqual(lines[1],'cpio|Thu Oct 02 04:19:58 UTC 2025')
        self.assertEqual(lines[2],'final|'+self.profile['final_timestamp'])
