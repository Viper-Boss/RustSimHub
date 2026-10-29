// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {webcrypto} from 'node:crypto';import {readFile} from 'node:fs/promises';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const {RecoveryController}=await import('../src/recovery-controller.js');const c=await import('../src/contracts.js');
const pin='a'.repeat(64),trial='b'.repeat(64),id='11111111-1111-4111-8111-111111111111';
const clone=structuredClone;
function fixture({lost=false,cas=false}={}){let record=null,now=0,counter=0;const calls=[],views=[],history=[];
 const op=(requestId,action,state='ready')=>({contract:1,source:'core',request_id:requestId,action,trial_id:trial,state,authoritative:true,settings_saved:state==='saved',rollback_verified:false,expires_in_ms:40000});
 const store={load:async()=>clone(record),save:async(next,revision)=>{history.push(clone(next));if(cas||(record?.revision??null)!==revision)return false;record=clone(next);return true;}};
 const client={connectFromGesture:()=>{calls.push('gesture');return Promise.resolve();},disconnect:async()=>{calls.push('disconnect');},readStatus:async()=>({contract:1,source:'core',wifi:'disconnected',busy:false,recovery_available:true}),
  beginWifi:async args=>{calls.push(['begin',args.requestId]);assert.ok(record.requests.some(r=>r.request_id===args.requestId&&r.submit_started));if(lost)throw Error('transport_lost');return op(args.requestId,'begin_wifi');},
  confirmWifi:async args=>{calls.push(['confirm',args.requestId]);return op(args.requestId,'confirm_wifi','saved');},cancelWifi:async args=>op(args.requestId,'cancel_wifi','unknown'),
  queryOperation:async args=>{calls.push(['query',args.targetRequestId]);return {contract:1,source:'core',target_request_id:args.targetRequestId,lookup:'not_found',operation:null};}};
 const controller=new RecoveryController({client,store,notify:v=>views.push(clone(v)),uuid:()=>`00000000-0000-4000-8000-${String(++counter).padStart(12,'0')}`,clock:()=>now,hidden:()=>false});
 return {controller,client,store,calls,views,history,record:()=>record,tick:value=>{now=value;},op};
}
{
 const f=fixture();const connecting=f.controller.connect(pin);assert.equal(f.calls[0],'gesture','native selection happens before async storage');await connecting;
 const r=await f.controller.write('begin_wifi',{ssid:'Synthetic Wi-Fi',passphrase:'SYNTHETIC_ONLY_PASSWORD'});assert.equal(r.state,'ready');assert.doesNotMatch(JSON.stringify(f.history)+JSON.stringify(f.views),/SYNTHETIC_ONLY_PASSWORD|Synthetic Wi-Fi/);
 await assert.rejects(f.controller.write('begin_wifi',{ssid:'other',passphrase:'abcdefgh'}));assert.equal(f.calls.filter(v=>v[0]==='begin').length,1);
 const saved=await f.controller.write('confirm_wifi',{trialId:trial});assert.equal(saved.state,'saved');await f.controller.archive(saved.request_id);assert.equal(f.record().requests.length,1);
 await f.controller.disconnect();assert.equal(f.controller.connected,false);
}
{
 const f=fixture({cas:true});await f.controller.connect(pin);await assert.rejects(f.controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'}));assert.equal(f.calls.filter(v=>v[0]==='begin').length,0);
}
{
 const f=fixture({lost:true});await f.controller.connect(pin);await assert.rejects(f.controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'}));const original=f.record().requests[0].request_id;
 await assert.rejects(f.controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'}));await f.controller.query(original);assert.equal(f.record().requests[0].request_id,original);assert.equal(f.calls.filter(v=>v[0]==='begin').length,1);await assert.rejects(f.controller.archive(original));
 const fresh=new RecoveryController({client:f.client,store:f.store,hidden:()=>false,clock:()=>1001});await fresh.connect(pin);await fresh.query(original);assert.equal(f.calls.filter(v=>v[0]==='begin').length,1);
}
{
 const f=fixture();await f.controller.connect(pin);await f.controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'});f.tick(40000);await assert.rejects(f.controller.write('confirm_wifi',{trialId:trial}));assert.equal(f.calls.filter(v=>v[0]==='confirm').length,0);
}
{
 const f=fixture();let finish;f.store.load=()=>new Promise(r=>{finish=r;});const connecting=f.controller.connect(pin);await new Promise(r=>setImmediate(r));await f.controller.disconnect();finish(null);await assert.rejects(connecting);assert.equal(f.controller.connected,false);
}
for(const state of ['saved','rolled_back']){const f=fixture();const op=f.op(id,'begin_wifi',state);op.settings_saved=false;op.rollback_verified=false;assert.throws(()=>c.operation(op,id));}
for(const args of [['', 'abcdefgh'],['a'.repeat(33),'abcdefgh'],['bad\nssid','abcdefgh'],['synthetic','short']])assert.throws(()=>c.credentials(...args));
for(const extra of [{token:'SECRET'},{request_id:'other'}]){const f=fixture(),op={...f.op(id,'begin_wifi'),...extra};assert.throws(()=>c.operation(op,id));}
{
 const f=fixture({lost:true});await f.controller.connect(pin);try{await f.controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'});}catch{}
 const original=f.record().requests[0].request_id;
 for(let n=0;n<30;n++){f.tick(n*1000);await f.controller.query(original);}
 f.tick(30000);await assert.rejects(f.controller.query(original));assert.equal(f.calls.filter(v=>v[0]==='query').length,30);
}
{
 const f=fixture();await f.controller.connect(pin);let finish;f.client.disconnect=()=>new Promise(r=>{finish=r;});const closing=f.controller.disconnect();await assert.rejects(f.controller.connect(pin));finish();await closing;assert.equal(f.controller.busy,false);
}
{
 const rows=Array.from({length:16},(_,n)=>({request_id:`00000000-0000-4000-8000-${String(n).padStart(12,'0')}`,action:'begin_wifi',trial_id:null,submit_started:true}));
 const root={contract:1,device_pin:pin,revision:0,requests:rows};assert.equal(c.pending(root,pin).requests.length,16);assert.throws(()=>c.pending({...root,requests:[...rows,{...rows[0]}]},pin));
}
const frozen=JSON.parse(await readFile(new URL('../../docs/team/interfaces/t5-offline-v1.json',import.meta.url),'utf8'));assert.deepEqual(c.actions,frozen.recovery.operation_actions);assert.deepEqual(c.states,frozen.recovery.operation_states);assert.deepEqual(c.errors,frozen.recovery.errors);
console.log('PASS: frozen contract, gesture-before-await, write-before-CAS, no secret persistence, lost reply/reload query-only, ready expiry, stale connection, invalid evidence and source boundaries');
const vectors=JSON.parse(await readFile(new URL('../../docs/team/interfaces/t5-vectors-v1.json',import.meta.url),'utf8'));
for(const v of vectors.wifi_inputs){let accepted=true;try{c.credentials(v.ssid,v.passphrase);}catch{accepted=false;}assert.equal(accepted,v.accepted,v.name);}
assert.throws(()=>c.operation(vectors.mock_recovery_operation,vectors.mock_recovery_operation.request_id),'production controller rejects mock source');
for(const v of vectors.gatt_frames){const bytes=Uint8Array.from(Buffer.from(v.hex,'hex'));assert.ok(bytes.length<=v.attribute_bytes);assert.equal(Buffer.from(bytes.slice(0,2)).toString(),'RB');assert.equal(bytes[2],1);assert.ok(bytes.length>12);}
console.log('PASS: frozen Wi-Fi vectors, production mock-source rejection and frame-vector header budgets (not a GATT transport test)');
