// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const load=async name=>import('data:text/javascript;base64,'+(await readFile(new URL('../crates/edge/src/web/'+name,import.meta.url))).toString('base64'));
const {TestCenter}=await load('test-center.js');
const {CallTest}=await load('call-test.js');
const {archiveEndedOperation}=await load('call-test.js');
const turn=()=>new Promise(resolve=>setImmediate(resolve));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {resolve,reject,promise};};
class Element {
 constructor(tag='div'){this.tagName=tag;this.id='';this.children=[];this.textContent='';this.dataset={};this.attributes={};this.classList={add(){}};}
 append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;}
 setAttribute(k,v){this.attributes[k]=v;}
 set innerHTML(_){throw Error('Never parse device/controller messages as HTML');}
}
const all=['check','outgoing','incoming','sms','forwarding','stop','recover'];
const view=(extra={})=>({message:'合成状态',actions:Object.fromEntries(all.map(id=>[id,{enabled:true}])),...extra});
function fixture(actions){const root=new Element();root.id='synthetic-center';const document={createElement:tag=>new Element(tag)};return {root,panel:new TestCenter({document,root,actions})};}
{
 let sends=0;const {panel,root}=fixture(Object.fromEntries(all.map(id=>[id,()=>{sends++;}])));
 assert.ok([...panel.buttons.values()].every(b=>b.disabled));panel.update(view());panel.render();assert.equal(sends,0);
 for(const [id,reason] of [['outgoing','未登录'],['sms','权限不足'],['incoming','SIM 未注册'],['forwarding','配置尚未确认']]){
  const state=view();state.actions[id]={enabled:false,reason};panel.update(state);await panel.run(id);assert.equal(sends,0);assert.equal(panel.reasons.get(id).textContent,reason);
 }
 const message='<img src=x onerror=alert(1)>'.repeat(50);panel.update(view({message}));assert.equal(panel.status.textContent,message);
 assert.equal(root.children.at(-1).tagName,'details');assert.ok(!root.children.at(-1).open);
}
{
 const gate=deferred();let starts=0,queries=0;const {panel}=fixture({outgoing:async()=>{starts++;await gate.promise;},recover:()=>{queries++;}});panel.update(view());
 const run=panel.run('outgoing');await panel.run('outgoing');assert.equal(starts,1);gate.reject(Error('HTTP409 synthetic-secret'));await run;
 assert.match(panel.status.textContent,/未确认/);assert.doesNotMatch(panel.status.textContent,/synthetic-secret/);
 await panel.run('outgoing');assert.equal(starts,1);await panel.run('recover');assert.equal(queries,1);assert.equal(starts,1);
 panel.update(view({unresolved:true}));for(const id of ['outgoing','incoming','sms','forwarding'])assert.equal(panel.buttons.get(id).disabled,true);
}
{
 const run=deferred(),stop=deferred();let stops=0;const {panel}=fixture({outgoing:()=>run.promise,stop:()=>{stops++;return stop.promise;}});panel.update(view());
 const started=panel.run('outgoing'),stopping=panel.run('stop');await panel.run('stop');assert.equal(stops,1);assert.match(panel.status.textContent,/等待控制器确认/);
 run.resolve();await started;stop.reject(Error('hangup lost'));await stopping;assert.match(panel.status.textContent,/停止未确认/);assert.equal(panel.buttons.get('outgoing').disabled,true);await panel.run('stop');assert.equal(stops,1);
}
{
 const gate=deferred();const {panel}=fixture({sms:async scope=>{await gate.promise;assert.equal(scope.isCurrent(),false);scope.update(view({message:'MUST-NOT-REAPPEAR'}));}});panel.update(view());const running=panel.run('sms');panel.reset();gate.resolve();await running;
 assert.match(panel.status.textContent,/已退出/);assert.ok([...panel.buttons.values()].every(b=>b.disabled));
}
// A real CallTest microphone failure passes through the component without preparing/submitting.
{
 const calls=[];let panel;const test=new CallTest({env:{navigator:{mediaDevices:{getUserMedia:async()=>{throw Error('麦克风拒绝');}}},clearTimeout,setTimeout},
  audio:{input:async()=>{throw Error('麦克风拒绝');},stop:async()=>{}},login:async()=>{},request:async c=>{calls.push(c);throw Error('must not submit');},getPending:()=>null,onLease(){},notify:message=>panel.update(view({message}))});
 ({panel}=fixture({outgoing:()=>test.start('outgoing')}));panel.update(view());await panel.run('outgoing');assert.equal(calls.length,0);assert.match(panel.status.textContent,/麦克风拒绝/);
}
// Real query-only reconciliation retains the exact original operation on uncertain history.
{
 const pending={type:'call',action:'dial',id:'original-synthetic-operation',submit_started:true};const calls=[];let cleared=false;
 const {panel}=fixture({recover:()=>archiveEndedOperation({pending,current(){},request:async c=>{calls.push(c);return {operation:{id:pending.id,state:'unknown',call_id:null}};},archivePending:async()=>{throw Error('must not archive');},setPending:async()=>{cleared=true;}})});
 panel.update(view({unresolved:true}));await panel.run('recover');assert.equal(calls.length,1);assert.equal(calls[0].operation_id,pending.id);assert.equal(calls[0].op,'call_operation');assert.equal(cleared,false);
}
// Use actual CallTest pre-submit checks for authorization, registration and HTTP409 failures.
for(const stage of ['permission','registration','conflict']){
 const calls=[];let panel;const stream={getTracks:()=>[{stop(){}}]};
 const test=new CallTest({env:{setTimeout,clearTimeout},audio:{input:async()=>stream,stop:async()=>{}},
  login:async()=>{if(stage==='permission')throw Error('权限不足');},getPending:()=>null,onLease(){},
  request:async c=>{calls.push(c);if(stage==='conflict')throw Error('HTTP409');return c.op==='call_status'?{available:true,active_calls:0}:{status:'searching',line:1};},
  notify:message=>panel.update(view({message}))});
 ({panel}=fixture({outgoing:()=>test.start('outgoing')}));panel.update(view());await panel.run('outgoing');
 assert.ok(calls.every(c=>!['prepare_call','submit_call'].includes(c.op)));assert.match(panel.status.textContent,stage==='permission'?/权限不足/:stage==='registration'?/SIM 尚未就绪/:/HTTP409/);
}
console.log('PASS: actual TestCenter explicit callbacks/default deny, permission reasons, no render submissions, duplicate/lost reply/stop/logout fencing, real CallTest mic rejection and exact query-only operation reconciliation');
