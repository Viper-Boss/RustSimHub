// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { webcrypto } from 'node:crypto';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const bytes=await readFile(new URL('../crates/edge/src/web/indicator.js',import.meta.url));
const {ConfigurationIndicator}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const turn=()=>new Promise(resolve=>setImmediate(resolve));
{
 const sent=[];const hint=new ConfigurationIndicator(command=>async()=>{sent.push(command);});
 const first=hint.set(true);const stop=hint.set(false);await Promise.all([first,stop]);
 assert.deepEqual(sent.map(v=>v.active),[false]);assert.equal(sent[0].view_id.length,36);
}
{
 let resolve;const gate=new Promise(r=>{resolve=r;});const sent=[];let token='before';
 const hint=new ConfigurationIndicator(command=>{const captured=token;return async()=>{sent.push({...command,token:captured});if(command.active)await gate;};});
 const first=hint.set(true);await turn();const stop=hint.set(false);token='after';
 await turn();assert.equal(sent.length,1);resolve();await Promise.all([first,stop]);
 assert.deepEqual(sent.map(v=>v.active),[true,false]);assert.equal(sent[1].token,'before');
 assert.equal(sent[0].view_id,sent[1].view_id);
}
{
 let count=0;const hint=new ConfigurationIndicator(()=>async()=>{if(++count===1)throw Error('offline');});
 await hint.set(true);await hint.set(false);assert.equal(count,2);
}
console.log('Web configuration indicator: close ordering, captured session, coalescing and network failure recovery passed; no network or hardware used.');
