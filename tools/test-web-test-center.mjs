// SPDX-License-Identifier: AGPL-3.0-only
// Execute the production app and formatter, with synthetic IPC/IndexedDB only.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const bytes=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager,smsListText,smsDetailText}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const archiveBytes=await readFile(new URL('../crates/edge/src/web/call-test.js',import.meta.url));
const {archiveEndedOperation}=await import('data:text/javascript;base64,'+archiveBytes.toString('base64'));
const id='00000000-0000-4000-8000-000000000001',other='00000000-0000-4000-8000-000000000002';
const turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;}
 set innerHTML(_){throw Error('Message data must never be parsed as HTML');}
 append(...values){this.children.push(...values);}replaceChildren(...values){this.children=values;this.textContent='';}addEventListener(){}
}
function fixture({respond,onPut,consent=true,loggedIn=true}={}){
 const elements=new Map(),sent=[],saved=new Map(),confirmations=[],starts=[],copied=[];let logins=0;
 const $=name=>{if(!elements.has(name))elements.set(name,new Element());return elements.get(name);};
 const document={hidden:false,getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}};
 let state='submitted';
 const context={document,crypto:webcrypto,TextEncoder,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener(){},
  indexedDB:{open:()=>({})},SmsManager,smsListText,smsDetailText,DailyPhone:class{constructor(){this.managing=false;this.working=false;}reset(){}start(){}},SessionKeeper:class{start(){}reset(){}async ensure(){}},
  AudioLink:class{constructor(){this.state='closed';}async stop(){}},
  ConfigurationIndicator:class{async set(){}},CallTest:class{constructor(options){this.options=options;this.running=false;}async start(mode){starts.push(mode);this.running=true;this.options.notify("synthetic running",true);this.running=false;this.options.notify("synthetic completed",false);}async stop(){this.running=false;}},archiveEndedOperation,navigator:{clipboard:{async writeText(text){copied.push(text);}}},NetworkPanel:class{start(){}stop(){}},EventSync:class{reset(){}},
  confirm:text=>{confirmations.push(text);return consent;},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body),c=body.command;sent.push(c);const result=respond?await respond(c):c.op==='line_status'?{status:'ready',line:7}:c.op==='prepare_sms'?{id,state:'prepared'}:{id,state};return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 context.loginCount=0;
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={runTestCenter,copyTestResult,reconcileTestPending,setAudio:value=>{audioReady=value;showPending();},clearLogin:()=>{token=null;},mockLogin:()=>{login=async()=>{globalThis.loginCount++;token="synthetic-session";};},dailySendSms,submit,query,closeSms,inboxPage,act,showPending,loginAs:()=>{token="synthetic-session";},setDb:value=>{db=value;},setPending:value=>{pending=value;showPending();},pending:()=>pending,logout:()=>$("logout").onclick()};',context);
 context.fixture.setDb({transaction(){
  const tx={};tx.objectStore=()=>({get:key=>{const request={};queueMicrotask(()=>{request.result=saved.get(key);request.onsuccess?.();});return request;},put:(value,key)=>{
   const snapshot=structuredClone(value);const completion=onPut?.(key,snapshot);queueMicrotask(async()=>{try{await completion;saved.set(key,snapshot);tx.oncomplete?.();}catch{tx.onerror?.();}});
  }});return tx;
 }});
 if(loggedIn)context.fixture.loginAs();$('recipient').value='12025550123';$('text').value='合成测试 <img src=x> 🙂';
 return {...context.fixture,$,sent,saved,confirmations,starts,copied,loginCount:()=>context.loginCount,setState:value=>{state=value;}};
}
// The actual HTML keeps the test center before any closed advanced panel.
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(match=>match[1]);
assert.equal(new Set(ids).size,ids.length,'No duplicate IDs after moving existing controls');
const center=html.slice(html.indexOf('<section id="test-center"'),html.indexOf('<details id="account-panel"'));
assert.equal([...center.matchAll(/class="primary"/g)].length,4);
for(const name of ['test-check','test-start','test-incoming-start','test-sms','test-stop','test-copy','test-recover'])assert.match(center,new RegExp('id="'+name+'"'));
for(const name of ['phone-panel','sms-panel','account-panel'])assert.match(html,new RegExp('<details id="'+name+'">'));
assert.equal((html.match(/<details[^>]*\sopen(?:>|\s)/g)||[]).length,1,'Only the nested composer is open inside its closed SMS panel');
{
 const f=fixture({loggedIn:false,respond:async c=>c.op==='line_status'?{status:'ready',line:7}:c.op==='call_status'?{available:true,active_calls:0}:{devices:[{kind:'wifi',state:100},{kind:'ethernet',state:100}]}});f.mockLogin();
 await f.$('test-check').onclick();assert.equal(f.loginCount(),1);assert.deepEqual(f.sent.map(c=>c.op),['line_status','call_status','network_status']);
 assert.match(f.$('test-state').textContent,/SIM 已就绪/);assert.match(f.$('test-state').textContent,/蜂窝数据未连接/);assert.equal(f.$('test-stop').disabled,true);
}
{
 const f=fixture();await f.$('test-sms').onclick();assert.equal(f.confirmations.length,2,'Preserve the synthetic existing draft, then confirm the actual SMS');
 assert.equal(f.saved.get('test-center-saved-draft').text,'合成测试 <img src=x> 🙂');
 const prepare=f.sent.find(c=>c.op==='prepare_sms');assert.equal(prepare.recipient,'10010');assert.equal(prepare.text,'CXYE');
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,1);assert.equal(f.pending().state,'submitted');
 // A malformed inbox response must never trigger another send.
 assert.match(f.$('test-state').textContent,/测试停止/);assert.equal(f.$('test-recovery').hidden,false);
}
{
 const f=fixture({consent:false});f.$('text').value='';f.$('recipient').value='';await f.$('test-sms').onclick();assert.equal(f.sent.length,0);assert.match(f.$('test-state').textContent,/未发送/);
}
for(const state of ['unknown','submitting','prepared']){
 const old={type:'sms',action:'send',id,state,submit_started:true,token:'MUST-NOT-COPY',recipient:'MUST-NOT-COPY',text:'MUST-NOT-COPY',lease:{secret:'MUST-NOT-COPY'}};
 const f=fixture({respond:async c=>({id,state})});f.setPending({...old});await f.$('test-sms').onclick();assert.equal(f.pending().id,id);assert.equal(f.sent.filter(c=>c.op==='prepare_sms'||c.op==='submit_sms').length,0);
 await f.copyTestResult();assert.equal(f.copied.length,1);assert.doesNotMatch(f.copied[0],/MUST-NOT-COPY|synthetic-session/);assert.equal(JSON.parse(f.copied[0]).operation.operation_id,id);
}
{
 const f=fixture({respond:async()=>{throw Error('synthetic query timeout');}});f.setPending({type:'call',action:'hangup',id,state:'prepared',submit_started:true});
 await f.$('test-recover').onclick();assert.equal(f.pending().id,id);assert.equal(f.sent.length,1);assert.match(f.$('test-state').textContent,/timeout/);
}
for(const reason of ['active','wrong-call','not-ended','missing-call','wrong-id','archive-storage']){
 const f=fixture({respond:async c=>c.op==='call_operation'?{operation:{id:reason==='wrong-id'?other:id,state:'unknown',call_id:reason==='missing-call'?null:other}}:c.op==='call_status'?{available:true,active_calls:reason==='active'?1:0}:{id:reason==='wrong-call'?id:other,state:reason==='not-ended'?'active':'ended',availability:'observed'},onPut:(key)=>{if(reason==='archive-storage'&&key==='ended-hangup-receipts')throw Error('synthetic storage failure');}});
 f.setPending({type:'call',action:'dial',id,state:'unknown',submit_started:true});await f.$('test-recover').onclick();assert.equal(f.pending().id,id,reason);assert.equal(f.sent.filter(c=>c.op.startsWith('submit')||c.op.startsWith('prepare')).length,0);
}
for(const state of ['ended','reconciling']){
 const f=fixture({respond:async c=>c.op==='call_operation'?{operation:{id,state:'unknown',call_id:other}}:c.op==='call_status'?{available:true,active_calls:0}:{id:other,state,availability:state==='ended'?'observed':'reconciling',modem_state:7}});
 f.setPending({type:'call',action:'dial',id,state:'unknown',submit_started:true,prior_operation:{id:'synthetic-prior'}});await f.$('test-recover').onclick();assert.equal(f.pending(),null);
 assert.equal(f.saved.get('ended-hangup-receipts')[0].prior_operation.id,'synthetic-prior');assert.equal(f.starts.length,0);assert.match(f.$('test-state').textContent,/已核对/);
}
{
 const f=fixture({respond:async()=>({id,state:'prepared'})});f.setPending({type:'sms',action:'send',id,state:'prepared',submit_started:false});await f.$('test-recover').onclick();assert.equal(f.pending(),null);assert.equal(f.confirmations.length,1);assert.equal(f.saved.get('ended-hangup-receipts')[0].local_discarded,true);assert.equal(f.sent.length,1);
}
{
 const f=fixture({respond:async c=>c.op==='call_operation'?{operation:{id,state:'submitted',call_id:other}}:c.op==='call_status'?{available:true,active_calls:1}:null});f.setPending({type:'call',action:'dial',id,state:'submitted',submit_started:true});await f.$('test-recover').onclick();assert.equal(f.pending().id,id);assert.equal(f.starts.length,0);
}
for(const task of ['test-start','test-incoming-start']){
 const f=fixture();f.setAudio(true);await f.$(task).onclick();assert.deepEqual(f.starts,[task==='test-start'?'outgoing':'incoming']);assert.equal(f.$('test-stop').disabled,true);assert.equal(f.$('test-check').disabled,false);
}
for(const stage of ['line_status','prepare_sms','marker']){
 const gate=deferred();const f=fixture({respond:async c=>c.op===stage?gate.promise:c.op==='line_status'?{status:'ready',line:7}:c.op==='prepare_sms'?{id,state:'prepared'}:{id,state:'submitted'},onPut:(key,value)=>stage==='marker'&&key==='pending'&&value?.submit_started===true?gate.promise:undefined});f.$('text').value='';f.$('recipient').value='';
 const sending=f.$('test-sms').onclick();await turn();await f.$('test-sms').onclick();f.$('test-stop').onclick();gate.resolve(stage==='marker'?undefined:stage==='line_status'?{status:'ready',line:7}:{id,state:'prepared'});await sending;
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0,stage);assert.equal(f.confirmations.length,1);if(stage!=='line_status')assert.equal(f.pending().id,id);
}
{
 const gate=deferred();const f=fixture({respond:async c=>c.op==='line_status'?gate.promise:{id,state:'prepared'}});f.$('text').value='';f.$('recipient').value='';const sending=f.$('test-sms').onclick();await turn();f.logout();gate.resolve({status:'ready',line:7});await sending;assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0);
}
console.log('PASS: visible four-task center, collapsed panels, automatic login, exact no-retry recovery, safe result copy, one SMS submit, draft preservation and stop/logout/storage races; synthetic only.');
