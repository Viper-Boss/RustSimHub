// SPDX-License-Identifier: AGPL-3.0-only
// Production module with synthetic data and a controlled clock. Optional old-source negative control.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {harness} from './test-web-console.mjs';
const source=await readFile(process.argv[2]||new URL('../crates/edge/src/web/console.js',import.meta.url),'utf8');
const {ConsoleDashboard}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const before=!!process.argv[2],flush=async()=>{await new Promise(r=>setImmediate(r));};
const phone={id:'0123456789abcdef01234567',device_hint:'TEST',last_peer:'192.168.1.20',last_seen_ms:1793600000000};
function fixture(){
 let now=1793600000000,state={enabled:true,pcm_rate:8000,pairings:[],window_seconds:110},failure=null,hold=null;
 const x=harness(c=>{
  if(c.op==='airsim_status'){if(failure)throw Error(failure);return hold?hold(structuredClone(state)):structuredClone(state);}
  if(c.op==='airsim_open_pairing')return {code:'042917',expires_at:Math.floor(now/1000)+120,pairing_port:7594};
  return {};
 },{Dashboard:ConsoleDashboard,clock:()=>now});
 return {...x,setState:s=>state=s,getState:()=>state,advance:n=>now+=n,fail:e=>failure=e,hold:f=>hold=f,reads:()=>x.seen.filter(c=>c.op==='airsim_status').length};
}
let passed=0,failed=0;
async function test(name,run){try{await run();passed++;console.log('PASS '+name);}catch(e){failed++;console.log('FAIL '+name+': '+e.message);if(!before)throw e;}}

