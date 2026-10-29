// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {RecoveryController} from '../src/recovery-controller.js';
const pin='a'.repeat(64),trial='b'.repeat(64),clone=structuredClone;
for(const lost of [false,true]){
 let record=null,n=0,cancellations=0,now=0;
 const store={load:async()=>clone(record),save:async(next,revision)=>{if((record?.revision??null)!==revision)return false;record=clone(next);return true;}};
 const op=(id,action,state)=>({contract:1,source:'core',request_id:id,action,trial_id:trial,state,authoritative:true,settings_saved:false,rollback_verified:state==='rolled_back',expires_in_ms:40000});
 const client={connectFromGesture:async()=>{},readStatus:async()=>({contract:1,source:'core',wifi:'disconnected',busy:false,recovery_available:true}),
 beginWifi:async a=>op(a.requestId,'begin_wifi','connecting'),cancelWifi:async a=>{cancellations++;if(lost)throw Error('transport_lost');return op(a.requestId,'cancel_wifi','rolled_back');}};
 const c=new RecoveryController({client,store,clock:()=>now,hidden:()=>false,uuid:()=>`00000000-0000-4000-8000-${String(++n).padStart(12,'0')}`});await c.connect(pin);await c.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'});assert.ok(c.cancellable(trial));assert.ok(!c.ready(trial));
 await assert.rejects(c.write('confirm_wifi',{trialId:trial}));
 if(lost){await assert.rejects(c.write('cancel_wifi',{trialId:trial}));await assert.rejects(c.write('cancel_wifi',{trialId:trial}));assert.equal(record.requests.length,2,'uncertain cancellation retained');}
 else{const cancelled=await c.write('cancel_wifi',{trialId:trial});assert.equal(cancelled.state,'rolled_back');assert.ok(!c.cancellable(trial));await c.archive(cancelled.request_id);assert.ok(!c.cancellable(trial),'archiving cancel receipt cannot revive the old ready/connecting snapshot');await assert.rejects(c.write('cancel_wifi',{trialId:trial}));await assert.rejects(c.write('confirm_wifi',{trialId:trial}));}
 assert.equal(cancellations,1);now=40000;assert.ok(!c.cancellable(trial));
}
console.log('PASS: explicit cancellation of current connecting trial, ready-only confirmation, lost cancel never resubmitted, original IDs retained and expiry fence');
