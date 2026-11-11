# SPDX-License-Identifier: AGPL-3.0-only
"""WEB2-RUN subprocess/count/environment and exact-source inventory regression."""
from dataclasses import replace
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import run
from checklist import CHECKS, NAS_COMMANDS, NAS_ENV, Check, fixed_environment
import workflow_coverage as coverage


class Synchronization(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='web2-run-fixture-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        (self.root/'.gitignore').write_text('.local/\n__pycache__/\n')
        self.commit()

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.DEVNULL)

    def commit(self):
        self.git('add', '.')
        self.git('-c','user.name=Synthetic','-c','user.email=fixture@example.invalid',
                 '-c','commit.gpgsign=false','commit','-qm','fixture')

    def test_actual_module_unittest_zero_skip_and_success(self):
        sources = [('', 'failed'), ('import unittest\nclass T(unittest.TestCase):\n @unittest.skip("synthetic")\n def test_skip(self): pass\n', 'not_run'),
                   ('import unittest\nclass T(unittest.TestCase):\n def test_pass(self): self.assertTrue(True)\n','passed')]
        for i,(text,_) in enumerate(sources): (self.root/('case%d.py'%i)).write_text(text)
        self.commit()
        for i,(_,expected) in enumerate(sources):
            check = Check('module','runner',(sys.executable,'-m','unittest','case%d'%i,'-v'),kind='unittest')
            with patch.object(run,'CHECKS',[check]),patch.object(run,'environment',return_value={'capabilities':{}}):
                code=run.run(['runner'],'.local/count%d/report.json'%i,self.root)
            data=json.loads((self.root/('.local/count%d/report.json'%i)).read_text())
            self.assertEqual(data['checks'][0]['status'],expected)
            if i == 0:
                # Python versions may return either 0 or 5 for zero selection.
                # Preserve the real status; neither is evidence of a test pass.
                self.assertIn(data['checks'][0]['exit_code'], (0, 5))
            else:
                self.assertEqual(data['checks'][0]['exit_code'],0)
            self.assertEqual(code,0 if expected=='passed' else 1)
            self.assertFalse(data['source']['dirty'])

    def fake_go(self):
        directory=self.root/'bin';directory.mkdir()
        script=directory/'go'
        script.write_text('''#!/usr/bin/env python3
import json,os,sys
env={key:os.environ.get(key) for key in ('GO111MODULE','GOTOOLCHAIN','GOPROXY','GOSUMDB')}
if '-json' in sys.argv:
 print(json.dumps({'Action':'output','Package':'fixture/control','Output':json.dumps(env)}))
 print(json.dumps({'Action':'pass','Package':'fixture/control','Test':'TestSynthetic'}))
 print(json.dumps({'Action':'pass','Package':'fixture/control'}))
else:
 print(json.dumps(env))
 if sys.argv[1]=='test': print('--- PASS: TestSynthetic (0.00s)')
''')
        script.chmod(0o755)
        for path in ('tools/nas-adapter/control','tools/nas-adapter/cmd/control','tools/nkn-engine'):
            (self.root/path).mkdir(parents=True)
            (self.root/path/'fixture.txt').write_text('synthetic external tool')
        self.commit()
        return str(directory)+os.pathsep+os.environ['PATH']

    def test_fixed_go_env_applies_only_to_two_stages_not_following_nkn(self):
        path=self.fake_go()
        stages=[next(c for c in CHECKS if c.name==name) for name in (*NAS_COMMANDS,'go-offline-tests')]
        parent={'PATH':path,'GO111MODULE':'on','GOTOOLCHAIN':'auto','GOPROXY':'synthetic-parent','GOSUMDB':'synthetic-parent'}
        with patch.dict(os.environ,parent),patch.object(run,'CHECKS',stages),patch.object(run,'environment',return_value={'capabilities':{'go':True,'go-race':True}}):
            self.assertEqual(run.run(['ordinary'],'.local/env/report.json',self.root),0)
            self.assertEqual(os.environ['GO111MODULE'],'on')
        rows=json.loads((self.root/'.local/env/report.json').read_text())['checks']
        for row in rows[:2]:
            self.assertEqual(row['fixed_environment'],dict(NAS_ENV))
            self.assertIn('off',row['summary'])
        self.assertNotIn('fixed_environment',rows[2])
        self.assertIn('synthetic-parent',rows[2]['summary'])
        self.assertIn('"GO111MODULE": "on"',rows[2]['summary'])
        self.assertIn('test_counts',rows[0])
        self.assertNotIn('test_counts',rows[1], 'successful vet is not a test count')

    def test_race_capability_missing_leaves_test_not_run_but_vet_executes(self):
        path=self.fake_go()
        stages=[next(c for c in CHECKS if c.name==name) for name in NAS_COMMANDS]
        with patch.dict(os.environ,{'PATH':path}),patch.object(run,'CHECKS',stages),patch.object(run,'environment',return_value={'capabilities':{'go':True,'go-race':False}}):
            self.assertEqual(run.run(['ordinary'],'.local/no-race/report.json',self.root),1)
        rows=json.loads((self.root/'.local/no-race/report.json').read_text())['checks']
        self.assertEqual(rows[0]['status'],'not_run');self.assertIsNone(rows[0]['exit_code'])
        self.assertEqual(rows[1]['status'],'passed')

    def test_fixed_environment_refuses_changed_command_or_generic_field(self):
        stage=next(c for c in CHECKS if c.name=='nas-control-test')
        self.assertEqual(fixed_environment(stage),dict(NAS_ENV))
        with self.assertRaises(ValueError): fixed_environment(replace(stage,argv=('go','test','./...')))
        with self.assertRaises(ValueError): fixed_environment(replace(stage,cwd='tools/nkn-engine'))
        with self.assertRaises(TypeError): Check('arbitrary','ordinary',('go','test'),environment={'LD_PRELOAD':'bad'})
        self.assertEqual(fixed_environment(Check('other','ordinary',('go','test'))),{})

    def test_fixed_stage_selection_cannot_supply_commands_or_skip_dependencies_as_passed(self):
        self.assertEqual([c.name for c in run.select_checks(['ordinary'],['offline-delivery'])],['offline-delivery'])
        for names in ([],['unknown'],['native-test'],['go; shell']):
            with self.assertRaises(ValueError):run.select_checks(['ordinary'],names)
        checks=[Check('blocked','runner',(sys.executable,'-c','raise SystemExit(99)'),depends=('omitted',))]
        with patch.object(run,'CHECKS',checks),patch.object(run,'environment',return_value={'capabilities':{}}):
            self.assertEqual(run.run(['runner'],'.local/dependency/report.json',self.root,names=['blocked']),1)
        row=json.loads((self.root/'.local/dependency/report.json').read_text())['checks'][0]
        self.assertEqual(row['execution'],'not_run');self.assertIsNone(row['exit_code'])

    def test_exact_workflow_and_all_expanded_identities_match(self):
        value=coverage.audit()
        self.assertEqual(value['check_count'],len(CHECKS))
        self.assertTrue(value['not_general_ci_or_release_approval'])
        for name in ('offline-delivery','kernel-artifact-audit','nas-control-test','nas-control-vet','native-test-relay','native-clippy-relay'):
            self.assertIn(name,{c.name for c in CHECKS})
        # Every manually named step must actually exist in this exact version.
        contract=json.loads(Path(coverage.__file__).with_name('workflow-inventory.json').read_text())
        raw=(run.ROOT/coverage.WORKFLOW).read_text()
        for row in contract['workflow_steps']:
            self.assertEqual(raw.count('      - name: '+row['step']+'\n'),1)

    def test_workflow_edit_unknown_syntax_and_identity_add_delete_or_arg_drift_rejected(self):
        target=self.root/coverage.WORKFLOW;target.parent.mkdir(parents=True)
        raw=(run.ROOT/coverage.WORKFLOW).read_bytes();target.write_bytes(raw)
        coverage.audit(self.root,CHECKS)
        for changed in (raw+b'\ninclude: !unknown\n',raw.replace(b'-race',b'-run TestOnly'),raw.replace(b'      - name: Format\n',b'')):
            target.write_bytes(changed)
            with self.assertRaisesRegex(ValueError,'workflow changed'):coverage.audit(self.root,CHECKS)
        target.write_bytes(raw)
        for stages in (CHECKS[:-1],CHECKS+[Check('new','ordinary',('python3','new.py'))],[replace(CHECKS[0],argv=('cargo','fmt','--all')), *CHECKS[1:]]):
            with self.assertRaisesRegex(ValueError,'identity changed'):coverage.audit(self.root,stages)

    def test_go_json_actual_counts_skips_zero_and_malformed(self):
        def classify(events):
            result={'execution':'executed','exit_code':0,'cleanup_complete':True,'summary':'\n'.join(json.dumps(e) for e in events)}
            return run.classify(result,'go-json'),result
        good=[{'Action':'pass','Package':'p','Test':'TestOne/sub'},{'Action':'pass','Package':'p','Test':'TestOne'},{'Action':'skip','Package':'cmd'}]
        status,result=classify(good);self.assertEqual(status,'passed');self.assertEqual(result['test_counts']['passed'],1);self.assertEqual(result['test_counts']['passed_subtests'],1)
        for events in ([],[{'Action':'pass','Package':'p'}],[good[1],good[1]],[{'Action':'future','Package':'p'}],[{'Action':'pass','Test':7}]):
            self.assertEqual(classify(events)[0],'failed')
        self.assertEqual(classify([{'Action':'skip','Package':'p','Test':'TestOne'}])[0],'not_run')
        self.assertEqual(classify([good[1],{'Action':'skip','Package':'p','Test':'TestOther'}])[0],'not_run')
        self.assertEqual(classify([good[1],{'Action':'fail','Package':'p'}])[0],'failed')


if __name__=='__main__':unittest.main()
