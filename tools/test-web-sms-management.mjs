// SPDX-License-Identifier: AGPL-3.0-only
// Real shipped module, synthetic DOM and messages; no hardware or network.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager}=await import('data:text/javascript;base64,'+source.toString('base64'));
const id='00000000-0000-4000-8000-000000000001',other='00000000-0000-4000-8000-000000000002';
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {constructor(){this.textContent='';this.children=[];this.disabled=false;this.hidden=false;}replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}}
const page=(messages=[],seq='0')=>({messages,cursor:{db_epoch:id,seq}});
function fixture(respond=async c=>c.op==='sms_management_status'?{available:true}:c.op==='deleted_inbox'?page():{sms_id:c.sms_id,deleted:c.op==='delete_sms'}){
 const elements=new Map(),commands=[];let consent=true,reloads=0;
 const document={getElementById(key){if(!elements.has(key))elements.set(key,new Element());return elements.get(key);},createElement:()=>new Element()};
 const manager=new SmsManager({document,request:async c=>{commands.push(c);return respond(c);},reloadInbox:async()=>{reloads++;},confirm:()=>consent});
 return {manager,commands,$:key=>document.getElementById(key),consent:v=>{consent=v;},get reloads(){return reloads;}};
}
{
 const f=fixture();await f.manager.permissions();f.manager.select(id);f.consent(false);await f.manager.deleteSelected();assert.deepEqual(f.commands.map(c=>c.op),['sms_management_status']);assert.equal(f.$('delete-sms').disabled,false);
 f.consent(true);await f.manager.deleteSelected();assert.equal(f.reloads,1);assert.deepEqual(f.commands.map(c=>c.op),['sms_management_status','sms_management_status','delete_sms','deleted_inbox']);assert.equal(f.manager.selected,null);assert.match(f.$('sms-management-state').textContent,/已移到/);
 await f.manager.restore(id);assert.equal(f.reloads,2);assert.equal(f.commands[5].op,'restore_sms');assert.match(f.$('sms-management-state').textContent,/已恢复/);
}
{
 const f=fixture(async()=>({available:false}));await f.manager.permissions();f.manager.select(id);await f.manager.deleteSelected();await f.manager.restore(id);await f.manager.loadTrash();assert.equal(f.commands.length,1);assert.equal(f.$('delete-sms').disabled,true);assert.equal(f.$('trash-section').hidden,true);
}
{
 const gate=deferred();const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:c.op==='delete_sms'?gate.promise:page());await f.manager.permissions();f.manager.select(id);const pending=f.manager.deleteSelected();await Promise.resolve();await f.manager.deleteSelected();await f.manager.restore(id);await f.manager.loadTrash();assert.equal(f.commands.filter(c=>c.op==='delete_sms').length,1);assert.equal(f.$('delete-sms').disabled,true);
 gate.reject(Error('offline'));await pending;assert.equal(f.manager.uncertain,true);await f.manager.deleteSelected();assert.equal(f.commands.length,3);assert.match(f.$('sms-management-state').textContent,/不会自动重试/);
 // A read does not silently resolve an unknown write. Fresh session/reset required.
 await f.manager.loadTrash(true);assert.equal(f.manager.uncertain,true);f.manager.reset();assert.equal(f.manager.uncertain,false);
}
{
 const f=fixture(async c=>{if(c.op==='sms_management_status')return {available:true};if(c.op==='delete_sms')return {sms_id:id,deleted:true};throw Error('read offline');});await f.manager.permissions();f.manager.select(id);await f.manager.deleteSelected();assert.equal(f.manager.uncertain,false);assert.match(f.$('sms-management-state').textContent,/操作已确认/);
}
{
 const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:{sms_id:other,deleted:true});await f.manager.permissions();f.manager.select(id);await f.manager.deleteSelected();assert.equal(f.manager.uncertain,true);assert.equal(f.reloads,0);
}
for(const op of ['sms_management_status','deleted_inbox','deleted_sms_detail','delete_sms']){
 const gate=deferred(),f=fixture(async c=>c.op===op?gate.promise:c.op==='sms_management_status'?{available:true}:page());
 let pending;if(op==='sms_management_status')pending=f.manager.permissions();else{await f.manager.permissions();f.manager.select(id);pending=op==='deleted_inbox'?f.manager.loadTrash(true):op==='deleted_sms_detail'?f.manager.showDeleted(id):f.manager.deleteSelected();}
 f.manager.reset();gate.resolve(op==='sms_management_status'?{available:true}:op==='deleted_inbox'?page([{id,line:1}],'1'):op==='delete_sms'?{sms_id:id,deleted:true}:{text:'synthetic private body'});await pending;
 assert.equal(f.$('trash-section').hidden,true);assert.equal(f.$('trash-list').children.length,0);assert.equal(f.$('trash-detail').textContent,'');assert.equal(f.manager.active,null);assert.equal(f.reloads,0);
}
{
 const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:page(Array.from({length:16},()=>({id,line:2})),'19'));await f.manager.permissions();await f.manager.loadTrash(true);assert.equal(f.$('trash-list').children.length,16);assert.equal(f.$('more-trash').disabled,false);await f.manager.loadTrash();assert.equal(f.commands[2].after.seq,'19');assert.equal(f.$('trash-list').children.length,32);
 const row=f.$('trash-list').children[0];assert.equal(row.children[0].textContent,`${id} · SIM 代次 2`);assert.equal(row.children[2].textContent,'恢复');
}
{
 const one=deferred(),two=deferred();let n=0;const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:++n===1?one.promise:two.promise);await f.manager.permissions();const a=f.manager.showDeleted(id),b=f.manager.showDeleted(other);two.resolve({text:'latest synthetic'});await b;one.resolve({text:'stale synthetic'});await a;assert.match(f.$('trash-detail').textContent,/latest/);assert.doesNotMatch(f.$('trash-detail').textContent,/stale/);
}
{
 const gate=deferred();const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:gate.promise);await f.manager.permissions();f.manager.select(id);const load=f.manager.loadTrash();await f.manager.deleteSelected();assert.equal(f.commands.length,2);assert.equal(f.$('delete-sms').disabled,true);gate.resolve(page());await load;assert.equal(f.$('delete-sms').disabled,false);
}
for(const malformed of [{},page([{id:'bad'}]),page(Array(17).fill({id})),{messages:[],cursor:{seq:3,db_epoch:id}}]){
 const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:malformed);await f.manager.permissions();assert.equal(await f.manager.loadTrash(true),false);assert.equal(f.manager.cursor,null);assert.equal(f.$('trash-list').children.length,0);
}
const detail=(record=id,candidate=other)=>({summary:{id:record,line:4,binary:false,duplicate_candidate:candidate},sender:'synthetic sender',timestamp:'2026-01-01T00:00:00Z',text:'synthetic text <img src=x onerror=alert(1)>',data:[]});
{
 const f=fixture(async c=>{assert.equal(c.op,'sms_detail');assert.equal(c.sms_id,other);return detail(other,null);});
 f.manager.select(id,detail());assert.equal(f.$('sms-comparison').hidden,false);assert.match(f.$('sms-comparison-current').textContent,/<img/);assert.equal(f.commands.length,0);
 assert.equal(await f.$('sms-compare').onclick(),true);assert.equal(f.commands.length,1);assert.match(f.$('sms-comparison-prior').textContent,/synthetic sender/);assert.equal(f.$('sms-compare-trash').hidden,true);
 await f.manager.review.compare(true);await f.manager.deleteSelected();assert.equal(f.commands.length,1,'read-only client cannot inspect trash or delete');
}
{
 const f=fixture(async c=>{if(c.op==='sms_management_status')return {available:true};if(c.op==='sms_detail')throw Error('not found');assert.equal(c.op,'deleted_sms_detail');return detail(other,null);});
 await f.manager.permissions();f.manager.select(id,detail());assert.equal(await f.manager.review.compare(),false);assert.deepEqual(f.commands.map(c=>c.op),['sms_management_status','sms_detail'],'no fallback read without another click');
 assert.equal(await f.$('sms-compare-trash').onclick(),true);assert.match(f.$('sms-comparison-state').textContent,/回收站中的候选旧记录/);assert.equal(f.commands.length,3);
 f.manager.review.setOwner(false);assert.equal(f.$('sms-comparison-prior').textContent,'');assert.equal(f.$('sms-compare-trash').hidden,true);
}
for(const candidate of [id,'bad','https://example.invalid/','x'.repeat(4096)]){
 const f=fixture();f.manager.select(id,detail(id,candidate));assert.equal(f.$('sms-compare').disabled,true);await f.manager.review.compare();assert.equal(f.commands.length,0);assert.equal(f.$('sms-comparison-current').textContent,'');
}
for(const reply of [detail(id,null),{}, {...detail(other),data:[256]}, {...detail(other),text:'x'.repeat(4097)}]){
 const f=fixture(async()=>reply);f.manager.select(id,detail());assert.equal(await f.manager.review.compare(),false);assert.equal(f.$('sms-comparison-prior').textContent,'');assert.equal(f.commands.length,1);
}
for(const invalidate of [f=>f.manager.reset(),f=>f.manager.select(other,detail(other,id)),f=>f.manager.select(null)]){
 for(const fails of [false,true]){
  const gate=deferred(),f=fixture(()=>gate.promise);f.manager.select(id,detail());const pending=f.manager.review.compare();invalidate(f);const text=f.$('sms-comparison-state').textContent;
  if(fails)gate.reject(Error('old access revoked'));else gate.resolve(detail(other,null));assert.equal(await pending,false);assert.equal(f.$('sms-comparison-prior').textContent,'');assert.equal(f.$('sms-comparison-state').textContent,text);
 }
}
{
 const gate=deferred();const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:gate.promise);await f.manager.permissions();f.manager.select(id,detail());
 const pending=f.manager.review.compare(true);f.manager.review.setOwner(false);gate.resolve(detail(other,null));assert.equal(await pending,false);assert.equal(f.$('sms-comparison-prior').textContent,'');
}
{
 const old=deferred(),fresh=deferred();let n=0;const f=fixture(()=>++n===1?old.promise:fresh.promise);f.manager.select(id,detail());const a=f.manager.review.compare();await f.manager.review.compare();assert.equal(n,1);
 f.manager.select(other,detail(other,id));const b=f.manager.review.compare();old.resolve(detail(other,null));assert.equal(await a,false);assert.equal(f.$('sms-compare').disabled,true,'old completion cannot release new request');fresh.resolve(detail(id,null));assert.equal(await b,true);assert.equal(f.$('sms-compare').disabled,false);
}
{
 const gate=deferred();const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:c.op==='sms_detail'?gate.promise:c.op==='deleted_inbox'?page():{sms_id:id,deleted:true});await f.manager.permissions();f.manager.select(id,detail());
 const comparison=f.manager.review.compare();await f.manager.deleteSelected();gate.resolve(detail(other,null));assert.equal(await comparison,false);assert.equal(f.$('sms-comparison').hidden,true);assert.equal(f.$('sms-comparison-prior').textContent,'');assert.equal(f.commands.filter(c=>c.op==='delete_sms').length,1);
}
{
 const f=fixture(async()=>({...detail(other,null),data:[0,10,255],text:''}));f.manager.select(id,{...detail(),data:[0,10,255],text:''});assert.equal(await f.manager.review.compare(),true);assert.match(f.$('sms-comparison-prior').textContent,/000aff/);
 f.manager.select(other,detail(other,null));assert.equal(f.$('sms-comparison').hidden,true);assert.equal(f.$('sms-comparison-current').textContent,'');assert.equal(f.$('sms-comparison-prior').textContent,'');
}
console.log('SMS management and candidate review: read-only comparison, explicit trash reads, bounded DTOs, selection/logout/revocation races, consent, restore and uncertain writes passed');

