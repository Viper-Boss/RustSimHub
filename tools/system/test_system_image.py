# SPDX-License-Identifier: AGPL-3.0-only
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('system_image',ROOT/'tools/build-system-image.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


class SystemImageInputTests(unittest.TestCase):
    def fixture(self,base):
        source=base/'source';source.mkdir()
        tree=source/'rootfs';(tree/'var/lib/rustsimhub').mkdir(parents=True)
        (tree/'program').write_bytes(b'synthetic source')
        digest,count=module.rootfs_tools.tree_hash(tree)
        value={'kind':'internal-system-candidate',**{k:False for k in module.GATES},
               'app_service_group_installed':True,'native_media_installed':True,'kernel_audio_modules_installed':True,
               'tree_sha256':digest,'tree_entries':count,
               'kernel_audio_layer':{'kernel_source_commit':'93a71ee9468dd8d47a9e082e5d818608f08712e3'}}
        (source/'SYSTEM-CANDIDATE.json').write_text(json.dumps(value))
        return source,base/'out',value

    def test_existing_output_and_changed_tree_stop_before_any_builder(self):
        with tempfile.TemporaryDirectory() as temporary:
            source,out,value=self.fixture(pathlib.Path(temporary))
            module.check_source(source,out)
            out.mkdir();(out/'evidence').write_bytes(b'preserve')
            with patch.object(module.subprocess,'run',side_effect=AssertionError('must not run')):
                with self.assertRaises(ValueError):module.build(source,out)
            self.assertEqual((out/'evidence').read_bytes(),b'preserve')
            other=out.parent/'other'
            (source/'rootfs/program').write_bytes(b'tampered')
            with self.assertRaises(ValueError):module.build(source,other)
            self.assertFalse(other.exists())

    def test_private_or_incomplete_candidate_is_rejected_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            source,out,value=self.fixture(pathlib.Path(temporary))
            for key,new in [('private_data_included',True),('kernel_audio_modules_installed',False),('flash_allowed',True)]:
                (source/'SYSTEM-CANDIDATE.json').write_text(json.dumps({**value,key:new}))
                with self.assertRaises(ValueError):module.build(source,out)
                self.assertFalse(out.exists())

    def test_wrong_kernel_attribution_or_state_payload_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            source,out,value=self.fixture(pathlib.Path(temporary))
            value['kernel_audio_layer']['kernel_source_commit']='unattributed'
            (source/'SYSTEM-CANDIDATE.json').write_text(json.dumps(value))
            with self.assertRaises(ValueError):module.build(source,out)
            value['kernel_audio_layer']['kernel_source_commit']='93a71ee9468dd8d47a9e082e5d818608f08712e3'
            (source/'rootfs/var/lib/rustsimhub/identity').write_bytes(b'private')
            value['tree_sha256'],value['tree_entries']=module.rootfs_tools.tree_hash(source/'rootfs')
            (source/'SYSTEM-CANDIDATE.json').write_text(json.dumps(value))
            with self.assertRaises(ValueError):module.build(source,out)
            self.assertFalse(out.exists())
