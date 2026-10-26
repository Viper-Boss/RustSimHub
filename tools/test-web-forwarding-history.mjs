// SPDX-License-Identifier: AGPL-3.0-only
// Production UI class, synthetic replies; real Auth/SQLite are tested in Core.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const source=await readFile(new URL('../crates/edge/src/web/forwarding.js',import.meta.url));
const {ForwardingHistory,historyPendingInput,historyReceiptInput}=await import('data:text/javascript;base64,'+source.toString('base64'));
const target='11111111-1111-4111-8111-111111111111',rule='22222222-2222-4222-8222-222222222222',realm='33333333-3333-4333-8333-333333333333',message='44444444-4444-4444-8444-444444444444';
const clone=structuredClone;
class Element{constructor(tag='div'){this.tagName=tag;this.children=[];this.value='';this.textContent='';this.disabled=false;this.checked=false;this.selected=false;}append(...v){this.children.push(...v);}replaceChildren(...v){this.children=v;this.textContent='';}get options(){return this.children;}set innerHTML(v){throw Error('Never render reply HTML');}}
function fixture({shared={pending:null,archives:[]},full=false,onRequest,onSave,consent=true}={}){
 const elements=new Map(),$=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};$('forwarding-history-after').value='0';$('forwarding-history-limit').value='16';
 const doc={getElementById:$,createElement:tag=>new Element(tag)},calls=[],plans=new Map(),confirms=[];let token=true;
 const status={available:true,configuration_api:true,rules_revision:2,history_api:true,history_revision:2,history_realm:realm,generation:1,enabled:true,process_state_verified:true};
 const panel=new ForwardingHistory({document:doc,getToken:()=>token,crypto:webcrypto,confirm:s=>{confirms.push(s);return consent;},loadPending:async()=>clone(shared.pending),loadArchives:async()=>clone(shared.archives),
  savePending:async(next,expected)=>{await onSave?.(next,expected);assert.deepEqual(shared.pending,expected,'CAS');shared.pending=clone(next);},archivePending:async value=>{shared.archives.push(clone(value));},
  request:async c=>{calls.push(clone(c));const custom=await onRequest?.(c,{plans,shared,panel});if(custom!==undefined)return custom;
   if(c.op==='forwarding_status')return clone(status);
   if(c.op==='forwarding_rules')return {enabled:status.enabled,generation:1,targets:[{id:target,kind:'bark',credentials_redacted:true}],rules:[{id:rule,targets:[target],sender_equals:null,body_contains:null,line:null,share:full?'full_text':'notice'}]};
   if(c.op==='forwarding_history_prepare'){
    assert.equal(shared.pending.id,c.request_id);assert.equal(shared.pending.submit_started,false);
    const p={state:'prepared',expires_in_seconds:300,receipts:[],missing_receipts:false,not_found_is_not_proof:true,preview:{id:c.request_id,confirmation_id:webcrypto.randomUUID(),generation:1,line:1,range:clone(c.range),cursor:{db_epoch:realm,seq:'1'},scanned:1,skipped_messages:0,already_queued_or_recorded:0,deliveries:[{id:webcrypto.randomUUID(),message_id:message,target_id:target,rule_id:rule,share:full?'full_text':'notice'}]}};plans.set(c.request_id,p);return clone(p);
   }
   if(c.op==='forwarding_history_request')return {request_id:c.request_id,known:plans.has(c.request_id),receipt:clone(plans.get(c.request_id)??null),not_found_is_not_proof:true};
   const p=plans.get(c.request_id);assert.ok(p);assert.equal(c.confirmation_id,p.preview.confirmation_id);
   if(c.op==='forwarding_history_commit'){
    assert.equal(shared.pending.submit_started,true);assert.equal(c.accept_full_text,full);p.state='submitted';p.expires_in_seconds=0;p.receipts=p.preview.deliveries.map((d,i)=>({...d,share:undefined,seq:String(i+1),generation:1,synthetic:false,state:'confirmed'}));return clone(p);
   }
   if(c.op==='forwarding_history_cancel'){p.state='cancelled';p.expires_in_seconds=0;return clone(p);}
   if(c.op==='forwarding_history_archive'){assert.ok(shared.archives.some(x=>x.record.id===c.request_id));assert.equal(shared.pending.archive_started,true);plans.delete(c.request_id);return {archived:true,jobs_preserved:true,request_id:c.request_id,confirmation_id:c.confirmation_id};}
   throw Error('Unexpected command');
  }});
 return {panel,$,calls,plans,shared,confirms,status,read:()=>panel.action(g=>panel.read(g)),prepare:()=>panel.action(g=>panel.prepare(g)),commit:()=>panel.action(g=>panel.commit(g)),query:()=>panel.action(g=>panel.query(g)),archive:()=>panel.action(g=>panel.archive(g)),token:v=>{token=v;}};
}
{
 const f=fixture();assert.equal(f.calls.length,0);await f.read();assert.equal(f.calls.filter(c=>c.op.includes('prepare')).length,0);await f.prepare();assert.equal(f.shared.pending.submit_started,false);assert.equal(f.$('forwarding-history-commit').disabled,false);
 await Promise.all([f.commit(),f.commit()]);assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,1);assert.equal(f.shared.pending.submit_started,true);await f.query();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,1);
 await f.archive();assert.equal(f.shared.pending,null);assert.equal(f.shared.archives.length,1);assert.equal(f.shared.archives[0].uncertain,false);assert.equal(f.calls.filter(c=>c.op==='forwarding_history_archive').length,1);
}
{
 const f=fixture({onRequest:async(c,{plans})=>{if(c.op==='forwarding_history_commit'){const p=plans.get(c.request_id);p.state='submitted';p.expires_in_seconds=0;p.receipts=p.preview.deliveries.map(d=>({...d,seq:'1',generation:1,synthetic:false,state:'unknown'}));throw Error('lost reply');}}});
 await f.read();await f.prepare();await f.commit();assert.equal(f.shared.pending.submit_started,true);assert.equal(f.$('forwarding-history-commit').disabled,true);await f.query();await f.commit();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,1);
 await f.archive();assert.equal(f.shared.pending,null);assert.equal(f.shared.archives[0].uncertain,true);assert.equal(f.calls.filter(c=>c.op==='forwarding_history_archive').length,0);assert.equal(f.plans.size,1,'Device Unknown index is retained');
}
{
 const f=fixture({onSave:async next=>{if(next?.submit_started)throw Error('IDB unavailable');}});await f.read();await f.prepare();await f.commit();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,0);assert.equal(f.shared.pending.submit_started,false);
}
{
 const f=fixture({full:true});await f.read();await f.prepare();await f.commit();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,0);f.$('forwarding-history-full').checked=true;await f.commit();assert.match(f.confirms[0],/完整正文/);
}
{
 const f=fixture({onRequest:async c=>{if(c.op==='forwarding_history_prepare')throw Error('lost prepare');}});await f.read();await f.prepare();const saved=clone(f.shared.pending);assert.equal(saved.confirmation_id,null);f.panel.reset();await f.read();assert.deepEqual(f.shared.pending,saved);assert.equal(f.calls.filter(c=>c.op==='forwarding_history_prepare').length,1);await f.archive();assert.equal(f.shared.archives[0].uncertain,true);assert.equal(f.shared.pending,null);
}
{
 const f=fixture();f.status.enabled=false;await f.read();await f.prepare();assert.equal(f.$('forwarding-history-commit').disabled,true);await f.commit();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,0);
}
{
 let release;const delayed=new Promise(r=>release=r);const f=fixture({onRequest:async c=>{if(c.op==='forwarding_history_prepare')await delayed;}});await f.read();const running=f.prepare();await new Promise(r=>setImmediate(r));f.panel.reset();release();await running;assert.equal(f.shared.pending.confirmation_id,null);assert.equal(f.$('forwarding-history-commit').disabled,true);assert.equal(f.calls.filter(c=>c.op==='forwarding_history_commit').length,0);
}
{
 const shared={pending:null,archives:[]},a=fixture({shared});await a.read();await a.prepare();const b=fixture({shared});b.plans.set(a.shared.pending.id,clone(a.plans.get(a.shared.pending.id)));await b.read();
 await Promise.all([a.commit(),b.commit()]);assert.equal([...a.calls,...b.calls].filter(c=>c.op==='forwarding_history_commit').length,1,'Second tab loses CAS before network');
}
{
 const f=fixture();await f.read();await f.prepare();const value=clone(f.panel.result);value.preview.confirmation_id=webcrypto.randomUUID();assert.throws(()=>historyReceiptInput(value,f.shared.pending));
 assert.throws(()=>historyPendingInput({...f.shared.pending,token:'synthetic-secret'}));
 await f.panel.action(g=>f.panel.cancel(g));await f.archive();f.$('forwarding-history-after').value='9223372036854775808';await f.prepare();assert.equal(f.calls.filter(c=>c.op==='forwarding_history_prepare').length,1);
}
console.log('Historical forwarding UI passed: explicit preview, durable CAS before one commit, full consent, OFF guard, lost responses, Unknown preservation, storage/logout races, cross-tab admission, challenge binding, bounded inputs and explicit archive; synthetic only.');
