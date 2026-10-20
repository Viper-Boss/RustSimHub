# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import json
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
import audio_modules as audio


def elf(info):
    names = b'\0.shstrtab\0.modinfo\0'
    payload = b'\0'.join(k.encode()+b'='+v.encode() for k, v in info.items())+b'\0'
    table = 64+len(names)+len(payload)
    header = bytearray(64)
    header[:6] = b'\x7fELF\x02\x01'
    struct.pack_into('<HH', header, 16, 1, 183)
    struct.pack_into('<Q', header, 40, table)
    struct.pack_into('<HHH', header, 58, 64, 3, 1)
    sections = bytes(64)+struct.pack('<IIQQQQIIQQ',1,3,0,0,64,len(names),0,0,1,0)
    sections += struct.pack('<IIQQQQIIQQ',11,1,0,0,64+len(names),len(payload),0,0,1,0)
    return bytes(header)+names+payload+sections


class AudioModuleSourceTests(unittest.TestCase):
    def test_pinned_preloads_include_exact_16_audio_identities(self):
        value,_ = audio.profile()
        rows = audio.loaded_rows(value)
        self.assertEqual(len({r['modinfo']['name'] for r in rows}),16)
        self.assertIn('q6voice_dai',{r['modinfo']['name'] for r in rows})
        self.assertIn('snd_soc_apq8016_sbc',{r['modinfo']['name'] for r in rows})
        with self.assertRaises(ValueError): audio.loaded_rows({'expected_modules':[]})

    def test_pinned_source_digest_and_source_symlink_are_checked(self):
        audio.profile()
        with tempfile.TemporaryDirectory() as temporary:
            copy = pathlib.Path(temporary)/'source'
            shutil.copytree(audio.SOURCE, copy)
            target = copy/'nassimhub-incall-audio.patch'
            target.write_bytes(target.read_bytes()+b'\n')
            with self.assertRaises(ValueError): audio.profile(copy)
            target.unlink();target.symlink_to(audio.SOURCE/'nassimhub-incall-audio.patch')
            with self.assertRaises(ValueError): audio.profile(copy)

    def test_exact_module_set_refuses_extra_missing_changed_symlink_and_hardlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            info = {'name':'synthetic','vermagic':audio.RELEASE,'srcversion':'A'*24,'depends':''}
            data = elf(info)
            row = {'name':'synthetic.ko','bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'modinfo':info}
            value = {'expected_modules':[row]}
            path = directory/'synthetic.ko';path.write_bytes(data)
            audio.verify_artifacts(directory, value)
            (directory/'extra').write_bytes(b'extra')
            with self.assertRaises(ValueError): audio.verify_artifacts(directory, value)
            (directory/'extra').unlink();path.unlink()
            with self.assertRaises(ValueError): audio.verify_artifacts(directory, value)
            path.write_bytes(data+b'changed')
            with self.assertRaises(ValueError): audio.verify_artifacts(directory, value)
            path.unlink();path.symlink_to(audio.SOURCE/'Makefile.outoftree')
            with self.assertRaises(ValueError): audio.verify_artifacts(directory, value)
            path.unlink();path.write_bytes(data)
            link = directory.parent/(directory.name+'-hardlink')
            try:
                os.link(path, link)
                with self.assertRaises(ValueError): audio.verify_artifacts(directory, value)
            finally: link.unlink(missing_ok=True)
            wrong = {**value, 'expected_modules':[{**row, 'modinfo':{**info, 'srcversion':'B'*24}}]}
            with self.assertRaises(ValueError): audio.verify_artifacts(directory, wrong)

    def test_elf_architecture_and_section_bounds_checked(self):
        data = elf({'name':'synthetic','vermagic':audio.RELEASE,'srcversion':'A'*24,'depends':''})
        self.assertEqual(audio.modinfo(data)['name'], 'synthetic')
        wrong = bytearray(data);struct.pack_into('<H', wrong, 18, 62)
        with self.assertRaises(ValueError): audio.modinfo(wrong)
        wrong = bytearray(data);struct.pack_into('<Q', wrong, 40, len(data)+1)
        with self.assertRaises(ValueError): audio.modinfo(wrong)
        with self.assertRaises(ValueError): audio.modinfo(data[:64])

    def test_destination_never_follows_overlay_symlink_or_overwrites_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root/'alias').symlink_to(root, target_is_directory=True)
            with self.assertRaises(ValueError): audio.target(root, 'alias/escape/file')
            with self.assertRaises(ValueError): audio.target(root, '../outside')
            (root/'existing').write_text('preserve')
            with self.assertRaises(ValueError): audio.target(root, 'existing', new=True)
            self.assertEqual((root/'existing').read_text(), 'preserve')

    def test_invalid_artifacts_fail_before_any_tree_change(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)/'tree';root.mkdir()
            artifacts = pathlib.Path(temporary)/'empty';artifacts.mkdir()
            with self.assertRaises(ValueError): audio.install(root, artifacts)
            self.assertEqual(list(root.iterdir()), [])

    def test_stock_selection_or_multiple_alias_cannot_claim_override_resolution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            prefix = root/'lib/modules'/audio.RELEASE
            path = prefix/'kernel/q6voice.ko';path.parent.mkdir(parents=True);path.write_bytes(b'stock')
            value = {'expected_modules':[{'name':'q6voice.ko','sha256':'0'*64}]}
            def query(*args):
                self.assertIn('/dev/null', args)
                return 'insmod '+str(path)+'\n'
            with patch.object(audio, 'run', side_effect=query):
                with self.assertRaises(ValueError): audio.dependencies(root, value)
            with patch.object(audio, 'run', return_value='q6voice\nforeign\n'):
                with self.assertRaises(ValueError): audio.dependencies(root, {'expected_modules':[]})


