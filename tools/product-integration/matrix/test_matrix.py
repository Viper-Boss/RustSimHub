# SPDX-License-Identifier: AGPL-3.0-only
"""Tool failure paths use synthetic public metadata; full crypto uses old runners."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('web5_matrix',Path(__file__).with_name('run.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)


class Contracts(unittest.TestCase):
    def test_profile_table_keeps_smtp_and_ap_health_rejection(self):
        rows=json.loads(m.PROFILES.read_text())
        health=[r for r in rows['profiles'] if r['automatic_health']]
        self.assertEqual([r['mode'] for r in health],['product-recovery-health-v1'])
        self.assertEqual(health[0]['constraints'],{'root_mode':'forwarding','worker_protocol':4,'same_core_bytes':True,'max_age_ms':5000,'process_budget_ms':250})
        combined=next(r for r in rows['profiles'] if r['mode']=='product-email-recovery-v1')
        self.assertEqual(combined['migration_order'],['email3-to4-at-base6','business6-or7-to9'])
        self.assertFalse(rows['hardware_acceptance'])

    def test_source_requires_exact_frozen_head_and_clean_tracked_tree(self):
        with self.assertRaisesRegex(ValueError,'full frozen'):m.identity('HEAD')
        with patch.object(m.subprocess,'check_output',side_effect=['1'*40+'\n',b' M tracked']):
            with self.assertRaises(ValueError):m.identity('1'*40)

    def test_outputs_and_inputs_refuse_overwrite_symlink_and_external_roots(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);work=root/'work';work.mkdir();old=work/'old';old.mkdir();secret=root/'external';secret.write_bytes(b'public synthetic fixture')
            alias=work/'alias';alias.symlink_to(secret)
            with patch.object(m,'ROOT',root):
                self.assertEqual(m.fresh(work/'new'),work/'new')
                for p in [old,root/'outside',alias]:
                    with self.assertRaises(ValueError):m.fresh(p)
                with self.assertRaises(ValueError):m.ordinary(alias)

    def test_mixed_source_and_changed_worker_fail_before_mode_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            for name in ['rustsimhub-core','rustsimhub-forwarder','rustsimhub-core-launcher']:(root/name).write_bytes(b'public synthetic executable')
            archive='a'*64;runtime={'format':1,'worker_protocol':5}
            def metadata(argv,**kw):
                name=Path(argv[0]).name
                value={'source_archive_sha256':archive,'development_fixtures':False,'forwarding_runtime_contract':runtime,'worker':True}
                if name=='rustsimhub-forwarder':value['forwarding_runtime_contract']={'format':1,'worker_protocol':4}
                return subprocess.CompletedProcess(argv,0,json.dumps(value).encode(),b'')
            with patch.object(m,'call',side_effect=metadata):
                with self.assertRaisesRegex(ValueError,'generations'):m.modes(root,{'source_archive_sha256':archive})
                with self.assertRaisesRegex(ValueError,'mixed source'):m.modes(root,{'source_archive_sha256':'b'*64})


if __name__=='__main__':unittest.main()
