// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { NasBridgePanel, bindingInput, statusInput, pendingInput, receiptInput, journalInput, exportInput, exportJournal, clearExported, readJournal, clearJournal } from '../crates/edge/src/web/nas-bridge.js';

const pin='ab'.repeat(32),key=Array.from({length:32},(_,i)=>i+1),other=key.map(n=>n+1);
const keyHex=key.map(n=>n.toString(16).padStart(2,'0')).join('');
const coreId='nas-'+keyHex.slice(0,8),epoch='bc'.repeat(32);
const binding=()=>({core_id:coreId,public_key:[...key],generation:2,binding_epoch:epoch,scopes:11});
const nas={id:'nas-row',public_key:key,kind:'nas',generation:2,scopes:1023,active:true};
const owner={...nas,id:'owner',kind:'owner',public_key:other};
const intent=()=>({format:1,id:'11111111-1111-4111-8111-111111111111',device_pin:pin,action:'bind',core_id:coreId,public_key:key,generation:2,scopes:11,expected_binding_epoch:null});
const clone=value=>value===undefined?undefined:structuredClone(value);
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element{constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;}replaceChildren(...children){this.children=children;}append(...children){this.children.push(...children);}}
function fixture(){
 const elements=new Map(),commands=[],writes=[],archives=[],prompts=[],downloads=[],revoked=[];
 const state={pin,saved:null,binding:null,consent:true,rows:[owner,nas],loseBind:false,loseRevoke:false,failArchive:false,reply:null,beforeRequest:null,beforeSave:null};
 const document={getElementById(id){if(!elements.has(id))elements.set(id,new Element());return elements.get(id);},createElement(){return new Element();}};
 const request=async command=>{
  commands.push(clone(command));await state.beforeRequest?.(command);
  if(command.op==='clients')return clone(state.rows);
  if(command.op==='legacy_binding_status')return state.binding?{known:true,binding:clone(state.binding),principal_current:true}:{known:false};
  if(command.op==='bind_legacy_checked'){
   assert.ok(state.saved,'persist intent before dispatch');
   const expected=command.expected_binding;
   if(expected.state==='absent'?state.binding!==null:state.binding?.binding_epoch!==expected.binding_epoch)throw Error('synthetic concurrent writer');
   state.binding={core_id:command.core_id,public_key:command.public_key,generation:command.expected_generation,scopes:command.scopes.reduce((m,s)=>m|({'status.read':1,'sms.read':2,'calls.read':8}[s]),0),binding_epoch:state.binding?'cd'.repeat(32):epoch};
   if(state.loseBind)throw Error('synthetic raw error must not enter UI');
   return clone(state.reply??state.binding);
  }
  if(command.op==='revoke_legacy_checked'){
   assert.ok(state.saved);if(state.binding?.binding_epoch!==command.expected_binding_epoch)throw Error('synthetic concurrent writer');state.binding=null;
   if(state.loseRevoke)throw Error('synthetic lost reply');return {ok:true};
  }
  throw Error('unexpected command '+command.op);
 };
 const panel=new NasBridgePanel({document,request,confirm:message=>{prompts.push(message);return state.consent;},getDevicePin:()=>state.pin,
  crypto:{randomUUID:()=>`11111111-1111-4111-8111-${String(writes.length+1).padStart(12,'0')}`},
  loadPending:async()=>clone(state.saved),savePending:async(value,expected)=>{
   await state.beforeSave?.(value);assert.deepEqual(pendingInput(state.saved),pendingInput(expected),'CAS');state.saved=clone(value);writes.push(clone(value));
  },archivePending:async value=>{if(state.failArchive)throw Error('synthetic full journal');archives.push(receiptInput(clone(value)));},
  loadJournal:async guard=>{await state.beforeJournal?.();guard();return clone({pending:state.saved,receipts:archives});},
  clearJournal:async(exported,guard)=>{await state.beforeClear?.();guard();const result=clearExported({pending:state.saved,receipts:archives},exported,state.pin);archives.splice(0,archives.length,...result.receipts);return result;},
  url:{createObjectURL:blob=>{downloads.push(blob);return 'blob:test-'+downloads.length;},revokeObjectURL:url=>revoked.push(url)}});
 return {panel,state,commands,writes,archives,prompts,downloads,revoked,$:id=>document.getElementById(id)};
}