@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0, 'Linux root ownership guard fixture')
class AudioDriverGuardTests(unittest.TestCase):
    def test_fixed_loader_and_kernel_profile_srcversion_failure_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            profile = directory/'profile'
            rows = ''.join('q6synthetic'+str(i)+' '+'A'*24+'\n' for i in range(16))
            profile.write_text(audio.RELEASE+'\n'+rows);profile.chmod(0o444)
            sysdir = directory/'sys'
            for i in range(16):
                path = sysdir/('q6synthetic'+str(i));path.mkdir(parents=True)
                (path/'srcversion').write_text('A'*24+'\n')
            log = directory/'modprobe-calls'
            probe = directory/'probe';probe.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> '+str(log)+'\n');probe.chmod(0o755)
            uname = directory/'uname';uname.write_text('#!/bin/sh\nprintf "%s\\n" '+audio.RELEASE+'\n');uname.chmod(0o755)
            script = (ROOT/'firmware/system/overlay/usr/local/libexec/rustsimhub-audio-drivers').read_text()
            script = script.replace('/usr/share/rustsimhub/audio-driver-profile', str(profile)).replace('/usr/sbin/modprobe', str(probe)).replace('$(uname -r)', '$('+str(uname)+' -r)').replace('/sys/module/', str(sysdir)+'/')
            def run(): return subprocess.run(['sh','-ec',script], capture_output=True, text=True)
            self.assertEqual(run().returncode, 0)
            expected = ['-C /dev/null --ignore-install '+name for name in audio.PRELOAD]
            self.assertEqual(log.read_text().splitlines(), expected)
            log.unlink();uname.write_text('#!/bin/sh\necho wrong-kernel\n')
            rejected=run();self.assertNotEqual(rejected.returncode, 0);self.assertFalse(log.exists())
            self.assertIn('kernel-release',rejected.stderr)
            uname.write_text('#!/bin/sh\necho '+audio.RELEASE+'\n')
            profile.chmod(0o644)
            self.assertNotEqual(run().returncode, 0);self.assertFalse(log.exists())
            profile.chmod(0o444);(sysdir/'q6synthetic0/srcversion').write_text('B'*24+'\n')
            rejected=run();self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('identity-q6synthetic0',rejected.stderr)
            self.assertEqual(log.read_text().splitlines(), expected)
            (sysdir/'q6synthetic0/srcversion').write_text('A'*24+'\n')
            profile.write_text(audio.RELEASE+'\n'+rows.rsplit('\n',2)[0]+'\n')
            self.assertNotEqual(run().returncode, 0)
