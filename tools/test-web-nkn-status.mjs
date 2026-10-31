// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../crates/edge/src/web/nkn-status.js',import.meta.url));
const {parseNknStatus,nknStatusText,NknStatusPanel,NKN_FIELDS,NKN_ENUMS}=await import('data:text/javascript;base64,'+source.toString('base64'));
const contract=JSON.parse(await readFile(new URL('../docs/team/interfaces/t5-offline-v1.json',import.meta.url),'utf8')).nkn_status;
assert.deepEqual(NKN_FIELDS,contract.fields);for(const key of ['source','enabled','service','path','dht','last_error'])assert.deepEqual(NKN_ENUMS[key],contract[{source:'sources',last_error:'errors'}[key]||key]);
const dto=(changes={})=>({contract:1,source:'engine',generation:9,sequence:7,age_ms:0,enabled:'on',service:'running',path:'unknown',dht:'unknown',rx_bytes:'18446744073709551615',tx_bytes:'0',rtt_ms:null,peer_count:0,last_error:null,...changes});
assert.equal(parseNknStatus(dto()).rx_bytes,'18446744073709551615');
assert.match(nknStatusText(dto(),0),/18446744073709551615 B/);assert.match(nknStatusText(dto(),0),/连接节点数：0（不是用户数或安装数）/);assert.match(nknStatusText(dto(),0),/DHT：未知/);assert.match(nknStatusText(dto(),0),/链路：未知/);
assert.match(nknStatusText(dto({enabled:'off'}),0),/开关：关闭\n服务：运行中/,'Saved switch does not imply process exit');
assert.match(nknStatusText(dto({source:'mock'}),0),/离线模拟（不代表真实设备）/);
for(const age of [null,NaN,Infinity,-1,5001]){const text=nknStatusText(dto(),age);assert.match(text,/陈旧或年龄未知/);assert.match(text,/接收字节：未知/);assert.doesNotMatch(text,/18446744073709551615 B/);}
assert.match(nknStatusText(dto({source:'unavailable'}),0),/开关：未知\n服务：未知/);
for(const change of [{contract:2},{source:'core'},{generation:-1},{generation:2**32},{sequence:true},{sequence:2**32},{age_ms:-1},{age_ms:Infinity},{enabled:true},{service:'SYNTHETIC_SECRET'},{path:'http://example.invalid'},{dht:'running'},{rx_bytes:'01'},{rx_bytes:'-1'},{rx_bytes:'18446744073709551616'},{tx_bytes:0},{tx_bytes:'9'.repeat(5000)},{rtt_ms:NaN},{rtt_ms:60001},{peer_count:1.5},{peer_count:100001},{last_error:'SYNTHETIC_SECRET'},{token:'SYNTHETIC_SECRET'}]){
 assert.throws(()=>parseNknStatus(dto(change)),e=>e.code==='protocol_error'&&!e.message.includes('SYNTHETIC_SECRET'));
}
{const missing=dto();delete missing.sequence;assert.throws(()=>parseNknStatus(missing));}
class Element{constructor(){this.style={};this.disabled=false;this.textContent='';}setAttribute(){}replaceChildren(...v){this.children=v;}set innerHTML(_){throw Error('Never parse data as markup');}}
function fixture({readStatus,clock={mono:100,wall:10000}}={}){
 let listener;const document={hidden:false,createElement:()=>new Element(),addEventListener(type,fn){assert.equal(type,'visibilitychange');listener=fn;},removeEventListener(_type,fn){if(listener===fn)listener=null;}};
 const panel=new NknStatusPanel({document,root:new Element(),readStatus,clock:()=>clock.mono,wallClock:()=>clock.wall});
 return {panel,clock,visibility:hidden=>{document.hidden=hidden;listener?.();},listener:()=>listener};
}
const turn=()=>new Promise(r=>setImmediate(r));
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
{
 const f=fixture();assert.ok(f.panel.button.disabled);await f.panel.refresh();assert.match(f.panel.output.textContent,/尚未接入/);f.panel.dispose();
}
{
 let calls=0;const f=fixture({readStatus:async()=>{calls++;return dto();}});assert.equal(calls,0);await f.panel.refresh();assert.equal(calls,1);assert.match(f.panel.output.textContent,/18446744073709551615 B/);
 f.clock.wall+=5001;f.panel.render();assert.match(f.panel.output.textContent,/陈旧或年龄未知/);assert.equal(calls,1);f.panel.dispose();assert.equal(f.listener(),null);await f.panel.refresh();assert.equal(calls,1);
}
{
 const f=fixture({readStatus:async()=>dto({age_ms:200})});await f.panel.refresh();f.clock.mono-=1;f.panel.render();assert.match(f.panel.output.textContent,/陈旧或年龄未知/);f.panel.dispose();
}
for(const delta of [{mono:3000,wall:0},{mono:0,wall:3000},{mono:-1,wall:0},{mono:0,wall:-1},{mono:NaN,wall:0},{mono:0,wall:NaN}]){
 const clock={mono:100,wall:10000},f=fixture({clock,readStatus:async()=>{clock.mono+=delta.mono;clock.wall+=delta.wall;return dto();}});await f.panel.refresh();assert.equal(f.panel.snapshot,null);assert.match(f.panel.output.textContent,/未知/);f.panel.dispose();
}
{
 const clock={mono:100,wall:10000},f=fixture({clock,readStatus:()=>{queueMicrotask(()=>{clock.mono+=100;clock.wall+=100;});return dto({age_ms:200});}});await f.panel.refresh();assert.equal(f.panel.snapshot.age,300);f.panel.dispose();
}
{
 let value=dto();const f=fixture({readStatus:async()=>value});await f.panel.refresh();value=dto({sequence:6});await f.panel.refresh();assert.equal(f.panel.snapshot,null);assert.equal(f.panel.order.sequence,7);value=dto({sequence:8});await f.panel.refresh();assert.equal(f.panel.order.sequence,8);value=dto({sequence:8});await f.panel.refresh();assert.equal(f.panel.snapshot,null);f.panel.dispose();
}
{
 let value=dto(),calls=0;const f=fixture({readStatus:async()=>{calls++;return value;}});await f.panel.refresh();value=dto({generation:1,sequence:9});await f.panel.refresh();assert.equal(f.panel.snapshot,null);assert.ok(f.panel.button.disabled);value=dto({sequence:99});await f.panel.refresh();assert.equal(calls,2,'An old generation cannot be revived after a context mismatch');
 f.panel.reset();value=dto({generation:1,sequence:1});await f.panel.refresh();assert.equal(f.panel.snapshot.data.generation,1,'A trusted host reset permits a lower numeric generation without guessing order');f.panel.dispose();
}
{
 const old=deferred(),next=deferred();let count=0;const f=fixture({readStatus:()=>{count++;return count===1?old.promise:next.promise;}});const a=f.panel.refresh();await f.panel.refresh();assert.equal(count,1);f.panel.reset();const b=f.panel.refresh();next.resolve(dto({generation:1}));await b;old.resolve(dto({generation:99}));await a;assert.equal(f.panel.snapshot.data.generation,1);f.panel.dispose();
}
{
 let calls=0,value=dto();const f=fixture({readStatus:async()=>{calls++;return value;}});await f.panel.refresh();f.visibility(true);assert.equal(f.panel.snapshot,null);await f.panel.refresh();assert.equal(calls,1);f.visibility(false);assert.equal(calls,1);value=dto({sequence:6});await f.panel.refresh();assert.equal(f.panel.snapshot,null,'Hiding does not clear sequence evidence');f.panel.dispose();
}
{
 const gate=deferred();let signal;const f=fixture({readStatus:a=>{signal=a.signal;return gate.promise;}});const waiting=f.panel.refresh();f.visibility(true);assert.ok(signal.aborted);gate.resolve(dto());await waiting;assert.equal(f.panel.snapshot,null);f.panel.dispose();
}
{
 const f=fixture({readStatus:async()=>{throw {code:'permission_denied',message:'SYNTHETIC_SECRET'};}});await f.panel.refresh();assert.match(f.panel.output.textContent,/权限被拒绝/);assert.doesNotMatch(f.panel.output.textContent,/SYNTHETIC_SECRET/);f.panel.dispose();
}
{
 let calls=0,aborted=0;const f=fixture({readStatus:({signal})=>{calls++;return new Promise(resolve=>signal.addEventListener('abort',()=>{aborted++;resolve(dto());},{once:true}));}});await f.panel.refresh();assert.equal(f.panel.snapshot,null,'Abort cleanup cannot beat the already rejected deadline');assert.equal(calls,1);assert.equal(aborted,1);assert.match(f.panel.output.textContent,/超时/);f.panel.dispose();
}
await turn();
console.log('PASS: actual frozen NknStatusV1 parser/display, exact u64, independent fields, explicit mock/unavailable/stale, manual reads, clock/sequence/context/visibility/reset/abort fences; no engine or network actions');
