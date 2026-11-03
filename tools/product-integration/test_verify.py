# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import verify


class Sources(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Synthetic integration test')
        self.git('config', 'user.email', 'synthetic@example.invalid')
        self.base = self.save('baseline')
        self.first = self.save('real first source')
        self.second = self.save('real second source')
        self.lock = {'format':1, 'repository':'Viper-Boss/RustSimHub', 'baseline':self.base,
            'inputs':[{'id':'first', 'kind':'product', 'pr':1, 'head':self.first, 'requires':[], 'includes':[]},
                      {'id':'second', 'kind':'product', 'pr':2, 'head':self.second, 'requires':['first'], 'includes':['first']}]}

    def git(self, *args):
        return subprocess.check_output(['git','-C',str(self.repo),*args], stderr=subprocess.DEVNULL, text=True).strip()

    def save(self, text):
        (self.repo/'material').write_text(text)
        self.git('add','material'); self.git('commit','-qm',text)
        return self.git('rev-parse','HEAD')

    def check(self, value=None, at=None):
        value = verify.parse(json.dumps(value or self.lock).encode())
        return verify.inspect(self.repo, value, at or self.second)

    def test_real_ancestry_and_complete_sources_never_authorize_install_or_health(self):
        result = self.check()
        self.assertTrue(result['source_assembly_complete'])
        for field in ['authorization_verified','health_authorized','installable','tests_verified','cloud_checks_verified','device_access']:
            self.assertFalse(result[field])

    def test_older_combination_is_not_complete_despite_commit_objects_existing(self):
        result = self.check(at=self.first)
        self.assertEqual(result['not_in_combination'], ['second'])
        self.assertFalse(result['source_assembly_complete'])

    def test_pending_delivery_is_not_satisfied_by_a_title_or_a_report(self):
        value = copy.deepcopy(self.lock)
        value['inputs'][1].update(head=None,pr=None,includes=[])
        result = self.check(value)
        self.assertEqual(result['awaiting_delivery'], ['second'])
        self.assertFalse(result['source_assembly_complete'])
        value['inputs'][1]['pr']=99
        with self.assertRaises(ValueError): self.check(value)

    def test_report_cannot_supply_product_dependency(self):
        value = copy.deepcopy(self.lock); value['inputs'][0]['kind']='report'
        with self.assertRaises(ValueError): self.check(value)

    def test_missing_optional_report_does_not_supply_or_block_product_sources(self):
        value = copy.deepcopy(self.lock)
        value['inputs'].append({'id':'report', 'kind':'report', 'pr':None, 'head':None, 'requires':[], 'includes':[]})
        result = self.check(value)
        self.assertTrue(result['source_assembly_complete'])
        self.assertEqual(result['awaiting_reports'],['report'])

    def test_clean_checkout_at_a_later_commit_cannot_prove_the_frozen_worktree(self):
        self.save('later product changes')
        result = self.check()
        self.assertFalse(result['checkout_matches_integration'])
        self.assertFalse(result['source_assembly_complete'])

    def test_untracked_or_modified_source_prevents_complete_claim(self):
        (self.repo/'untracked').write_text('a source not in the commit')
        self.assertFalse(self.check()['source_assembly_complete'])
        (self.repo/'untracked').unlink()
        (self.repo/'material').write_text('changed')
        self.assertFalse(self.check()['source_assembly_complete'])

    def test_claimed_included_history_must_really_be_an_ancestor(self):
        value = copy.deepcopy(self.lock)
        value['inputs'][0].update(requires=['second'], includes=['second'])
        value['inputs'][1].update(requires=[],includes=[])
        with self.assertRaises(ValueError): self.check(value)

    def test_missing_or_noncommit_object_is_refused(self):
        for head in ['f'*40,self.git('rev-parse','HEAD:material')]:
            value = copy.deepcopy(self.lock);value['inputs'][1]['head']=head
            with self.assertRaises(ValueError): self.check(value)

    def test_unknown_fields_duplicates_versions_and_graph_errors(self):
        cases=[]
        for field,value in [('format',True),('format',2),('repository','another/repo'),('baseline','HEAD')]:
            c=copy.deepcopy(self.lock);c[field]=value;cases.append(c)
        c=copy.deepcopy(self.lock);c['trusted']=True;cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][1]['id']='first';cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][1]['pr']=1;cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][1]['head']=self.first;cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][0]['requires']=['second'];cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][0]['requires']=['absent'];cases.append(c)
        c=copy.deepcopy(self.lock);c['inputs'][0]['includes']=['second'];cases.append(c)
        for value in cases:
            with self.subTest(value=value), self.assertRaises(ValueError): self.check(value)
        with self.assertRaises(ValueError): verify.parse(b'{"format":1,"format":1}')

    def test_git_replace_cannot_fake_dependency_history(self):
        self.git('replace',self.second,self.base)
        # --no-replace-objects must see second's actual first parent history.
        result = self.check()
        self.assertTrue(result['source_assembly_complete'])

    def test_cli_strict_mode_reports_incomplete_without_writing_repository(self):
        lock = Path(self.temp.name)/'lock.json'
        lock.write_text(json.dumps(self.lock));self.git('add','lock.json');self.git('commit','-qm','source lock')
        at = self.git('rev-parse','HEAD')
        script = Path(verify.__file__)
        before = self.git('status','--porcelain')
        result=subprocess.run([sys.executable,str(script),'--repository',str(self.repo),'--integration',self.first,
                               '--lock',str(lock),'--require-complete'],capture_output=True,timeout=10)
        self.assertEqual(result.returncode,3,result.stderr)
        self.assertFalse(json.loads(result.stdout)['source_assembly_complete'])
        self.assertEqual(self.git('status','--porcelain'),before)
        self.assertEqual(self.git('rev-parse','HEAD'),at)


if __name__=='__main__':unittest.main()
