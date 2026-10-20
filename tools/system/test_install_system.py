# SPDX-License-Identifier: AGPL-3.0-only
import copy
import hashlib
import importlib.util
import json
import os
import pathlib
import types
import unittest
from unittest.mock import patch

import test_restore_executor as fixtures

with patch.dict(fixtures.sys.modules,{'edl_readonly':fixtures.recovery,'restore_protocol':fixtures.protocol}):
    spec=importlib.util.spec_from_file_location('fixed_install',fixtures.DIRECTORY/'install-system.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


@unittest.skipUnless(hasattr(os,'geteuid') and os.geteuid()==0,'Linux Root isolated images')
class InstallSystemTests(unittest.TestCase):
    def setUp(self):
        self.fixture=fixtures.RestoreExecutorTests(methodName='runTest');self.fixture.setUp()
        f=self.fixture;self.base=f.base
        self.root_data=b'R'*fixtures.PARTITIONS[14][1];self.state_data=b'S'*fixtures.PARTITIONS[15][1]
        self.root=self.base/'rootfs.img';self.root.write_bytes(self.root_data);self.root.chmod(0o600)
        self.state=self.base/'NSHSTATE.img';self.state.write_bytes(self.state_data);self.state.chmod(0o600)
        self.patch=patch.object(module,'ROOTFS_SHA256',hashlib.sha256(self.root_data).hexdigest());self.patch.start()
        self.root_meta={'kind':'internal-unflashable-system-image','passed':True,
            **{k:False for k in module.FALSE_GATES},'image_sha256':module.ROOTFS_SHA256,'tree_sha256':module.ROOTFS_TREE,
            'bytes':len(self.root_data),'private_data_included':False,'private_firmware_included':False,
            'device_state_initialized':False,'generic_audit_passed':True,'source_tree_unchanged':True,
            'filesystem_clean_readonly_check':True}
        self.state_meta={'kind':'offline-maintenance-state-candidate','passed':True,
            **{k:False for k in module.FALSE_GATES},'builder_sha256':module.STATE_BUILDER_SHA256,
            'source_plan_sha256':f.plan_hash,'bytes':len(self.state_data),
            **{k:True for k in ('source_image_unchanged','private_mount_namespace','loop_detached',
                'legacy_entries_unchanged','volume_identity_preserved','filesystem_checks_passed')},
            'device_installed':False,'device_partition_writes':False,
            'candidate_state_sha256':hashlib.sha256(self.state_data).hexdigest(),
            'maintenance_seed_manifest_sha256':'a'*64,'maintenance_public_sha256':'b'*64}
        self.root_manifest=self.base/'root.json';f.write_json(self.root_manifest,self.root_meta)
        self.state_report=self.base/'state.json';f.write_json(self.state_report,self.state_meta)
        self.args=types.SimpleNamespace(output=self.base/'candidate',backup_plan=f.plan,backup_plan_sha256=f.plan_hash,
            capture_report=f.capture,offline_report=f.offline,root_image=self.root,root_manifest=self.root_manifest,
            state_image=self.state,state_report=self.state_report)
    def tearDown(self):self.patch.stop();self.fixture.tearDown()
    def prepare(self):
        # Synthetic GPT/partitions are intentionally not ext4. The real NAS
        # preparation must run the unchanged independent filesystem verifier.
        with patch.object(module.backup,'verify') as check:plan=module.prepare(self.args)
        self.assertEqual(check.call_count,1)
        return self.args.output/'plan.json',plan
    def test_candidate_replaces_only_two_targets_retains_source_and_all_gates_false(self):
        original=self.fixture.image.read_bytes();path,plan=self.prepare()
        data=(path.parent/'candidate.img').read_bytes();expected=bytearray(original)
        for part,value in zip(self.fixture.source.targets,(self.root_data,self.state_data)):
            expected[part['offset']:part['offset']+part['bytes']]=value
        self.assertEqual(data,bytes(expected));self.assertEqual(self.fixture.image.read_bytes(),original)
        self.assertEqual(plan['kind'],'fixed-system-install-plan')
        self.assertTrue(plan['experimental_cold_boot_only']);self.assertFalse(plan['restore_rehearsal_verified'])
        self.assertTrue(all(plan[k] is False for k in (*module.FALSE_GATES,'execution_authorized','partition_writes')))
        source=module.Source(path,fixtures.recovery.digest(path))
        try:self.assertEqual(source.targets,plan['write_targets'])
        finally:source.close()
        with self.assertRaises(ValueError):module.prepare(self.args)
    def test_wrong_generic_state_backup_provenance_or_dirty_filesystem_stops_before_plan(self):
        for target,changes in ((self.root_meta,{'private_data_included':True}),
                               (self.root_meta,{'image_sha256':'0'*64}),
                               (self.root_meta,{'filesystem_clean_readonly_check':False}),
                               (self.state_meta,{'source_plan_sha256':'e'*64}),
                               (self.state_meta,{'loop_detached':False}),
                               (self.state_meta,{'device_partition_writes':True}),
                               (self.state_meta,{'builder_sha256':'f'*64})):
            root=copy.deepcopy(self.root_meta);state=copy.deepcopy(self.state_meta)
            (root if target is self.root_meta else state).update(changes)
            with self.assertRaises(ValueError):module.check_manifests(root,state,self.fixture.plan_hash)
        with patch.object(module.backup,'verify',side_effect=ValueError('dirty candidate')):
            with self.assertRaises(ValueError):module.prepare(self.args)
        self.assertFalse((self.args.output/'plan.json').exists())
        self.assertFalse((self.fixture.tools/'install-attempts').exists())
    def test_changed_candidate_input_evidence_or_wrong_plan_digest_refuses(self):
        path,plan=self.prepare()
        with self.assertRaises(ValueError):module.Source(path,'0'*64)
        image=path.parent/'candidate.img'
        original=image.read_bytes();changed=bytearray(original);changed[41*512]^=1;image.write_bytes(changed)
        with self.assertRaises(ValueError):module.Source(path,fixtures.recovery.digest(path))
        image.write_bytes(original)
        self.state_report.write_text(json.dumps({**self.state_meta,'maintenance_public_sha256':'c'*64}))
        with self.assertRaises(ValueError):module.Source(path,fixtures.recovery.digest(path))
    def test_private_input_links_wrong_owner_mode_or_short_bytes_are_refused(self):
        self.root.chmod(0o644)
        with self.assertRaises(ValueError):module.open_part(self.root,len(self.root_data),module.ROOTFS_SHA256)
        self.root.chmod(0o600);alias=self.base/'alias';alias.symlink_to(self.root)
        with self.assertRaises(ValueError):module.open_part(alias,len(self.root_data),module.ROOTFS_SHA256)
        with self.assertRaises(ValueError):module.open_part(self.root,len(self.root_data)+512,module.ROOTFS_SHA256)
        os.link(self.root,self.base/'hardlink')
        with self.assertRaises(ValueError):module.open_part(self.root,len(self.root_data),module.ROOTFS_SHA256)
    def test_explicit_install_uses_separate_marker_and_exact_shared_readback_policy(self):
        path,plan=self.prepare();source=module.Source(path,fixtures.recovery.digest(path))
        original_marker=module.executor.consume_once(self.fixture.tools,self.fixture.source,self.base,{'sha256':'a'*64})
        original=(pathlib.Path(original_marker)/'attempt.json').read_bytes()
        out=self.base/'installed';out.mkdir(mode=0o700)
        before=bytearray(self.fixture.original);before[200*512]=77
        policy=module.protocol.RestorePolicy(plan,source.targets,source.protected)
        wire=fixtures.MemoryWire(before,policy)
        hardware=module.executor.Hardware(wire.client(),policy,lambda:None);report={'passed':False}
        try:
            with patch.object(module.backup,'verify'):
                module.executor.transaction(source,hardware,self.fixture.tools,out,'install',report,lambda:None,
                                            consume_attempt=module.consume_install)
            self.assertTrue(report['passed']);self.assertEqual(wire.commands,['configure','program','program','power'])
            self.assertEqual(wire.data[200*512],77)
            self.assertTrue((self.fixture.tools/'install-attempts'/plan['plan_id']/'attempt.json').exists())
            self.assertEqual((pathlib.Path(original_marker)/'attempt.json').read_bytes(),original)
            with self.assertRaises(FileExistsError):module.consume_install(self.fixture.tools,source,out,report['current_before'])
            with self.assertRaises(ValueError):module.executor.consume_once(self.fixture.tools,source,out,report['current_before'])
        finally:hardware.close();source.close()
    def test_default_validation_never_opens_usb_or_consumes_marker_and_install_requires_both_acknowledgements(self):
        path,plan=self.prepare();f=self.fixture
        args=types.SimpleNamespace(action='validate',output=self.base/'validation',tools=f.tools,plan=path,
            plan_sha256=fixtures.recovery.digest(path),confirm_install_plan_sha256=None,accept_unrehearsed_restore=False,usb_port=None)
        with patch.object(module.recovery,'verify_tools'), \
                patch.object(module.executor.Hardware,'connect',side_effect=AssertionError('USB forbidden')):
            result=module.execute(args)
        self.assertTrue(result['passed']);self.assertFalse(result['usb_opened'])
        self.assertFalse((f.tools/'install-attempts').exists())
        args.action='install';args.output=self.base/'unconfirmed'
        for confirmed,accepted in ((None,False),(args.plan_sha256,False),(None,True)):
            args.confirm_install_plan_sha256=confirmed;args.accept_unrehearsed_restore=accepted
            with self.assertRaises(ValueError):module.execute(args)
        self.assertFalse(args.output.exists())