// Strict finite contracts, including bitwise overflow and metadata from another device.
assert.deepEqual(bindingInput(binding()),binding());
for(const change of [{core_id:'../x'},{core_id:'x'.repeat(129)},{generation:0},{generation:2**53},{scopes:4},{scopes:2**32+1},{binding_epoch:epoch.toUpperCase()},{public_key:Array(32)},{token:'private'}])assert.throws(()=>bindingInput({...binding(),...change}));
assert.deepEqual(statusInput({known:false},coreId),{known:false});
for(const value of [{known:false,binding:binding()},{known:true,binding:binding(),principal_current:1},{known:true,binding:{...binding(),core_id:'different'},principal_current:true}])assert.throws(()=>statusInput(value,coreId));
assert.deepEqual(pendingInput(intent()),intent());
for(const change of [{device_pin:'bad'},{id:[intent().id]},{action:'start'},{scopes:2**32+1},{token:'secret'},{action:'revoke',expected_binding_epoch:null}])assert.throws(()=>pendingInput({...intent(),...change}));
assert.throws(()=>receiptInput({intent:intent(),source:'success_inferred',observed:{known:false}}));

// No constructor/reset request; only active NAS keys become selectable, no owner/client promotion.
{
 const f=fixture();assert.equal(f.commands.length,0);f.panel.reset();assert.equal(f.commands.length,0);
 f.state.rows.push({...nas,kind:'client',public_key:other});await f.panel.refresh();
 assert.deepEqual(f.commands.map(c=>c.op),['clients','legacy_binding_status']);assert.equal(f.$('nas-client').children.length,1);
 assert.equal(f.$('nas-bind').disabled,false);assert.equal(f.$('nas-revoke').disabled,true);
 assert.equal(await f.panel.bind(),true);assert.equal(f.state.saved,null);
 assert.deepEqual(f.commands.at(-1),{op:'bind_legacy_checked',core_id:coreId,public_key:key,expected_generation:2,scopes:['status.read','sms.read','calls.read'],expected_binding:{state:'absent'}});
 assert.match(f.prompts.at(-1),new RegExp(keyHex));assert.equal(f.archives[0].source,'reply');assert.equal(f.archives[0].intent.device_pin,pin);
 assert.match(f.$('nas-state').textContent,/已保存/);assert.equal(f.$('nas-revoke').disabled,false);
 await f.panel.revoke();assert.equal(f.state.binding,null);assert.equal(f.state.saved,null);assert.equal(f.archives.length,2);
 assert.equal(f.commands.at(-1).expected_binding_epoch,epoch);assert.match(f.$('nas-state').textContent,/已撤销/);
}
// Cancelled consent, stale selection and invalid names never dispatch a write.
{
 const f=fixture();await f.panel.refresh();f.state.consent=false;await f.panel.bind();assert.equal(f.writes.length,0);
 f.state.consent=true;f.$('nas-alias').value='../x';await f.panel.bind();assert.equal(f.writes.length,0);
 f.$('nas-alias').value='different';await f.panel.bind();assert.equal(f.writes.length,0);
 f.$('nas-alias').oninput();assert.equal(f.$('nas-bind').disabled,true);
}
// Lost bind reply stays durable across reset; read/ack records current state, not original success.
{
 const f=fixture();await f.panel.refresh();f.state.loseBind=true;assert.equal(await f.panel.bind(),false);const saved=clone(f.state.saved);
 assert.equal(f.$('nas-acknowledge').disabled,true);assert.match(f.$('nas-state').textContent,/不会自动重复/);assert.doesNotMatch(f.$('nas-state').textContent,/synthetic/);
 assert.equal(await f.panel.bind(),false);assert.equal(f.commands.filter(c=>c.op==='bind_legacy_checked').length,1);
 f.panel.reset();await f.panel.refresh();assert.deepEqual(f.state.saved,saved);assert.equal(f.$('nas-acknowledge').disabled,false);
 assert.equal(await f.panel.acknowledge(),true);assert.equal(f.state.saved,null);assert.equal(f.archives.at(-1).source,'current_state');assert.equal(f.archives.at(-1).observed.known,true);
 assert.equal(f.commands.filter(c=>c.op==='bind_legacy_checked').length,1);
}
// Changed binding epoch at the actual writer rejects stale bind/revoke, no fallback to old APIs.
for(const action of ['bind','revoke']){
 const f=fixture();f.state.binding=binding();await f.panel.refresh();f.state.binding.binding_epoch='de'.repeat(32);
 assert.equal(await f.panel[action](),false);assert.equal(f.state.binding.binding_epoch,'de'.repeat(32));assert.ok(f.state.saved);
 assert.equal(f.commands.at(-1).op,action==='bind'?'bind_legacy_checked':'revoke_legacy_checked');
 await f.panel.query();await f.panel.acknowledge();assert.equal(f.archives.at(-1).source,'current_state');
}
// Another tab's durable intent prevents any write; it is not overwritten or automatically archived.
{
 const f=fixture();await f.panel.refresh();f.state.saved=intent();const before=clone(f.state.saved);assert.equal(await f.panel.bind(),false);
 assert.deepEqual(f.state.saved,before);assert.equal(f.commands.filter(c=>c.op.startsWith('bind_')).length,0);
 assert.equal(await f.panel.acknowledge(),false);assert.equal(f.archives.length,0);
}
// Record from a different pinned device cannot be queried or cleared in the current context.
{
 const f=fixture();f.state.saved={...intent(),device_pin:'ef'.repeat(32)};
 assert.equal(await f.panel.refresh(),false);assert.equal(f.commands.length,0);assert.ok(f.state.saved);assert.equal(f.archives.length,0);
 assert.equal(await f.panel.acknowledge(),false);assert.ok(f.state.saved);
}
// Reply mismatch and archive failure preserve the original pending marker for manual readback.
for(const failure of ['reply','archive']){
 const f=fixture();await f.panel.refresh();if(failure==='reply')f.state.reply={...binding(),public_key:other};else f.state.failArchive=true;
 assert.equal(await f.panel.bind(),false);assert.ok(f.state.saved);assert.equal(f.$('nas-acknowledge').disabled,true);
 assert.equal(f.commands.filter(c=>c.op==='bind_legacy_checked').length,1);
}
// Logout while local persistence is unresolved dispatches no network write.
{
 const f=fixture(),gate=deferred();await f.panel.refresh();f.state.beforeSave=async value=>{if(value)await gate.promise;};
 const work=f.panel.bind();await new Promise(r=>setTimeout(r,0));f.panel.reset();gate.resolve();await work;
 assert.equal(f.commands.filter(c=>c.op==='bind_legacy_checked').length,0);assert.ok(f.state.saved);assert.equal(f.archives.length,0);
}
// Logout or pinned-context switch while the write is in flight never paints/clears a late reply.
for(const reset of [true,false]){
 const f=fixture(),gate=deferred();await f.panel.refresh();f.state.beforeRequest=async c=>{if(c.op==='bind_legacy_checked')await gate.promise;};
 const work=f.panel.bind();await new Promise(r=>setTimeout(r,0));if(reset)f.panel.reset();else f.state.pin='ef'.repeat(32);
 gate.resolve();await work;assert.ok(f.state.saved);assert.equal(f.archives.length,0);assert.doesNotMatch(f.$('nas-state').textContent,/绑定已保存/);
}
// A late read after reset cannot expose old metadata or make write controls active.
{
 const f=fixture(),gate=deferred();f.state.beforeRequest=async c=>{if(c.op==='clients')await gate.promise;};
 const work=f.panel.refresh();f.panel.reset();gate.resolve();await work;assert.equal(f.$('nas-client').children.length,0);assert.equal(f.$('nas-bind').disabled,true);
}
// A lost revoke is also query-only until explicitly acknowledged, never automatically resent.
{
 const f=fixture();f.state.binding=binding();await f.panel.refresh();f.state.loseRevoke=true;await f.panel.revoke();assert.ok(f.state.saved);
 await f.panel.query();await f.panel.acknowledge();assert.equal(f.archives.at(-1).observed.known,false);assert.equal(f.commands.filter(c=>c.op==='revoke_legacy_checked').length,1);
}

