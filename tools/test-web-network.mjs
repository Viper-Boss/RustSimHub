// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const source=await readFile(new URL('../crates/edge/src/web/network.js',import.meta.url));
const {bytes,rate,policyInput,radioRows,NetworkPanel}=await import('data:text/javascript;base64,'+source.toString('base64'));
assert.equal(bytes(1e9),'1.00 GB');assert.equal(bytes(-1),'暂不可用');assert.match(rate(125000),/1.00 Mbps/);
assert.deepEqual(policyInput(true,'1.5','31'),{enabled:true,limit_bytes:1500000000,reset_day:31});
for(const values of [['',1],['NaN',1],['Infinity',1],[1,0],[1,32],[1,1.5],[100001,1]])assert.throws(()=>policyInput(true,...values));
const rows=radioRows({configured_bands:['eutran-1','eutran-3'],serving_cell:{}});assert.equal(rows.find(r=>r[0]==='当前驻留频段')[1],null);assert.equal(rows.find(r=>r[0]==='当前带宽')[1],'设备暂未提供');
class Element {constructor(){this.value='';this.checked=false;this.children=[];this.textContent='';}addEventListener(){}replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}get options(){return this.children;}getContext(){return null;}}
function setup(request){const map=new Map();const doc={hidden:false,getElementById:id=>{if(!map.has(id))map.set(id,new Element());return map.get(id);},createElement:()=>new Element()};return {panel:new NetworkPanel({document:doc,request,getToken:()=>true,runTest:()=>{throw Error('must not run automatically');}}),doc,map};}
const turn=()=>new Promise(r=>setImmediate(r));
{
 let finish;const {panel,map}=setup(()=>new Promise(r=>{finish=r;}));panel.start();await turn();panel.stop();finish({available:true,devices:[{interface:'private-address'}]});await turn();assert.equal(map.get('network-devices').children.length,0);assert.equal(panel.sample,null);assert.equal(panel.running,false);
}
{
 const seen=[];let ack='';const sample={available:true,sample_unix_seconds:1,devices:[],policy:{enabled:false,limit_bytes:1e9,reset_day:1},usage:{},last_request_id:''};
 const {panel,doc}=setup(async c=>{seen.push(c);if(c.op==='set_network_policy')return {request_id:c.request_id,state:'pending'};return {...sample,last_request_id:ack};});panel.running=true;panel.sample=sample;
 doc.getElementById('network-limit-enabled').checked=true;doc.getElementById('network-limit-gb').value='2';doc.getElementById('network-reset-day').value='15';
 await panel.savePolicy();const id=panel.pending;assert.ok(id);await panel.savePolicy();assert.equal(seen.filter(c=>c.op==='set_network_policy').length,1);ack=id;await panel.refresh();assert.equal(panel.pending,null);panel.stop();
}
{
 let calls=0;const {panel,doc}=setup(async()=>{calls++;});doc.hidden=true;panel.start();await turn();assert.equal(calls,0);panel.stop();
}
assert.ok(!source.toString().includes('innerHTML'));
console.log('PASS: network units/input, radio unknowns, logout late-response isolation, pending-policy no-repeat, hidden-page no-poll, no automatic data tests');
