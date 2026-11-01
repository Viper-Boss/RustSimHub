// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { NasBridgePanel, bindingInput, statusInput, pendingInput, receiptInput } from '../crates/edge/src/web/nas-bridge.js';

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
 const elements=new Map(),commands=[],writes=[],archives=[],prompts=[];
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
  },archivePending:async value=>{if(state.failArchive)throw Error('synthetic full journal');archives.push(receiptInput(clone(value)));}});
 return {panel,state,commands,writes,archives,prompts,$:id=>document.getElementById(id)};
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
for(const id of ['nas-bridge-panel','nas-refresh','nas-query','nas-client','nas-key','nas-alias','nas-bind','nas-revoke','nas-state','nas-acknowledge','nas-binding'])assert.match(html,new RegExp(`id="${id}"`));
assert.match(edge,/"\/nas-bridge\.js"/);assert.match(edge,/include_str!\("web\/nas-bridge\.js"\)/);
assert.match(app,/authenticatedDevicePin=verifiedDevicePin/);assert.match(app,/nasBridgePanel\?\.reset\(\);authenticatedDevicePin=null/);
assert.match(app,/nas-binding-pending/);assert.match(app,/nas-binding-receipts/);assert.match(app,/visibilitychange.*nasBridgePanel\.reset/);
console.log('NAS bridge: strict read-only bindings, CAS writes, durable unknown, cross-device and stale-reply fences passed');
