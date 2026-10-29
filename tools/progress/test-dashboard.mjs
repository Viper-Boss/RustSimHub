// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {calculate,selectModules,estimateDays,escapeHTML,dependencyReady,handoffText,publicURL,teamHTML,personAssignments,personHandoffText} from '../../docs/progress/dashboard.mjs';
const data=JSON.parse(readFileSync(new URL('../../docs/progress/progress.json',import.meta.url),'utf8'));
const stats=calculate(data);
assert.equal(stats.original,data.original_milestones.reduce((sum,m)=>sum+m.done_points,0));assert.equal(stats.total,169);assert.equal(stats.parked,9);
assert.equal(stats.done+stats.remaining,169);
assert.ok(stats.hours[0]>0&&stats.hours[1]>=stats.hours[0]);
assert.equal(selectModules(data).length,23);
assert.equal(selectModules(data,{scope:'parked'}).flatMap(row=>row.matches).length,9);
assert.equal(selectModules(data,{query:'FWD-03'}).length,1);
assert.equal(selectModules(data,{query:'FWD-03'})[0].matches[0].id,'FWD-03');
assert.equal(selectModules(data,{query:'不存在的完全随机任务'}).length,0);
assert.equal(selectModules(data,{condition:'done'}).reduce((sum,row)=>sum+row.matches.length,0),stats.done);
assert.ok(selectModules(data,{condition:'offline'}).flatMap(row=>row.matches).every(t=>t.offline_delegable&&t.status!=='done'));
const map=new Map(data.modules.flatMap(m=>m.tasks).map(t=>[t.id,t]));
for(const id of ['FWD-01','FWD-02'])map.set(id,{...map.get(id),status:'done'});
for(const id of ['FWD-03','FWD-04'])map.set(id,{...map.get(id),status:'todo'});
assert.equal(dependencyReady(map.get('FWD-03'),map),true);assert.equal(dependencyReady(map.get('FWD-04'),map),false);
assert.deepEqual(estimateDays([24,48],12),[2,4]);assert.throws(()=>estimateDays([24,48],0));
assert.equal(escapeHTML('<img onerror="bad">'), '&lt;img onerror=&quot;bad&quot;&gt;');
assert.ok(publicURL('docs/PROGRESS-SCALE.md').startsWith('https://github.com/Viper-Boss/RustSimHub/blob/main/'));
assert.ok(handoffText(data.modules[0]).includes('FWD-22'));
assert.ok(handoffText(data.modules[0]).includes('不要假定你可以操作设备'));
const revised=structuredClone(data),candidate=revised.modules.filter(m=>m.scope!=='parked').flatMap(m=>m.tasks).find(t=>t.status!=='done');
if(candidate){candidate.status='done';assert.equal(calculate(revised).original,stats.original);assert.equal(calculate(revised).done,stats.done+1);assert.ok(calculate(revised).weighted>stats.weighted);}
console.log('Dashboard calculation/filter/dependency/estimate/escaping/handoff checks passed.');
const cards=teamHTML(data.team,data);
assert.ok(cards.includes('fungi1220'));assert.ok(cards.includes('wnfhsj88'));assert.ok(cards.includes('https://github.com/wnfhsj88'));assert.ok(!cards.includes('>wnfhsj<'));assert.ok(cards.includes('yh1030'));assert.ok(cards.includes('RedamancyGTW'));
assert.ok(cards.includes('未填报'));
for(const member of data.team.members.filter(m=>m.id!=='lead'))assert.ok(cards.includes(`${member.work.accepted_steps} / ${member.work.assigned_steps}`));
assert.equal((cards.match(/<article/g)||[]).length,5);
assert.equal((cards.match(/data-person-details=/g)||[]).length,5);
assert.equal((cards.match(/<template data-person-template=/g)||[]).length,5);
// Task content must remain available for each owner, but be inert until Details opens.
const visibleCards=cards.replace(/<template\b[^>]*>[\s\S]*?<\/template>/g,'');
assert.ok(!visibleCards.includes('data-step-id='));
assert.ok(!visibleCards.includes('class="person-tasks"'));
assert.equal((visibleCards.match(/class="team-column"/g)||[]).length,5);
for(const member of data.team.members){
  const expected=(member.stats.current_lines.product/data.team.totals.product*100).toFixed(1);
  assert.ok(visibleCards.includes(`<strong>${expected}</strong><span>%</span>`));
}
const noProduct=structuredClone(data.team);noProduct.totals.product=0;
assert.ok(!teamHTML(noProduct).includes('height:NaN'));
assert.ok(!teamHTML(noProduct).includes('<strong>NaN</strong>'));
assert.ok(teamHTML(noProduct).includes('暂无统计'));
const hostile=structuredClone(data.team);hostile.members[1].name='<img src=x onerror=bad>';
assert.ok(teamHTML(hostile).includes('&lt;img'));assert.ok(!teamHTML(hostile).includes('<img'));
assert.ok(teamHTML(null).includes('尚未生成'));
const people=personAssignments(data),lead=people.find(p=>p.member.id==='lead');
assert.deepEqual(lead.steps.map(s=>s.id),Array.from({length:20},(_,n)=>`D-${String(n+1).padStart(2,'0')}`));
assert.ok(personHandoffText(lead).includes('D-03'));
assert.ok(!people.find(p=>p.member.id==='member-d').steps.some(s=>s.id==='D-03'));
assert.ok(teamHTML(data.team,data,{query:'D-03'}).includes('data-person-id="lead"'));
assert.ok(personHandoffText(lead).includes('D-20'));
assert.equal(people.find(p=>p.member.id==='member-d').steps.length,6);
const allProjects=people.flatMap(p=>p.projects.map(row=>row.task.id));
assert.equal(allProjects.length,stats.total);assert.equal(new Set(allProjects).size,stats.total);
assert.equal(people.reduce((n,p)=>n+p.remaining,0),stats.remaining);
assert.equal(people.reduce((n,p)=>n+p.completed,0),stats.done);
assert.equal(lead.remaining,stats.remaining-people.filter(p=>p.member.id!=='lead').reduce((n,p)=>n+p.remaining,0));
for(const p of people.filter(p=>p.member.id!=='lead')){
  assert.deepEqual(p.steps.map(s=>s.id),p.member.step_ids);
  assert.deepEqual(p.projects.map(row=>row.task.id).sort(),[...p.member.task_ids].sort());
  for(const step of p.steps)assert.ok(cards.includes(`data-step-id="${step.id}"`));
}
assert.equal((cards.match(/data-step-id=/g)||[]).length,data.team.members.reduce((n,m)=>n+m.step_ids.length,0));
assert.ok(teamHTML(data.team,data,{query:'C-07'}).includes('person-member-c'));
assert.ok(!teamHTML(data.team,data,{query:'C-07'}).includes('person-member-a'));
assert.equal(teamHTML(data.team,data,{query:'nonexistent-person-task-qxyz'}),'');
const a=people.find(p=>p.member.id==='member-a');assert.ok(personHandoffText(a).includes('A-30'));assert.ok(!personHandoffText(a).includes('B-29'));
const malicious=structuredClone(data);malicious.team.task_plan.members[0].steps[0].work='<script>bad()</script>';
assert.ok(teamHTML(malicious.team,malicious).includes('&lt;script&gt;'));assert.ok(!teamHTML(malicious.team,malicious).includes('<script>bad'));
const duplicate=structuredClone(data);duplicate.team.members[2].task_ids.push(duplicate.team.members[1].task_ids[0]);assert.throws(()=>personAssignments(duplicate));
console.log('Team cards, independent work/code metrics, pending member and HTML escaping passed.');
console.log('All personal steps, unique project ownership, lead integration boundary, search and private handoff checks passed.');
