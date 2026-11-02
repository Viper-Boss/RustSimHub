// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {fragments,Receiver} from '../src/gatt-framing.js';
for(const size of [1,8,9,106,1103,2048]){
 const wire=Uint8Array.from({length:size},(_,i)=>i%251),receiver=new Receiver(()=>0);
 let id=0;
 for(const kind of [1,1,1,3,3]){
  const frames=fragments(kind,++id,wire);let result=null;
  for(const frame of frames){assert.ok(frame.length<=20);result=receiver.receive(kind,frame);}
  assert.deepEqual(result,wire);
 }
}
const original=fragments(1,1,new Uint8Array(17).fill(9));
for(const mutation of [f=>{f[0]=0;},f=>{f[2]=2;},f=>{f[3]=3;},f=>{f[4]=2;},f=>{f[6]=1;},f=>{f[8]=2;},f=>{f[10]=0;},f=>{f[11]=255;}]){
 const receiver=new Receiver(()=>0),frame=original[0].slice();mutation(frame);
 assert.throws(()=>receiver.receive(1,frame));assert.throws(()=>receiver.receive(1,original[0]));
}
for(const second of [original[0],original[2],original[1].subarray(0,19),fragments(3,1,new Uint8Array(17))[1]]){
 const receiver=new Receiver(()=>0);assert.equal(receiver.receive(1,original[0]),null);
 assert.throws(()=>receiver.receive(second[3],second));
}
for(const invalid of [0,2049])assert.throws(()=>fragments(1,1,new Uint8Array(invalid)));
for(const time of [-1,15000,NaN]){
 let now=0;const receiver=new Receiver(()=>now);receiver.receive(1,original[0]);now=time;
 assert.throws(()=>receiver.receive(1,original[1]));
}
console.log('GATT framing: exact boundaries, sequential messages, malformed fragments, replay, gaps, cross-characteristic and time failures passed');
