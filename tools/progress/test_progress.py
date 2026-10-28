# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
from pathlib import Path
import unittest
import build
import update


class LedgerTests(unittest.TestCase):
    def setUp(self):
        self.data=json.loads((build.DIRECTORY/'progress.json').read_text(encoding='utf-8'))
        # Transitions use a synthetic copy with a known dependency frontier;
        # recording real completions must not make the regression suite stale.
        self.fixture=copy.deepcopy(self.data)
        for module in self.fixture['modules']:
            for task in module['tasks']:
                if module['scope']!='parked':task.update(status='todo',evidence=[],completed_at=None,actual_hours=None)
        for task in self.fixture['modules'][0]['tasks'][:2]:
            task.update(status='done',evidence=['docs/VALIDATION-FORWARDING-INSTALLER-X1M.md'],completed_at='2026-10-25T13:00:00+00:00')
        self.fixture=update.transition(self.fixture,'FWD-03','focus','Synthetic dependency frontier; not actual work.',now='2026-10-25T13:00:00+00:00')
    def test_complete_scope_and_milestone_evidence(self):
        tasks=build.validate(self.data);value=build.metrics(self.data)
        self.assertEqual(value['original_percent'],sum(m['done_points'] for m in self.data['original_milestones']))
        self.assertEqual(value['active'],169);self.assertEqual(value['parked'],9)
        self.assertEqual(len(tasks),178);self.assertEqual(len(self.data['modules']),25)
        self.assertTrue(all(t['work'] and t['acceptance'] and len(t['estimate_hours'])==2 for t in tasks.values()))
    def test_graph_unknown_cycle_and_done_without_evidence_refuse(self):
        for kind in ['unknown','cycle','evidence']:
            data=copy.deepcopy(self.data)
            if kind=='unknown':data['modules'][0]['tasks'][2]['dependencies']=['NO-SUCH-01']
            elif kind=='cycle':data['modules'][0]['tasks'][2]['dependencies']=['FWD-04']
            else:data['modules'][0]['tasks'][0]['evidence']=[]
            with self.assertRaises(ValueError):build.validate(data)
    def test_original_drift_and_parked_inclusion_refuse(self):
        data=copy.deepcopy(self.data);data['original_milestones'][0]['done_points']+=.25
        with self.assertRaises(ValueError):build.validate(data)
        data=copy.deepcopy(self.data);data['modules'][-1]['scope']='extension'
        with self.assertRaises(ValueError):build.validate(data)
    def test_private_material_and_estimate_bounds_refuse(self):
        for field,value in [('work','http://192.168.1.2/'),('work','.private/identity.seed'),('estimate_hours',[-1,2])]:
            data=copy.deepcopy(self.data);data['modules'][0]['tasks'][2][field]=value
            with self.assertRaises(ValueError):build.validate(data)
    def test_generated_files_current_and_embedded_data_safe(self):
        for name,text in build.outputs(self.data).items():self.assertEqual((build.DIRECTORY/name).read_bytes(),text.encode())
        data=copy.deepcopy(self.data);data['project']['title']='</script><script>bad</script>'
        result=build.outputs(data)['index.html']
        self.assertNotIn('</script><script>bad',result);self.assertIn('\\u003c/script>',result)
    def test_static_percentage_labels_follow_the_current_ledger(self):
        value=build.metrics(self.data)['original_percent']
        result=build.outputs(self.data)['index.html']
        self.assertIn(f'id="overall-value">{value:g}</strong>',result)
        self.assertIn(f'id="original-remaining">{100-value:g}</strong>',result)
        self.assertIn(f'aria-label="原计划进度{value:g}%，目标100%"',result)
        self.assertIn(f'已有{value:g}点；新增扩展独立跟踪',result)
        self.assertIn(f'{value:g}%的来处，每一项都保留边界。',result)
        self.assertNotIn('{{MILESTONE_',result)
    def test_one_completion_advances_only_remaining_ledger(self):
        before=build.metrics(self.fixture)
        result=update.transition(self.fixture,'FWD-03','complete','Synthetic update test; not actual completion.',['docs/VALIDATION-FORWARDING-INSTALLER-X1M.md'],1,now='2026-10-25T13:00:00+00:00')
        after=build.metrics(result);self.assertEqual(after['done'],before['done']+1);self.assertGreater(after['sprint_percent'],before['sprint_percent'])
        self.assertEqual(result['focus']['task_id'],'FWD-04')
        self.assertEqual(after['original_percent'],before['original_percent']);self.assertEqual(build.metrics(self.fixture),before)
        self.assertEqual(update.transition(result,'FWD-03','complete','Same result',['docs/VALIDATION-FORWARDING-INSTALLER-X1M.md'],1),result)
    def test_no_completion_without_evidence_or_dependencies(self):
        for key,evidence in [('FWD-03',[]),('FWD-04',['docs/VALIDATION-FORWARDING-INSTALLER-X1M.md'])]:
            with self.assertRaises(ValueError):update.transition(self.fixture,key,'complete','Not proved',evidence)
        with self.assertRaises(ValueError):update.transition(self.data,'POWER-01','complete','Do not resume parked work',['docs/PROGRESS-SCALE.md'])
    def test_private_evidence_and_silent_rewrite_rejected(self):
        for path in ['.private/not-public.json','../outside.md','/etc/passwd']:
            with self.assertRaises(ValueError):build.public_evidence(path)
        with self.assertRaises(ValueError):update.transition(self.data,'FWD-01','state','Do not silently reopen',state='todo')
    def test_start_checks_dependencies_and_records_real_state(self):
        result=update.transition(self.fixture,'FWD-03','state','Starting concrete work',state='in_progress')
        self.assertEqual(result['modules'][0]['tasks'][2]['status'],'in_progress')
        self.assertEqual(build.metrics(result)['done'],build.metrics(self.fixture)['done'])
        with self.assertRaises(ValueError):update.transition(self.fixture,'FWD-04','state','Missing prerequisite',state='in_progress')
    def test_markdown_has_every_action_and_actual_criteria(self):
        text=build.markdown(self.data)
        for module in self.data['modules']:
            for task in module['tasks']:
                self.assertIn(task['id']+' · '+task['title'],text)
                self.assertIn('- 验收：'+task['acceptance'],text)
    def test_focus_changes_do_not_mark_work_completed(self):
        result=update.transition(self.fixture,'AP-01','focus','准备接通受限AP授权生产者')
        self.assertEqual(result['focus']['task_id'],'AP-01')
        self.assertEqual(build.metrics(result)['done'],build.metrics(self.fixture)['done'])


if __name__=='__main__':unittest.main()