// The server's fresh-owner decision must be rechecked on every explicit write.
for(const restore of [false,true]){
 for(const offline of [false,true]){
  let checks=0;const f=fixture(async c=>{if(c.op==='sms_management_status'){if(++checks===1)return {available:true};if(offline)throw Error('offline');return {available:false};}throw Error('mutation must not be submitted');});
  await f.manager.permissions();f.manager.select(id);if(restore)await f.manager.restore(id);else await f.manager.deleteSelected();
  assert.equal(f.commands.filter(c=>['delete_sms','restore_sms'].includes(c.op)).length,0);assert.equal(f.manager.uncertain,false);assert.match(f.$('sms-management-state').textContent,/操作未提交/);assert.equal(f.$('delete-sms').disabled,true);
 }
}
for(const invalidate of [f=>f.manager.reset(),f=>f.manager.select(other)]){
 const gate=deferred();let checks=0;const f=fixture(async c=>c.op==='sms_management_status'?(++checks===1?{available:true}:gate.promise):Promise.reject(Error('no mutation allowed')));
 await f.manager.permissions();f.manager.select(id);const changing=f.manager.deleteSelected();invalidate(f);gate.resolve({available:true});await changing;
 assert.equal(f.commands.filter(c=>c.op==='delete_sms').length,0);assert.equal(f.manager.uncertain,false);assert.equal(f.manager.active,null);
}
{
 let checks=0;const f=fixture(async c=>{if(c.op==='sms_management_status'){checks++;return {available:true};}throw Error('post-submit response lost');});await f.manager.permissions();f.manager.select(id);await f.manager.deleteSelected();
 assert.equal(checks,2);assert.equal(f.manager.uncertain,true);await f.manager.deleteSelected();assert.equal(f.commands.filter(c=>c.op==='delete_sms').length,1,'no retry after a write may have reached the server');
}
console.log('Fresh-owner write preflight: expired/offline checks, selection/logout races, no mutation before approval and unchanged uncertain-write lock passed');
