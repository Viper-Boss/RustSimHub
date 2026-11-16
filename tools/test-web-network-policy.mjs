// SPDX-License-Identifier: AGPL-3.0-only
// Production NetworkPanel with an atomic, failure-injectable journal port.
// Real IndexedDB, default CSP and page reload are separately tested by app-assembly.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const source=await readFile(new URL('../crates/edge/src/web/network.js',import.meta.url));
const {NetworkPanel,policyJournalInput,policyRecordInput}=await import('data:text/javascript;base64,'+source.toString('base64'));
const pin='a'.repeat(64),otherPin='b'.repeat(64),boot='11111111-1111-4111-8111-111111111111';
const empty=()=>({format:1,pending:null,receipts:[]});
const sample=()=>({available:true,boot_id:boot,sample_unix_seconds:100,devices:[],cellular:{},policy:{enabled:false,limit_bytes:1e9,reset_day:1},usage:{uncertain:false},policy_applied:true,last_request_id:''});
class Element{constructor(){this.value='';this.checked=false;this.children=[];this.disabled=false;this.textContent='';this.listeners={};}addEventListener(name,fn){this.listeners[name]=fn;}replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}get options(){return this.children;}getContext(){return null;}}
function store(){const values=new Map();return {values,load:async key=>structuredClone(values.get(key)??null),save:async(next,expected,key,guard)=>{guard();assert.deepEqual(values.get(key)??empty(),expected,'Journal compare-and-swap must reject another tab');values.set(key,structuredClone(next));}};}
function setup({storage=store(),request=async c=>c.op==='network_diagnostics'?sample():{request_id:c.request_id,state:'pending',apn_activated:false},getPin=()=>pin,load=storage.load,save=storage.save,confirm=()=>true}={}){
 const map=new Map(),doc={hidden:false,getElementById:id=>{if(!map.has(id))map.set(id,new Element());return map.get(id);},createElement:()=>new Element()};
 const panel=new NetworkPanel({document:doc,request,getToken:()=>true,getDevicePin:getPin,loadPolicyJournal:load,savePolicyJournal:save,confirm,runTest:()=>{throw Error('No network probes');}});
 panel.running=true;panel.sample=sample();doc.getElementById('network-limit-enabled').checked=true;doc.getElementById('network-limit-gb').value='2';doc.getElementById('network-reset-day').value='15';return {panel,doc,storage};
}
const turn=()=>new Promise(resolve=>setImmediate(resolve));
const intent=()=>({format:1,id:webcrypto.randomUUID(),device_pin:pin,enabled:true,limit_bytes:2e9,reset_day:15,acknowledge_meter_gap:false});
{
 // First assertion is a behavioral old-code negative: a write must precede POST.
 const storage=store(),calls=[];
 const {panel}=setup({storage,request:async c=>{calls.push(c);if(c.op==='set_network_policy')throw Error('lost acknowledgement');return sample();}});
 await panel.savePolicy();assert.equal(calls.filter(c=>c.op==='set_network_policy').length,1);
 assert.equal(storage.values.get(pin)?.pending?.id,calls[0].request_id,'Original ID must be durable before a lost-ack POST');
 const id=panel.pending;assert.ok(id);assert.equal(panel.$('network-policy-save').disabled,true);await panel.savePolicy();assert.equal(calls.length,1);
 panel.stop();const reopened=setup({storage,request:async c=>{calls.push(c);assert.equal(c.op,'network_diagnostics');return sample();}}).panel;
 await reopened.refresh();assert.equal(reopened.pending,id);assert.equal(calls.filter(c=>c.op==='set_network_policy').length,1);assert.equal(reopened.$('network-policy-save').disabled,true);reopened.stop();
}
{
 const calls=[],storage=store();let phase='write';
 const {panel}=setup({storage,save:async(...args)=>{if(phase==='write')throw Error('storage full');await storage.save(...args);},request:async c=>{calls.push(c);return sample();}});
 await panel.savePolicy();assert.equal(calls.length,0,'Failed durable write must prevent POST');assert.equal(storage.values.size,0);
 phase='readback';panel.loadPolicyJournal=async key=>storage.values.has(key)?null:null;
 await panel.savePolicy(); // A failed write is conservatively blocked until a read.
 await panel.refresh();await panel.savePolicy();assert.equal(calls.filter(c=>c.op==='set_network_policy').length,0,'Missing write read-back must prevent POST');assert.ok(storage.values.get(pin)?.pending);panel.stop();
}
{
 const storage=store(),record=intent();storage.values.set(pin,{...empty(),pending:record});let current=sample();const calls=[];
 const {panel}=setup({storage,request:async c=>{calls.push(c);assert.equal(c.op,'network_diagnostics');return current;}});
 for(const bad of [sample(),{...sample(),last_request_id:record.id,policy:record,policy_applied:false},{...sample(),last_request_id:record.id,policy:{...record,limit_bytes:3e9}},{...sample(),last_request_id:record.id,policy:record,boot_id:'bad'}]){current=bad;await panel.refresh();assert.equal(panel.pending,record.id);assert.equal(storage.values.get(pin).receipts.length,0);}
 current={...sample(),last_request_id:record.id,policy:record};await panel.refresh();assert.equal(panel.pending,null);const saved=storage.values.get(pin);assert.equal(saved.receipts.length,1);assert.equal(saved.receipts[0].state,'observed');assert.equal(saved.receipts[0].record.id,record.id);assert.equal(saved.receipts[0].boot_id,boot);assert.ok(calls.every(c=>c.op==='network_diagnostics'));panel.stop();
}
{
 const storage=store(),record={...intent(),acknowledge_meter_gap:true};storage.values.set(pin,{...empty(),pending:record});let uncertain=true;
 const {panel}=setup({storage,request:async()=>({...sample(),last_request_id:record.id,policy:record,usage:{uncertain}})});
 await panel.refresh();assert.equal(panel.pending,record.id,'Acknowledging a meter gap needs evidence it was cleared');uncertain=false;await panel.refresh();assert.equal(panel.pending,null);panel.stop();
}
{
 const storage=store(),calls=[],a=setup({storage,request:async c=>{calls.push(c);return c.op==='network_diagnostics'?sample():{request_id:c.request_id,state:'pending',apn_activated:false};}}),b=setup({storage,request:a.panel.request});
 await Promise.all([a.panel.savePolicy(),b.panel.savePolicy()]);assert.equal(calls.filter(c=>c.op==='set_network_policy').length,1,'Two tabs cannot each publish a new ID');const id=storage.values.get(pin).pending.id;
 await a.panel.refresh();await b.panel.refresh();assert.equal(a.panel.pending,id);assert.equal(b.panel.pending,id);a.panel.stop();b.panel.stop();
}
{
 const storage=store(),record=intent();storage.values.set(pin,{...empty(),pending:record});let activePin=otherPin;const calls=[];
 const {panel}=setup({storage,getPin:()=>activePin,request:async c=>{calls.push(c);return sample();}});await panel.refresh();assert.equal(panel.pending,null);assert.equal(storage.values.get(pin).pending.id,record.id);
 activePin=pin;panel.stop();panel.start();await turn();assert.equal(panel.pending,record.id);assert.ok(calls.every(c=>c.op==='network_diagnostics'));panel.stop();
}
{
 const storage=store(),calls=[];let release;
 const {panel}=setup({storage,save:async(...args)=>{await new Promise(resolve=>release=resolve);await storage.save(...args);},request:async c=>{calls.push(c);return sample();}});
 const old=panel.savePolicy();await turn();assert.ok(release);panel.stop();panel.start();await turn();release();await old;
 assert.ok(calls.every(c=>c.op==='network_diagnostics'),'Logout during journal write prevents POST');assert.equal(panel.pending,null);assert.equal(storage.values.size,0);panel.stop();
}
{
 const storage=store(),record=intent();storage.values.set(pin,{...empty(),pending:record});const calls=[],{panel,doc}=setup({storage,request:async c=>{calls.push(c);return sample();}});
 await panel.refresh();panel.confirm=()=>false;await panel.archivePolicy();assert.equal(storage.values.get(pin).pending.id,record.id);
 panel.confirm=()=>true;const before=calls.length;await panel.archivePolicy();assert.equal(calls.length,before,'Local unknown archive must not send or query');const saved=storage.values.get(pin);assert.equal(saved.pending,null);assert.equal(saved.receipts[0].state,'unknown');assert.equal(saved.receipts[0].record.id,record.id);assert.equal(doc.getElementById('network-policy-save').disabled,true);
 await panel.savePolicy();assert.ok(calls.every(c=>c.op==='network_diagnostics'),'Archived unknown is not permission to repeat the form');doc.getElementById('network-limit-gb').value='3';doc.getElementById('network-limit-gb').listeners.input();assert.equal(doc.getElementById('network-policy-save').disabled,false);panel.stop();
}
{
 const storage=store(),record=intent();storage.values.set(pin,{...empty(),pending:record});const replies=[];
 const {panel}=setup({storage,request:async()=>new Promise(resolve=>replies.push(resolve))});const old=panel.refresh();panel.stop();panel.start();await turn();assert.equal(replies.length,2);
 replies[0]({...sample(),last_request_id:record.id,policy:record});await old;assert.equal(panel.pending,null);assert.equal(panel.loading,true);assert.equal(storage.values.get(pin).pending.id,record.id,'Old session cannot archive the new session journal');
 replies[1](sample());await turn();assert.equal(panel.pending,record.id);panel.stop();
}
{
 const storage=store(),record=intent();storage.values.set(pin,{...empty(),pending:record});let posted=0,finish;
 const {panel}=setup({storage,request:async c=>{if(c.op==='set_network_policy'){posted++;return new Promise(resolve=>finish=resolve);}return {...sample(),last_request_id:storage.values.get(pin)?.pending?.id??'',policy:record};}});
 storage.values.delete(pin);const saving=panel.savePolicy();await turn();const id=storage.values.get(pin).pending.id;
 await panel.refresh();assert.equal(panel.pending,null);finish({request_id:id,state:'wrong',apn_activated:false});await saving;
 assert.equal(posted,1);assert.match(panel.$('network-policy-state').textContent,/已核对/,'Late malformed reply must not overwrite an already observed receipt');panel.stop();
}
{
 const storage=store(),record=intent(),calls=[];storage.values.set(pin,{...empty(),pending:record});const {panel}=setup({storage,save:async()=>{throw Error('archive failed');},request:async c=>{calls.push(c);return {...sample(),last_request_id:record.id,policy:record};}});
 await panel.refresh();assert.equal(storage.values.get(pin).pending.id,record.id);assert.equal(panel.pending,record.id);assert.equal(panel.$('network-policy-save').disabled,true);assert.ok(calls.every(c=>c.op==='network_diagnostics'));panel.stop();
}
{
 const record=intent(),valid={...empty(),pending:record};assert.deepEqual(policyJournalInput(valid,pin),valid);assert.deepEqual(policyJournalInput(null,pin),empty());
 for(const bad of [{},[],{...valid,extra:1},{...valid,receipts:Array(65).fill({})},{...valid,pending:{...record,device_pin:otherPin}},{...valid,pending:{...record,limit_bytes:NaN}},{...valid,pending:{...record,reset_day:0}},{...valid,pending:{...record,id:'not-an-id'}},{...valid,pending:{...record,acknowledge_meter_gap:'true'}},{...valid,receipts:[{record,state:'unknown',boot_id:null,observed_unix_seconds:null}]}])assert.throws(()=>policyJournalInput(bad,pin));
 assert.throws(()=>policyRecordInput(record,otherPin));const full=empty();full.receipts=Array.from({length:64},()=>({record:intent(),state:'unknown',boot_id:null,observed_unix_seconds:null}));const storage=store();storage.values.set(pin,full);const calls=[],{panel}=setup({storage,request:async c=>{calls.push(c);return sample();}});await panel.refresh();await panel.savePolicy();assert.ok(calls.every(c=>c.op==='network_diagnostics'));assert.equal(storage.values.get(pin).receipts.length,64);panel.stop();
}
console.log('PASS: durable original network-policy ID, lost ack/reopen, write/read-back failure, exact observation, meter-gap proof, CAS tabs, device isolation, logout/late result fences, bounded unknown archives and corrupt journal refusal');
