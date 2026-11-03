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

    def test_c25_c30_transfer_keeps_stable_ids_and_original_acceptance(self):
        roster = json.loads((team.ROOT/'docs/team/roster.json').read_text())
        members = {m['id']: m for m in roster['members']}
        transferred = {'C-%02d' % n for n in range(25, 31)}
        self.assertEqual(len(members['member-b']['step_ids']), 68)
        self.assertEqual(len(members['member-c']['step_ids']), 24)
        self.assertTrue(transferred <= set(members['member-b']['step_ids']))
        self.assertFalse(transferred & set(members['member-c']['step_ids']))
        self.assertEqual(sum(len(m['step_ids']) for m in members.values()), 118)
        records = {m['member_id']: m['steps'] for m in self.data['team']['task_plan']['members']}
        self.assertTrue(all(s['status'] == 'not_accepted' for s in records['member-b'] if s['id'] in transferred))
        self.assertEqual(sum(s['status'] != 'not_accepted' for s in records['member-c']), 9)
        ownership = json.loads((team.ROOT/'docs/team/offline-ownership.json').read_text())
        moved = {'UPD-02','UPD-03','UPD-05','UPD-08','NAS-03','NAS-04','FWD-22','UPD-10','REC-05','REL-01','REL-04','BLE0-01','BLE0-02','BLE0-04','BLE6-04','REMOTE-02'}
        self.assertEqual({a['id'] for a in ownership['actions'] if a['id'] in moved and a['offline_owner'] == 'member-b'}, moved)
        self.assertTrue(moved <= set(members['member-b']['task_ids']))
        self.assertFalse(moved & set(members['member-c']['task_ids']))
        # The reassignment must not manufacture reviewer decisions or task IDs.
        self.assertEqual(len({step for m in members.values() for step in m['step_ids']}), 118)

    def test_range_and_t5_package_references_keep_actual_scope(self):
        self.assertEqual(plans.project_ids('BLE1-01～04、NKN-05/06',['BLE1-01','BLE1-02','BLE1-03','BLE1-04','NKN-05','NKN-06']),['BLE1-01','BLE1-02','BLE1-03','BLE1-04','NKN-05','NKN-06'])
        rows=plans.parse_document('| wnfhsj / branch | B-21：first；B-22：second | package acceptance |','docs/team/example.md',{'B-21','B-22'},['BLE4-01','BLE4-02','BLE4-03','BLE4-04'])
        self.assertEqual(rows['B-21']['work'],'first')
        self.assertEqual(rows['B-22']['acceptance'],'package acceptance')

    def test_source_drift_and_unapproved_acceptance_fail(self):
        value=copy.deepcopy(self.data['team']['task_plan']);value['sources'][0]['sha256']='0'*64
        with self.assertRaisesRegex(ValueError,'source changed'):plans.validate(value,self.members,team.ROOT)
        value=copy.deepcopy(self.data['team']['task_plan']);next(m for m in value['members'] if m['steps'])['steps'][0]['status']='accepted'
        with self.assertRaisesRegex(ValueError,'accepted'):plans.validate(value,self.members)


if __name__=='__main__':unittest.main()