// Assembly is explicit: production route, verified login pin, IDB CAS and cleanup; no mock switch.
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const edge=await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url),'utf8');
for(const id of ['nas-bridge-panel','nas-refresh','nas-query','nas-client','nas-key','nas-alias','nas-bind','nas-revoke','nas-state','nas-acknowledge','nas-binding','nas-export','nas-download','nas-clear-exported'])assert.match(html,new RegExp(`id="${id}"`));
assert.match(edge,/"\/nas-bridge\.js"/);assert.match(edge,/include_str!\("web\/nas-bridge\.js"\)/);
assert.match(app,/authenticatedDevicePin=verifiedDevicePin/);assert.match(app,/nasBridgePanel\?\.reset\(\);authenticatedDevicePin=null/);
assert.match(app,/nas-binding-pending/);assert.match(app,/nas-binding-receipts/);assert.match(app,/visibilitychange.*nasBridgePanel\.reset/);
assert.match(app,/readNasJournal\(db/);assert.match(app,/clearNasJournal\(db/);

const receipt=(n=1,devicePin=pin)=>receiptInput({intent:{...intent(),id:`11111111-1111-4111-8111-${String(n).padStart(12,'0')}`,device_pin:devicePin},source:n%2?'reply':'current_state',observed:{known:false}});
// Export only exact whitelisted metadata for this pin; unknowns remain distinct.
{
 const old=receipt(),foreign=receipt(2,'cd'.repeat(32)),pending={...intent(),id:receipt(3).intent.id};
 const snapshot={receipts:[old,foreign],pending},exported=exportJournal(snapshot,pin);
 assert.deepEqual(exported.receipts,[old]);assert.deepEqual(exported.pending,pending);assert.equal(exported.format,1);
 assert.deepEqual(exportInput(JSON.parse(JSON.stringify(exported))),exported);
 for(const value of [{...exported,token:'never-export'},{...exported,device_pin:'bad'},{...exported,format:2},{...exported,receipts:[foreign]},{...exported,pending:foreign.intent}])assert.throws(()=>exportInput(value));
 for(const rows of [[old,old],Array.from({length:65},(_,i)=>receipt(i+1)),[{...old,token:'no'}]])assert.throws(()=>journalInput({pending:null,receipts:rows}));
 assert.equal(exportJournal({...snapshot,pending:foreign.intent},pin).pending,null);
 const added=receipt(4),result=clearExported({receipts:[old,foreign,added],pending},exported,pin);
 assert.equal(result.removed,1);assert.deepEqual(result.receipts,[foreign,added]);assert.deepEqual(snapshot,{receipts:[old,foreign],pending});
 assert.throws(()=>clearExported(snapshot,exported,'cd'.repeat(32)));
 assert.throws(()=>clearExported({receipts:[],pending},exported,pin));
 assert.throws(()=>clearExported({receipts:[{...old,source:'current_state'}],pending},exported,pin));
 // Interrupted finish can have both an archive and its live pending marker: retain both.
 const protectedResult=clearExported({receipts:[old,added],pending:old.intent},exportJournal({receipts:[old,added],pending:null},pin),pin);
 assert.equal(protectedResult.removed,1);assert.deepEqual(protectedResult.receipts,[old]);
}
// Local preparation/download never queries Core or changes the journal. Cleanup needs
// an explicit download click plus confirmation, and preserves cross-tab arrivals.
{
 const f=fixture();f.archives.push(receipt(),receipt(2,'cd'.repeat(32)));f.state.saved=receipt(3).intent;
 assert.equal(await f.panel.clearExport(),false);assert.equal(f.archives.length,2);
 await f.panel.prepareExport();assert.equal(f.commands.length,0);assert.equal(f.archives.length,2);assert.equal(f.$('nas-clear-exported').disabled,true);
 const exported=JSON.parse(await f.downloads[0].text());assert.equal(exported.receipts.length,1);assert.equal(exported.pending.id,receipt(3).intent.id);
 assert.equal(f.$('nas-download').download,'rustsimhub-nas-records.json');assert.equal(f.$('nas-download').hidden,false);
 f.$('nas-download').onclick({preventDefault(){assert.fail('valid download prevented');}});assert.equal(f.$('nas-clear-exported').disabled,false);
 f.state.consent=false;await f.panel.clearExport();assert.equal(f.archives.length,2);
 f.state.consent=true;f.archives.push(receipt(4));await f.panel.clearExport();assert.deepEqual(f.archives,[receipt(2,'cd'.repeat(32)),receipt(4)]);
 assert.deepEqual(f.state.saved,receipt(3).intent);assert.equal(f.commands.length,0);assert.equal(f.$('nas-download').hidden,true);assert.equal(f.revoked.length,1);
 assert.match(f.$('nas-state').textContent,/已清理 1 条/);
}
// Changed/removed archives cannot be silently cleared using an old export.
{
 const f=fixture();f.archives.push(receipt());await f.panel.prepareExport();f.$('nas-download').onclick({preventDefault(){}});
 f.archives[0].source='current_state';assert.equal(await f.panel.clearExport(),false);assert.equal(f.archives.length,1);assert.equal(f.archives[0].source,'current_state');
}
// No download is produced after logout during read. Cleanup's transaction guard
// fences logout/pin changes without clearing any current pending/receipts.
for(const phase of ['read','clear'])for(const changePin of [false,true]){
 const f=fixture(),gate=deferred();f.archives.push(receipt());
 if(phase==='clear'){await f.panel.prepareExport();f.$('nas-download').onclick({preventDefault(){}});f.state.beforeClear=()=>gate.promise;}
 else f.state.beforeJournal=()=>gate.promise;
 const work=phase==='read'?f.panel.prepareExport():f.panel.clearExport();await new Promise(r=>setTimeout(r,0));
 if(changePin)f.state.pin='ef'.repeat(32);else f.panel.reset();gate.resolve();assert.equal(await work,false);
 assert.equal(f.archives.length,1);assert.equal(f.commands.length,0);if(phase==='read')assert.equal(f.downloads.length,0);
}
{
 const f=fixture();await f.panel.prepareExport();assert.equal(f.downloads.length,0);
 f.archives.push(receipt());await f.panel.prepareExport();f.state.pin='ef'.repeat(32);let prevented=false;
 f.$('nas-download').onclick({preventDefault(){prevented=true;}});assert.equal(prevented,true);assert.equal(f.$('nas-download').hidden,true);assert.equal(f.archives.length,1);
}

// Exercise the actual production IDB adapter with an asynchronous transaction model,
// including abort/commit. Native browser storage/download acceptance remains separate.
function journalDb(initial,{failPut=false}={}){
 const state=new Map(Object.entries(clone(initial))),transactions=[];
 const db={transaction(store,mode){assert.equal(store,'state');const snapshot=new Map(state),writes=[];let waiting=0,aborted=false;
  const tx={mode,writes,abort(){aborted=true;queueMicrotask(()=>tx.onabort?.());},objectStore(name){assert.equal(name,'state');return {
   get(key){const req={};++waiting;queueMicrotask(()=>{if(aborted)return;req.result=clone(snapshot.get(key));req.onsuccess?.();if(!--waiting)queueMicrotask(()=>{if(aborted)return;for(const [k,v] of writes)state.set(k,clone(v));tx.oncomplete?.();});});return req;},
   put(value,key){assert.equal(mode,'readwrite');if(failPut)throw Error('synthetic quota failure');writes.push([key,clone(value)]);}
  };}};transactions.push(tx);return tx;
 }};return {db,state,transactions};
}
{
 const old=receipt(),pending=receipt(2).intent,f=journalDb({'nas-binding-pending':pending,'nas-binding-receipts':[old]});
 const snapshot=await readJournal(f.db,()=>{});assert.deepEqual(snapshot,{pending,receipts:[old]});assert.equal(f.transactions[0].mode,'readonly');assert.deepEqual(f.transactions[0].writes,[]);
 const added=receipt(3);f.state.set('nas-binding-receipts',[old,added]);const exported=exportJournal(snapshot,pin);
 const result=await clearJournal(f.db,exported,pin,()=>{});assert.equal(result.removed,1);assert.deepEqual(f.state.get('nas-binding-receipts'),[added]);assert.deepEqual(f.state.get('nas-binding-pending'),pending);
 assert.deepEqual(f.transactions[1].writes.map(([k])=>k),['nas-binding-receipts']);
 for(const fail of ['guard','conflict','quota']){
  const current=fail==='conflict'?{...old,source:'current_state'}:old;
  const failed=journalDb({'nas-binding-pending':pending,'nas-binding-receipts':[current]},{failPut:fail==='quota'});
  await assert.rejects(clearJournal(failed.db,exported,pin,()=>{if(fail==='guard')throw Error('logged out');}));
  assert.deepEqual(failed.state.get('nas-binding-receipts'),[current]);assert.deepEqual(failed.state.get('nas-binding-pending'),pending);
 }
}
console.log('NAS bridge: read-only bindings/CAS, durable unknown, pinned journal export, exact transactional cleanup and stale-reply fences passed');
