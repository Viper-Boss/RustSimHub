# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
from pathlib import Path
import unittest
import plans
import team


class Plans(unittest.TestCase):
    def setUp(self):
        self.data = json.loads((team.ROOT/'docs/progress/progress.json').read_text(encoding='utf8'))
        self.members = self.data['team']['members']
        self.reviews = json.loads((team.ROOT/'docs/team/reviews.json').read_text(encoding='utf8'))['reviews']

    def test_actual_books_cover_every_assigned_step_and_fresh_acceptance(self):
        value = plans.build(team.ROOT,self.members,self.reviews,{m['id']:m['work'] for m in self.members})
        plans.validate(value,self.members,team.ROOT)
        self.assertEqual(value,self.data['team']['task_plan'])
        for entry in value['members']:
            member = next(m for m in self.members if m['id']==entry['member_id'])
            self.assertEqual([s['id'] for s in entry['steps']],member['step_ids'])
        c = next(m for m in value['members'] if m['member_id']=='member-c')
        self.assertEqual([s['id'] for s in c['steps'] if s['status']!='not_accepted'],['C-01','C-02','C-03','C-04','C-05','C-06','C-07','C-08','C-15'])
        self.assertEqual(next(s for s in c['steps'] if s['id']=='C-09')['status'],'not_accepted')

    def test_missing_or_duplicate_steps_cannot_silently_disappear(self):
        members=copy.deepcopy(self.members);members[1]['step_ids'].append('A-99')
        with self.assertRaisesRegex(ValueError,'missing'):
            plans.build(team.ROOT,members,self.reviews,{m['id']:m['work'] for m in members})
        with self.assertRaisesRegex(ValueError,'duplicate'):
            plans.parse_document('### A-01 / First\nDo first.\n### A-01 / Second\nDo second.','docs/team/example.md',{'A-01'},[])

    def test_range_and_t5_package_references_keep_actual_scope(self):
        self.assertEqual(plans.project_ids('BLE1-01～04、NKN-05/06',['BLE1-01','BLE1-02','BLE1-03','BLE1-04','NKN-05','NKN-06']),['BLE1-01','BLE1-02','BLE1-03','BLE1-04','NKN-05','NKN-06'])
        rows=plans.parse_document('| wnfhsj / branch | B-21：first；B-22：second | package acceptance |','docs/team/example.md',{'B-21','B-22'},['BLE4-01','BLE4-02','BLE4-03','BLE4-04'])
        self.assertEqual(rows['B-21']['work'],'first')
        self.assertEqual(rows['B-22']['acceptance'],'package acceptance')
        renamed=plans.parse_document('| wnfhsj88 / branch | B-21：first；B-22：second | package acceptance |','docs/team/example.md',{'B-21','B-22'},['BLE4-01','BLE4-02','BLE4-03','BLE4-04'])
        self.assertEqual(renamed['B-21']['work'],'first')
        self.assertEqual(renamed['B-22']['acceptance'],'package acceptance')

    def test_source_drift_and_unapproved_acceptance_fail(self):
        value=copy.deepcopy(self.data['team']['task_plan']);value['sources'][0]['sha256']='0'*64
        with self.assertRaisesRegex(ValueError,'source changed'):plans.validate(value,self.members,team.ROOT)
        value=copy.deepcopy(self.data['team']['task_plan']);next(s for m in value['members'] for s in m['steps'] if s['status']=='not_accepted')['status']='accepted'
        with self.assertRaisesRegex(ValueError,'accepted'):plans.validate(value,self.members)

    def test_transferred_steps_have_one_owner_and_lead_gets_the_original_specification(self):
        members=copy.deepcopy(self.members)
        lead=next(m for m in members if m['id']=='member-d')
        other=next(m for m in members if m['id']=='member-b')
        self.assertEqual(lead['step_ids'],['D-'+str(n).zfill(2) for n in range(1,27)])
        self.assertFalse(set(lead['step_ids']) & set(other['step_ids']))
        value=plans.build(team.ROOT,members,self.reviews,{m['id']:m['work'] for m in members})
        steps=next(m for m in value['members'] if m['member_id']=='member-d')['steps']
        self.assertEqual([s['id'] for s in steps],lead['step_ids'])
        self.assertEqual([s['source'] for s in steps],
            ['docs/team/MEMBER-D-SMS-RECOVERY.md'] * 12 +
            ['docs/team/ADDITIONAL-OFFLINE-PACKAGES-2026-10-27.md'] * 14)
        self.assertTrue(all(s['status']=='not_accepted' for s in steps))
        roster=json.loads((team.ROOT/'docs/team/roster.json').read_text(encoding='utf8'))
        next(m for m in roster['members'] if m['id']=='lead')['step_ids'].append('D-01')
        with self.assertRaisesRegex(ValueError,'multiple owners'):team.roster_valid(roster)
        self.assertEqual(len(other['step_ids']),62)
        self.assertEqual(sum(len(m['step_ids']) for m in members),118)


if __name__=='__main__':unittest.main()
