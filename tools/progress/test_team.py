# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import team


class Team(unittest.TestCase):
    def setUp(self):
        self.roster = json.loads((team.ROOT/'docs/team/roster.json').read_text(encoding='utf8'))
        self.members = team.roster_valid(self.roster)

    def test_source_categories_exclude_generated_docs_and_dependencies(self):
        for path, category in [('crates/a/src/lib.rs','product'),('crates/a/src/tests.rs','tests'),
                               ('crates/edge/src/web/network.js','product'),('tools/progress/team.py','tooling'),
                               ('tools/test-web-network.mjs','tests'),('firmware/system/a.service','firmware'),
                               ('tools/nas-adapter/readonly.go','product'),('tools/nas-adapter/http_test.go','tests'),
                               ('tools/nkn-engine/managed/production.go','product'),
                               ('tools/nkn-engine/rootguard/policy_linux_test.go','tests'),
                               ('tools/nkn-engine/vendor/sdk/main.go',None),
                               ('configurator/src/app.js','product'),('configurator/sw.js','product'),
                               ('configurator/index.html','product'),('configurator/tests/browser.py','tests'),
                               ('configurator/tests/controller.mjs','tests'),('configurator/assets/icon.svg',None),
                               ('work/configurator/index.html',None),('configurator/generated.js',None),
                               ('crates/compat/src/nas_adapter_tests.rs','tests'),('tools/nas-adapter/vendor/x.go',None),
                               ('docs/progress/index.html',None),('Cargo.lock',None),('../crates/a.rs',None)]:
            self.assertEqual(team.classify(path),category)

    def test_real_git_blame_churn_and_unknown_author_are_separate(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            def git(*args, who='lead'):
                login={'lead':'Viper-Boss','a':'fungi1220','unknown':'fixture-unknown'}[who]
                env=dict(os.environ,GIT_AUTHOR_NAME=login,GIT_AUTHOR_EMAIL=login+'@users.noreply.github.com',GIT_COMMITTER_NAME=login,GIT_COMMITTER_EMAIL=login+'@users.noreply.github.com')
                return subprocess.check_output(['git',*args],cwd=root,env=env,stderr=subprocess.DEVNULL).decode().strip()
            git('init','-b','main'); (root/'crates/x/src').mkdir(parents=True)
            path=root/'crates/x/src/lib.rs';path.write_text('fn a() {}\n\n')
            git('add','.');git('commit','-m','first')
            path.write_text('fn a() {}\n\nfn b() {}\n')
            git('add','.');git('commit','-m','second',who='a')
            (root/'docs').mkdir();(root/'docs/generated.html').write_text('generated\n'*30)
            git('add','.');git('commit','-m','excluded document',who='unknown')
            revision=git('rev-parse','HEAD')
            stats=team.code_stats(root,revision,self.members)
            self.assertEqual(stats['lead']['current_lines']['product'],1)
            self.assertEqual(stats['member-a']['current_lines']['product'],1)
            self.assertEqual(stats['member-a']['added'],1)
            self.assertEqual(stats['unmapped']['commits'],1)
            self.assertEqual(stats['unmapped']['current_lines']['product'],0)
            git('switch','-c','fixture-feature')
            (root/'tools').mkdir();(root/'tools/fixture.py').write_text('print(0)\n')
            git('add','.');git('commit','-m','branch code',who='a')
            git('switch','main');(root/'docs/note.md').write_text('main note\n')
            git('add','.');git('commit','-m','independent main change')
            git('merge','--no-ff','fixture-feature','-m','fixture merge')
            stats=team.code_stats(root,git('rev-parse','HEAD'),self.members)
            self.assertEqual(stats['member-a']['commits'],2)
            self.assertEqual(stats['member-a']['added'],2)
            self.assertEqual(stats['member-a']['current_lines']['tooling'],1)
            self.assertEqual(sum(row['commits'] for row in stats.values()),5)

    def test_merged_go_bridge_and_tests_keep_their_actual_authors(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            def git(*args, who='Viper-Boss'):
                env = dict(os.environ, GIT_AUTHOR_NAME=who, GIT_AUTHOR_EMAIL=who+'@users.noreply.github.com',
                           GIT_COMMITTER_NAME=who, GIT_COMMITTER_EMAIL=who+'@users.noreply.github.com')
                return subprocess.check_output(['git', *args], cwd=root, env=env, stderr=subprocess.DEVNULL).decode().strip()
            git('init', '-b', 'main')
            directory = root/'tools/nas-adapter'; directory.mkdir(parents=True)
            (directory/'main.go').write_text('package main\n\nfunc original() {}\n')
            git('add', '.'); git('commit', '-m', 'original bridge')
            git('switch', '-c', 'member-c')
            (directory/'main.go').write_text('package main\n\nfunc original() {}\nfunc added() {}\n')
            (directory/'main_test.go').write_text('package main\nfunc TestRead() {}\n')
            git('add', '.'); git('commit', '-m', 'bridge and fixture', who='yh1030')
            git('switch', 'main'); git('merge', '--no-ff', 'member-c', '-m', 'reviewed merge')
            stats = team.code_stats(root, git('rev-parse', 'HEAD'), self.members)
            self.assertEqual(stats['lead']['current_lines']['product'], 2)
            self.assertEqual(stats['member-c']['current_lines']['product'], 1)
            self.assertEqual(stats['member-c']['current_lines']['tests'], 2)
            self.assertEqual(stats['member-c']['commits'], 1)
            self.assertEqual(stats['member-c']['added'], 3)

    def test_identity_aliases_are_explicit_not_default_owner(self):
        self.assertEqual(team.identity('unknown','private@example.invalid',self.members),'unmapped')
        self.assertEqual(team.identity('another name','123+fungi1220@users.noreply.github.com',self.members),'member-a')
        self.assertEqual(team.identity('yh1030','private@example.invalid',self.members),'member-c')
        self.assertEqual(team.identity('YH1030','another@example.invalid',self.members),'member-c')
        self.assertEqual(team.identity('GTW','private@example.invalid',self.members),'member-d')
        self.assertEqual(team.identity('GTW-other','private@example.invalid',self.members),'unmapped')
        data=copy.deepcopy(self.roster);data['members'][2]['git_names']=['Viper-Boss']
        with self.assertRaises(ValueError):team.roster_valid(data)

    def test_changed_head_and_wrong_author_cannot_count_old_acceptance(self):
        member=copy.deepcopy(self.members[1]);member['step_ids']=['A-01','A-02']
        review={'member_id':'member-a','pr':2,'head':'a'*40,'decision':'offline_accepted','steps':['A-01','A-02'],'evidence':['docs/team/README.md'],'self_reported_hours':2}
        current={'number':2,'headRefOid':'a'*40,'state':'OPEN','author':{'login':'fungi1220'}}
        result=team.review_summary(member,[review],{2:current})
        self.assertEqual(result['accepted_steps'],2);self.assertEqual(result['self_reported_hours'],2)
        current['headRefOid']='b'*40
        result=team.review_summary(member,[review],{2:current})
        self.assertEqual(result['accepted_steps'],0);self.assertIsNone(result['self_reported_hours'])
        self.assertEqual(result['reviews'][0]['state'],'needs_re_review')
        current.update(headRefOid='a'*40,author={'login':'someone-else'})
        self.assertEqual(team.review_summary(member,[review],{2:current})['accepted_steps'],0)

    def test_merge_is_not_acceptance_and_missing_hours_stay_unknown(self):
        current={2:{'number':2,'headRefOid':'a'*40,'state':'MERGED','author':{'login':'fungi1220'}}}
        result=team.review_summary(self.members[1],[],current)
        self.assertEqual(result['accepted_steps'],0);self.assertIsNone(result['self_reported_hours'])
        self.assertEqual(len(result['deliveries']),1)

    def test_snapshot_totals_privacy_and_assignment_validation(self):
        data=json.loads((team.ROOT/'docs/progress/progress.json').read_text(encoding='utf8'))
        snapshot=data['team'];tasks={t['id'] for m in data['modules'] for t in m['tasks']}
        team.validate_snapshot(snapshot,tasks)
        self.assertNotIn('@',json.dumps(snapshot))
        for mutation in ['total','task','email','work']:
            changed=copy.deepcopy(snapshot)
            if mutation=='total':changed['totals']['product']+=1
            elif mutation=='task':changed['members'][1]['task_ids']=['NOT-01']
            elif mutation=='email':changed['members'][1]['email']='private@example.invalid'
            else:changed['members'][1]['work']['accepted_steps']=99
            with self.assertRaises(ValueError):team.validate_snapshot(changed,tasks)


if __name__=='__main__':unittest.main()