await test('visible security page reads without another button and never mutates',async()=>{
 const x=fixture();await x.h.signedIn();x.h.tick();await flush();assert.equal(x.reads(),0);
 x.h.go('security');x.h.tick();await flush();assert.equal(x.reads(),1);assert.match(x.$('c-as-msg').textContent,/0 条 iPhone 配对记录/);
 x.h.tick();await flush();assert.equal(x.reads(),1);x.advance(5000);x.h.tick();await flush();assert.equal(x.reads(),2);
 assert(x.seen.filter(c=>c.op.startsWith('airsim')).every(c=>c.op==='airsim_status'));x.h.reset();
});
await test('phone pairing appears on the next read and clears the consumed code',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');await x.h.airsimPair();assert.equal(x.$('c-as-code').hidden,false);
 x.setState({...x.getState(),pairings:[phone],window_seconds:null});x.h.tick();await flush();
 assert.equal(x.$('c-as-list').children.length,1);assert.equal(x.$('c-as-code').hidden,true);assert.equal(x.$('c-as-empty').hidden,true);
 assert.match(x.$('c-as-msg').textContent,/1 条 iPhone 配对记录/);assert.equal(x.seen.filter(c=>c.op==='airsim_open_pairing').length,1);x.h.reset();
});
await test('a read timeout preserves known devices and does not claim pairing failed',async()=>{
 const x=fixture();await x.h.signedIn();x.setState({...x.getState(),pairings:[phone]});await x.h.airsim({op:'airsim_status'});
 const row=x.$('c-as-list').children[0];x.fail('请求超时');await x.h.airsim({op:'airsim_status'});
 assert.equal(x.$('c-as-list').children[0],row);assert.equal(x.$('c-as-state').textContent,'已启用');
 assert.match(x.$('c-as-msg').textContent,/配对列表暂未更新.*不会撤销手机配对/);assert.doesNotMatch(x.$('c-as-msg').textContent,/配对失败|结果未知/);x.h.reset();
});
await test('invalid response cannot erase or replace the last confirmed list',async()=>{
 const x=fixture();await x.h.signedIn();x.setState({...x.getState(),pairings:[phone]});await x.h.airsim({op:'airsim_status'});
 const row=x.$('c-as-list').children[0];
 for(const bad of [{},null,{enabled:true,pcm_rate:8000,pairings:'bad'},{enabled:true,pcm_rate:8000,pairings:[{...phone,id:'bad'}]}]){
  x.setState(bad);assert.equal(await x.h.airsim({op:'airsim_status'}),null);assert.equal(x.$('c-as-list').children[0],row);
 }x.h.reset();
});
await test('page leave and hidden tab stop polling, and returning starts a fresh read',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');x.doc.hidden=true;x.h.tick();await flush();assert.equal(x.reads(),0);
 x.doc.hidden=false;x.h.tick();await flush();assert.equal(x.reads(),1);x.h.go('dashboard');x.advance(5000);x.h.tick();await flush();assert.equal(x.reads(),1);
 x.h.go('security');x.h.tick();await flush();assert.equal(x.reads(),2);x.h.reset();
});
await test('one status request in flight, late result after navigation is ignored',async()=>{
 const x=fixture();await x.h.signedIn();let release;x.hold(value=>new Promise(resolve=>release=()=>resolve(value)));
 x.h.go('security');x.h.tick();await flush();x.advance(6000);x.h.tick();await flush();assert.equal(x.reads(),1);
 x.h.go('dashboard');release();await flush();assert.equal(x.$('c-as-state').textContent,'未读取');assert.equal(x.$('c-as-list').children.length,0);x.h.reset();
});
await test('logout clears the list and an old completion cannot unlock a newer request',async()=>{
 const x=fixture();await x.h.signedIn();let oldRelease,newRelease;
 x.hold(value=>new Promise(resolve=>oldRelease=()=>resolve(value)));const old=x.h.airsim({op:'airsim_status'});await flush();x.h.reset();
 await x.h.signedIn();x.hold(value=>new Promise(resolve=>newRelease=()=>resolve(value)));const fresh=x.h.airsim({op:'airsim_status'});await flush();
 oldRelease();await old;assert.equal(x.h.asBusy,true);assert.equal(x.$('c-as-state').textContent,'未读取');newRelease();await fresh;assert.equal(x.h.asBusy,false);x.h.reset();
});
await test('a delayed pairing code never appears after leaving security',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');let release;
 const request=x.h.request;x.h.request=c=>c.op==='airsim_open_pairing'?new Promise(resolve=>release=resolve):request(c);
 const opening=x.h.airsimPair();await flush();x.h.go('dashboard');release({code:'042917',expires_at:1793600120,pairing_port:7594});await opening;
 assert.equal(x.$('c-as-code').hidden,true);assert.equal(x.$('c-as-digits').textContent,'');x.h.reset();
});
await test('polling preserves unsaved audio quality and makes no configure request',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');x.h.tick();await flush();x.$('c-as-rate').value='16000';
 x.advance(5000);x.h.tick();await flush();assert.equal(x.$('c-as-rate').value,'16000');assert.equal(x.seen.filter(c=>c.op==='airsim_configure').length,0);x.h.reset();
});
await test('three failed reads stop the watch, a manual read can restart it',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');x.fail('核心拒绝操作');
 for(let i=0;i<4;i++){x.h.tick();await flush();x.advance(5000);}assert.equal(x.reads(),3);
 x.fail(null);x.$('c-as-load').listeners.click();await flush();assert.equal(x.reads(),4);assert.match(x.$('c-as-msg').textContent,/已读取设备状态/);x.h.reset();
});
await test('watch has both a time limit and a finite read budget',async()=>{
 const x=fixture();await x.h.signedIn();x.h.go('security');
 for(let i=0;i<40;i++){x.h.tick();await flush();x.advance(5000);}assert.equal(x.reads(),30);
 x.h.go('dashboard');x.h.go('security');x.advance(150001);x.h.tick();await flush();assert.equal(x.reads(),30);x.h.reset();
});
console.log(JSON.stringify({mode:before?'before':'current',passed,failed}));
if(before)assert(failed>=3,'old-source negative control must reproduce the regressions');
