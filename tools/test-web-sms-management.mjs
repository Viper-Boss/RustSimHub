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
 f.consent(true);await f.manager.deleteSelected();assert.equal(f.reloads,1);assert.deepEqual(f.commands.map(c=>c.op),['sms_management_status','delete_sms','deleted_inbox']);assert.equal(f.manager.selected,null);assert.match(f.$('sms-management-state').textContent,/已移到/);
 await f.manager.restore(id);assert.equal(f.reloads,2);assert.equal(f.commands[3].op,'restore_sms');assert.match(f.$('sms-management-state').textContent,/已恢复/);
}
{
 const f=fixture(async()=>({available:false}));await f.manager.permissions();f.manager.select(id);await f.manager.deleteSelected();await f.manager.restore(id);await f.manager.loadTrash();assert.equal(f.commands.length,1);assert.equal(f.$('delete-sms').disabled,true);assert.equal(f.$('trash-section').hidden,true);
}
{
 const gate=deferred();const f=fixture(async c=>c.op==='sms_management_status'?{available:true}:c.op==='delete_sms'?gate.promise:page());await f.manager.permissions();f.manager.select(id);const pending=f.manager.deleteSelected();await f.manager.deleteSelected();await f.manager.restore(id);await f.manager.loadTrash();assert.equal(f.commands.filter(c=>c.op==='delete_sms').length,1);assert.equal(f.$('delete-sms').disabled,true);
 gate.reject(Error('offline'));await pending;assert.equal(f.manager.uncertain,true);await f.manager.deleteSelected();assert.equal(f.commands.length,2);assert.match(f.$('sms-management-state').textContent,/不会自动重试/);
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
console.log('SMS management: permissions, consent, pagination, restore, uncertain writes, stale replies and read failures passed');
