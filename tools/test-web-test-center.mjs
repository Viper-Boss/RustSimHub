// SPDX-License-Identifier: AGPL-3.0-only
import {installAppImports,appVmSource,assertNknAssembly} from './fixtures/web3-web/app-imports.mjs';
import { NasBridgePanel, pendingInput as nasPendingInput, receiptInput as nasReceiptInput } from '../crates/edge/src/web/nas-bridge.js';
// Execute the production app and formatter, with synthetic IPC/IndexedDB only.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const forwardingModule=await readFile(new URL('../crates/edge/src/web/forwarding.js',import.meta.url));
const {ForwardingPanel,ForwardingQuickTest,ForwardingHistory,historyPendingInput,pendingInput:forwardingPendingInput}=await import('data:text/javascript;base64,'+forwardingModule.toString('base64'));
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const bytes=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager,smsListText,smsDetailText}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const archiveBytes=await readFile(new URL('../crates/edge/src/web/call-test.js',import.meta.url));
const {archiveEndedOperation}=await import('data:text/javascript;base64,'+archiveBytes.toString('base64'));
const centerBytes=await readFile(new URL('../crates/edge/src/web/test-center.js',import.meta.url));
const {TestCenter}=await import('data:text/javascript;base64,'+centerBytes.toString('base64'));
const supportLoad=async name=>import('data:text/javascript;base64,'+(await readFile(new URL('../crates/edge/src/web/'+name+'.js',import.meta.url))).toString('base64'));
const {OperationReceipt}=await supportLoad('operation-receipt'),{DiagnosticPanel}=await supportLoad('diagnostics'),{ConfigurationState}=await supportLoad('configuration-state'),{UsageStatsPanel}=await supportLoad('usage-stats');
const id='00000000-0000-4000-8000-000000000001',other='00000000-0000-4000-8000-000000000002';
const turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;this.style={};this.dataset={};this.classList={add(){}};this.attributes={};}
 set innerHTML(_){throw Error('Message data must never be parsed as HTML');}
 append(...values){this.children.push(...values);}replaceChildren(...values){this.children=values;this.textContent='';}addEventListener(){}
 setAttribute(k,v){this.attributes[k]=v;}
 get options(){return this.children;}
}
function fixture({respond,onPut,consent=true,loggedIn=true,callGate=null,clock={mono:100,wall:10000}}={}){
 const listeners=new Map();const elements=new Map(),sent=[],saved=new Map(),confirmations=[],starts=[],copied=[];let logins=0;
 const $=name=>{if(!elements.has(name)){const e=new Element();e.id=name;elements.set(name,e);}return elements.get(name);};
 const visibilityListeners=[];
 const document={hidden:false,getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(type,fn){if(type==='visibilitychange')visibilityListeners.push(fn);}};
 let state='submitted';
 const context={OperationReceipt,DiagnosticPanel,ConfigurationState,UsageStatsPanel,TestCenter,listeners,ForwardingPanel,ForwardingQuickTest,ForwardingHistory,historyPendingInput,forwardingPendingInput,document,crypto:webcrypto,TextEncoder,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener:(type,fn)=>{if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push(fn);},
  indexedDB:{open:()=>({})},SmsManager,smsListText,smsDetailText,DailyPhone:class{constructor(){this.managing=false;this.working=false;}reset(){}start(){}},NasBridgePanel,nasPendingInput,nasReceiptInput,ClientsPanel:class{reset(){}},SessionKeeper:class{start(){}reset(){}async ensure(){}},
  AudioLink:class{constructor(){this.state='closed';}async stop(){}},
  ConfigurationIndicator:class{async set(){}},CallTest:class{constructor(options){this.options=options;this.running=false;}async start(mode){starts.push(mode);this.running=true;this.options.notify("synthetic running",true);if(callGate)await callGate.promise;this.running=false;this.options.notify("synthetic completed",false);}async stop(){this.running=false;}},archiveEndedOperation,navigator:{clipboard:{async writeText(text){copied.push(text);}}},NetworkPanel:class{start(){}stop(){}},EventSync:class{reset(){}},
  Date:class extends Date{static now(){return clock.wall;}},performance:{now:()=>clock.mono},confirm:text=>{confirmations.push(text);return consent;},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body),c=body.command;sent.push(c);const result=respond?await respond(c):c.op==='line_status'?{status:'ready',line:7}:c.op==='prepare_sms'?{id,state:'prepared'}:{id,state};return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 context.loginCount=0;
 installAppImports(context);
 vm.runInNewContext(appVmSource(app)+'\nglobalThis.fixture={nkn:()=>nknProductPanel,support:()=>supportPanels,querySupportOperation,center:()=>testCenterComponent,hide:()=>{for(const fn of listeners.get("pagehide")||[])fn();},show:()=>{for(const fn of listeners.get("pageshow")||[])fn();},setAp:value=>{apConfigurator=value;showTestControls();},forwarding:()=>forwardingPanel,runTestCenter,copyTestResult,reconcileTestPending,setAudio:value=>{audioReady=value;showPending();},clearLogin:()=>{token=null;showPending();},mockLogin:()=>{login=async()=>{globalThis.loginCount++;token="synthetic-session";};},dailySendSms,submit,query,closeSms,inboxPage,act,showPending,loginAs:()=>{token="synthetic-session";},setDb:value=>{db=value;},setPending:value=>{pending=value;showPending();},pending:()=>pending,logout:()=>$("logout").onclick()};',context);
 assertNknAssembly(context);
 context.fixture.setDb({transaction(){
  const tx={abort(){this.onabort?.();}};tx.objectStore=()=>({get:key=>{const request={};queueMicrotask(()=>{request.result=saved.get(key);request.onsuccess?.();});return request;},put:(value,key)=>{
   const snapshot=structuredClone(value);const completion=onPut?.(key,snapshot);queueMicrotask(async()=>{try{await completion;saved.set(key,snapshot);tx.oncomplete?.();}catch{tx.onerror?.();}});
  }});return tx;
 }});
 if(loggedIn){context.fixture.loginAs();context.fixture.setPending(null);}$('recipient').value='12025550123';$('text').value='合成测试 <img src=x> 🙂';
 return {...context.fixture,$,sent,saved,confirmations,starts,copied,loginCount:()=>context.loginCount,setState:value=>{state=value;},visibility:hidden=>{document.hidden=hidden;for(const fn of visibilityListeners)fn();}};
}
// The actual HTML keeps the test center before any closed advanced panel.
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(match=>match[1]);
assert.equal(new Set(ids).size,ids.length,'No duplicate IDs after moving existing controls');
const center=html.slice(html.indexOf('<section id="test-center"'),html.indexOf('<details id="account-panel"'));
assert.equal([...center.matchAll(/class="primary"/g)].length,5,'Keep legacy handler nodes, hidden behind one mounted component');
assert.match(center,/id="test-component-host"/);
for(const id of ['test-check','test-start','test-incoming-start','test-sms','test-forwarding','test-stop','test-recover'])assert.match(center,new RegExp('<button hidden id="'+id+'"'));
for(const name of ['test-check','test-start','test-incoming-start','test-sms','test-forwarding','test-stop','test-copy','test-recover'])assert.match(center,new RegExp('id="'+name+'"'));
for(const name of ['phone-panel','sms-panel','account-panel'])assert.match(html,new RegExp('<details id="'+name+'">'));
assert.equal((html.match(/<details[^>]*\sopen(?:>|\s)/g)||[]).length,1,'Only the nested composer is open inside its closed SMS panel');
{
 const f=fixture({loggedIn:false,respond:async c=>c.op==='line_status'?{status:'ready',line:7}:c.op==='call_status'?{available:true,active_calls:0}:{devices:[{kind:'wifi',state:100},{kind:'ethernet',state:100}]}});f.mockLogin();
 await f.$('test-check').onclick();assert.equal(f.loginCount(),1);assert.deepEqual(f.sent.map(c=>c.op),['line_status','call_status','network_status']);
 assert.match(f.$('test-state').textContent,/SIM 已就绪/);assert.match(f.$('test-state').textContent,/蜂窝数据未连接/);assert.equal(f.$('test-stop').disabled,true);
}
{
 const f=fixture();await f.center().run('sms');assert.equal(f.confirmations.length,2,'Preserve the synthetic existing draft, then confirm the actual SMS');
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
{
 const f=fixture();f.saved.set('test-center-saved-draft',{recipient:'12025550199',text:'合成旧草稿'});await f.$('test-restore-draft').onclick();assert.equal(f.$('recipient').value,'12025550199');assert.equal(f.$('text').value,'合成旧草稿');f.logout();assert.equal(f.saved.get('test-center-saved-draft').text,'合成旧草稿');assert.equal(f.sent.length,0);
}
{
 const f=fixture({consent:false});f.saved.set('test-center-saved-draft',{recipient:'12025550199',text:'另一份合成草稿'});await f.$('test-restore-draft').onclick();assert.equal(f.$('recipient').value,'12025550123');assert.equal(f.saved.get('test-center-saved-draft').text,'另一份合成草稿');assert.equal(f.sent.length,0);
}
{
 let savedPolicy=null,gen=0,writeGate=null;
 const state=()=>({available:true,configuration_api:true,retained_credentials_api:true,request_receipt_api:true,service_management_api:true,enabled:false,generation:gen,state:'disabled',process_state_verified:true});
 const f=fixture({respond:async c=>{
  if(c.op==='forwarding_status')return state();
  if(c.op==='forwarding_rules')return {enabled:false,generation:gen,targets:savedPolicy?.targets.map(t=>({id:t.id,kind:t.destination.kind,credentials_redacted:true}))||[],rules:savedPolicy?.rules||[]};
  if(c.op==='forwarding_configure'){if(writeGate)return writeGate.promise;savedPolicy=structuredClone(c.policy);gen++;return {...state(),configuration_applied:true,request_id:c.request_id};}
  throw Error('unexpected request');
 }});
 const panel=f.forwarding();await panel.action(g=>panel.read(g));const input=panel.targets[0].endpointInput;
 input.value='https://example.invalid/synthetic-secret';await panel.action(g=>panel.save(g));
 assert.equal(f.sent.filter(c=>c.op==='forwarding_configure').length,1);assert.equal(f.saved.get('forwarding-pending'),null);
 assert.equal(f.saved.get('forwarding-config-receipts').length,1);assert.ok(!JSON.stringify([...f.saved.entries()]).includes('synthetic-secret'));
 assert.equal(input.value,'');
 const external={format:1,id:other,kind:'disable',expected_generation:1};f.saved.set('forwarding-pending',external);
 await panel.action(g=>panel.save(g));assert.equal(f.sent.filter(c=>c.op==='forwarding_configure').length,1);assert.deepEqual(f.saved.get('forwarding-pending'),external,'Do not overwrite another tab reservation');
 f.saved.set('forwarding-pending',null);writeGate=deferred();const sending=panel.action(g=>panel.save(g));
 while(f.sent.filter(c=>c.op==='forwarding_configure').length<2)await turn();
 const submitted=f.sent.filter(c=>c.op==='forwarding_configure').at(-1);f.saved.set('forwarding-pending',external);gen=2;
 writeGate.resolve({...state(),configuration_applied:true,request_id:submitted.request_id});await sending;
 assert.deepEqual(f.saved.get('forwarding-pending'),external,'A stale completion must not clear another tab reservation');
 assert.equal(f.saved.get('forwarding-config-receipts').length,2);f.logout();assert.equal(panel.targets.length,0);assert.equal(panel.pending,null);
 assert.equal(f.sent.filter(c=>c.op==='submit_sms'||c.op==='submit_call').length,0);
}
console.log('PASS: preserved five-task handlers behind a single mounted center, collapsed panels, automatic login, exact no-retry recovery, safe result copy, one SMS submit, draft preservation, forwarding real app/IndexedDB wiring and stop/logout/storage races; synthetic only.');

{
 const policy={enabled:true,generation:5,targets:[{id,kind:'webhook',credentials_redacted:true}],rules:[{id:other,targets:[id],sender_equals:null,body_contains:null,line:null,share:'notice'}]};
 const f=fixture({respond:async c=>{
  if(c.op==='forwarding_status')return {available:true,configuration_api:true,retained_credentials_api:true,request_receipt_api:true,service_management_api:true,synthetic_test_api:true,delivery_api:true,enabled:true,generation:5,state:'running',process_state_verified:true};
  if(c.op==='forwarding_rules')return policy;
  if(c.op==='forwarding_test')return {request_id:c.request_id,accepted:true,synthetic:true,receipt:{id:c.request_id,target_id:id,generation:5,synthetic:true,state:'queued'}};
  if(c.op==='forwarding_test_request')return {request_id:c.request_id,known:true,receipt:{id:c.request_id,target_id:id,generation:5,synthetic:true,state:'confirmed'}};
  throw Error('unexpected request');
 }});
 const panel=f.forwarding();panel.delay=async()=>{};await panel.action(g=>panel.read(g));await panel.action(g=>panel.test(g));
 assert.equal(f.sent.filter(c=>c.op==='forwarding_test').length,1);assert.equal(f.saved.get('forwarding-pending'),null);
 const record=f.saved.get('forwarding-config-receipts').at(-1);assert.equal(record.kind,'test');assert.equal(record.target_id,id);assert.equal(record.applied_generation,5);
 assert.equal(f.sent.filter(c=>c.op==='submit_sms'||c.op==='submit_call').length,0);
}

{
 let admitted;
 const f=fixture({loggedIn:false,respond:async c=>{
  if(c.op==='forwarding_status')return {quick_test_api:true,service_management_api:true,enabled:false,state:'disabled',process_state_verified:true,quick_test_active:false,generation:1};
  if(c.op==='forwarding_rules')return {enabled:false,generation:1,targets:[{id,kind:'bark',credentials_redacted:true}]};
  if(c.op==='forwarding_quick_test'){
   assert.equal(f.saved.get('forwarding-pending').id,c.request_id);admitted=c;
   return {request_id:c.request_id,accepted:true,quick_test:true};
  }
  assert.equal(c.op,'forwarding_test_request');
  return {request_id:c.request_id,known:true,quick_active:false,automatic_stop_verified:true,
   receipt:{id:c.request_id,target_id:id,generation:1,synthetic:true,state:'confirmed'}};
 }});f.mockLogin();
 await f.center().run('forwarding');
 assert.equal(f.loginCount(),1);assert.equal(f.sent.filter(c=>c.op==='forwarding_quick_test').length,1);
 assert.equal(f.saved.get('forwarding-pending'),null);
 const record=f.saved.get('forwarding-config-receipts').at(-1);
 assert.equal(record.kind,'quick_test');assert.equal(record.id,admitted.request_id);assert.equal(record.applied_generation,1);
 assert.match(f.$('test-state').textContent,/Bark 已确认.*自动关闭/);
 assert.equal(f.$('test-forwarding').disabled,false);assert.equal(f.$('test-stop').disabled,true);
 await f.$('test-copy').onclick();while(!f.copied.length)await turn();assert.equal(JSON.parse(f.copied[0]).test,'forwarding');
 assert.ok(f.sent.every(c=>!['set_forwarding_enabled','forwarding_configure','submit_sms','submit_call'].includes(c.op)));
}
console.log('PASS: actual homepage Bark button, automatic Owner login, shared IndexedDB reservation, exact quick-test archive and copy result; no switch/config/SMS/call writes.');

// The real app gates all one-click tests while an AP transaction is unresolved.
for(const state of [{working:true,managing:false},{working:false,managing:true}]){
 const f=fixture();f.setAp(state);
 for(const name of ['test-start','test-sms','test-forwarding']){
  await f.$(name).onclick();assert.equal(f.sent.length,0);assert.equal(f.starts.length,0);
  assert.match(f.$('notice').textContent+f.$('test-state').textContent,/Wi-Fi 配置区/);
 }
 let ran=false;await f.act(async()=>{ran=true;});assert.equal(ran,false);
 await f.runTestCenter('sms');assert.match(f.$('test-state').textContent,/Wi-Fi 配置区/);assert.equal(f.sent.length,0);
}

// The actual component delegates to the actual app handlers; IPC remains synthetic.
{
 const f=fixture({respond:async c=>c.op==='line_status'?{status:'ready',line:7}:c.op==='call_status'?{available:true,active_calls:0}:{devices:[]}});
 await f.center().run('check');assert.deepEqual(f.sent.map(c=>c.op),['line_status','call_status','network_status']);assert.match(f.center().status.textContent,/检查完成/);
}
{
 const f=fixture({respond:async()=>({id,state:'unknown'})});f.setPending({type:'sms',id,state:'unknown',submit_started:true});
 await f.center().run('sms');assert.equal(f.sent.length,0);await f.center().run('recover');assert.deepEqual(f.sent.map(c=>c.op),['sms_operation']);assert.equal(f.pending().id,id);
}
{
 const gate=deferred(),f=fixture({callGate:gate});f.setAudio(true);const running=f.center().run('outgoing');await turn();assert.equal(f.starts.length,1);assert.equal(f.center().buttons.get('stop').disabled,false);await f.center().run('stop');f.hide();gate.resolve();await running;
 assert.match(f.center().status.textContent,/已退出/);assert.ok([...f.center().buttons.values()].every(b=>b.disabled));const count=f.sent.length;f.show();assert.equal(f.center().buttons.get('check').disabled,false);assert.equal(f.sent.length,count);
}
console.log('PASS: actual TestCenter mounted by actual app, original check handlers, unknown exact-ID recovery and late pagehide fencing');

{
 const f=fixture();f.setAp({working:true});assert.equal(f.center().buttons.get('sms').disabled,true);assert.match(f.center().reasons.get('sms').textContent,/Wi-Fi/);await f.center().run('sms');assert.equal(f.sent.length,0);
}

{
 const gate=deferred(),f=fixture({respond:async c=>c.op==='line_status'?{status:'ready',line:7}:gate.promise});const running=f.center().run('check');await turn();f.logout();gate.resolve({available:true,active_calls:0});await running;assert.match(f.center().status.textContent,/已退出/);assert.doesNotMatch(f.center().status.textContent,/检查完成/);
}

// All five display modules are real; only the existing IPC/storage boundaries are synthetic.
{
 const f=fixture({respond:async()=>({id,state:'submitted'})});assert.ok(f.support().usage.button.disabled);assert.equal(f.sent.length,0);
 f.$('pin').value='a'.repeat(64);f.setPending({type:'sms',id,state:'unknown',submit_started:true,recipient:'MUST-NOT-COPY',text:'MUST-NOT-COPY',token:'MUST-NOT-COPY'});
 assert.equal(f.support().receipt.buttons.query.disabled,false);assert.equal(f.support().receipt.buttons.archive.disabled,true);assert.equal(f.support().diagnostics.queryButton.disabled,true);
 await f.support().diagnostics.copyResult();assert.equal(f.copied.length,1);assert.doesNotMatch(f.copied[0],/MUST-NOT-COPY|aaaaaaaa|synthetic-session/);const diagnostic=JSON.parse(f.copied[0]);assert.equal(diagnostic.session,'unknown');assert.equal(diagnostic.certificate,'unknown');
 await f.support().receipt.run('query');assert.deepEqual(f.sent.map(c=>c.op),['sms_operation']);assert.equal(f.sent[0].operation_id,id);assert.equal(f.support().receipt.view.state,'submitted');assert.equal(f.pending().id,id);
 await assert.rejects(f.querySupportOperation(other));assert.equal(f.sent.length,1);
 f.setPending({type:'sms',id,state:'saved',submit_started:true});assert.equal(f.support().receipt.view.state,'unknown');
}
{
 const gate=deferred(),f=fixture({respond:()=>gate.promise});f.setPending({type:'call',id,state:'unknown',submit_started:true});const reading=f.support().receipt.run('query');await turn();assert.equal(f.sent.length,1);await f.support().receipt.run('query');assert.equal(f.sent.length,1);f.logout();gate.resolve({operation:{id,state:'submitted'}});await reading;assert.equal(f.support().receipt.view.state,'unknown');assert.ok(f.support().receipt.buttons.query.disabled);
}
{
 const f=fixture();f.setPending({type:'sms',id,state:'unknown',submit_started:true});f.setAp({working:true,suspend(){}});assert.ok(f.support().receipt.buttons.query.disabled);await f.support().receipt.run('query');assert.equal(f.sent.length,0);
 f.hide();assert.equal(f.support().receipt.view,null);f.show();assert.equal(f.sent.length,0);assert.ok(f.support().usage.button.disabled);
}
console.log('PASS: actual receipt exact-ID/duplicate/logout/AP guards, redacted copy without inferred authority, archive disabled and no unavailable-provider reads');

// Use the existing Core forwarding_status read and its request-causal proof.
const verifiedForwarding={available:true,configuration_api:true,service_management_api:true,service_action:'completed',process_state_verified:true,enabled:true,state:'running',generation:1,configuration_applied:true,token:'MUST-NOT-COPY'};
for(const enabled of [true,false]){
 const clock={mono:100,wall:10000},f=fixture({clock,respond:async c=>{assert.equal(c.op,'forwarding_status');clock.mono+=127;clock.wall+=139;return {...verifiedForwarding,enabled,state:enabled?'running':'disabled'};}}),panel=f.support().configuration;
 assert.equal(f.sent.length,0);assert.equal(panel.button.disabled,false);await panel.refresh();assert.deepEqual(f.sent.map(c=>c.op),['forwarding_status']);
 assert.match(panel.output.textContent,enabled?/服务运行已核对/:/服务关闭已核对/);assert.match(panel.output.textContent,/本次保存意图尚未确认/);assert.equal(panel.snapshot.age_ms,279);assert.equal(panel.snapshot.data.configuration_applied,false);assert.doesNotMatch(JSON.stringify(panel.snapshot),/MUST-NOT-COPY/);
 clock.wall+=5001;panel.render();assert.match(panel.output.textContent,/陈旧或未知/);panel.reset();
}
for(const change of [{service_management_api:false},{service_action:'failed'},{service_action:'unconfirmed'},{process_state_verified:false},{enabled:false},{generation:-1},{state:'awaiting_service'},{available:false},{configuration_api:false}]){
 const f=fixture({respond:async()=>({...verifiedForwarding,...change})}),panel=f.support().configuration;await panel.refresh();assert.doesNotMatch(panel.output.textContent,/服务运行已核对|服务关闭已核对/);assert.equal(panel.snapshot,null);assert.equal(f.sent.length,1);panel.reset();
}
for(const [mono,wall] of [[3000,100],[100,3000],[-1,100],[100,-1],[NaN,100],[100,NaN]]){
 const clock={mono:100,wall:10000},f=fixture({clock,respond:async()=>{clock.mono+=mono;clock.wall+=wall;return verifiedForwarding;}}),panel=f.support().configuration;await panel.refresh();assert.equal(panel.snapshot,null);assert.doesNotMatch(panel.output.textContent,/服务运行已核对/);assert.equal(f.sent.length,1);panel.reset();
}
{
 const f=fixture({loggedIn:false}),panel=f.support().configuration;assert.ok(panel.button.disabled);await panel.refresh();assert.equal(f.sent.length,0);
}
for(const end of ['logout','hide','visibility']){
 const gate=deferred(),f=fixture({respond:()=>gate.promise}),panel=f.support().configuration;const reading=panel.refresh();await turn();await panel.refresh();assert.equal(f.sent.length,1);
 if(end==='visibility')f.visibility(true);else f[end]();gate.resolve(verifiedForwarding);await reading;assert.equal(panel.snapshot,null);assert.doesNotMatch(panel.output.textContent,/服务运行已核对/);
 if(end==='visibility')f.visibility(false);else if(end==='hide')f.show();assert.equal(f.sent.length,1);if(end==='logout')assert.ok(panel.button.disabled);panel.reset();
}
{
 const gate=deferred(),f=fixture({respond:()=>gate.promise}),panel=f.support().configuration;const reading=panel.refresh();await turn();panel.reset();await panel.refresh();assert.equal(f.sent.length,1,'An aborted UI read must not overlap the still-pending HTTP read');gate.resolve(verifiedForwarding);await reading;assert.equal(panel.snapshot,null);panel.reset();
}
{
 const gate=deferred(),f=fixture({respond:()=>gate.promise}),panel=f.support().configuration;const reading=panel.refresh();await turn();f.clearLogin();f.loginAs();f.setPending(null);gate.resolve(verifiedForwarding);await reading;assert.equal(panel.snapshot,null,'Even a replacement session with the same synthetic token must fence an old read');panel.reset();
}
console.log('PASS: actual configuration reader, request-causal proof, conservative RTT age, no save inference, malformed/late/clock failures, no overlapping reads or automatic requests and logout/page/visibility fences');

// The actual app's NKN construction/storage/logout path, not a second state machine.
const nknContext='a'.repeat(64),nknId='11111111-1111-4111-8111-111111111111';
const nknReport=(sequence,revision='0',manual=false)=>({available:true,configuration_api:true,remote_control_supported:false,revision,provider:{contract:1,core_instance:nknContext,owners:{manual,remote:false,statistics:false},supervisor_state:'ready',root_generation:1,original_process_exit_verified:false,status:{contract:1,source:'engine',generation:1,sequence,age_ms:0,enabled:'on',service:'running',path:'unknown',dht:'unknown',rx_bytes:null,tx_bytes:null,rtt_ms:null,peer_count:null,last_error:null}}});
{
 let sequence=0,manual=false;
 const f=fixture({respond:async c=>{
  if(c.op==='nkn_status')return nknReport(++sequence,manual?'1':'0',manual);
  if(c.op==='set_nkn_enabled'){
   assert.equal(c.owner,'manual_engine');assert.equal(c.enabled,true);assert.equal(c.expected_revision,'0');assert.equal(c.expected_core_instance,nknContext);
   const durable=f.saved.get('nkn-control-pending');assert.equal(durable.id,c.request_id,'actual app metadata must be durable before IPC');
   manual=true;return {request_id:c.request_id,known:true,state:'accepted',core_instance:nknContext,owner:c.owner,enabled:c.enabled,expected_revision:'0',admitted_revision:'1',not_found_is_not_proof:true};
  }
  throw Error('Unexpected NKN assembly command '+c.op);
 }}),panel=f.nkn();
 assert.equal(f.sent.length,0);await panel.buttons.read.onclick();assert.deepEqual(f.sent.map(c=>c.op),['nkn_status']);assert(panel.buttons.remote_on.disabled);assert(!panel.buttons.manual_on.disabled);
 await panel.buttons.manual_on.onclick();assert.deepEqual(f.sent.map(c=>c.op),['nkn_status','set_nkn_enabled','nkn_status']);
 assert.equal(f.saved.get('nkn-control-pending'),null);const archive=f.saved.get('nkn-control-receipts');assert.equal(archive.length,1);assert.equal(archive[0].state,'accepted');assert.equal(archive[0].record.owner,'manual_engine');
 const epoch=panel.epoch;f.logout();assert(panel.epoch>epoch);assert.equal(panel.sample,null);assert(panel.buttons.remote_on.disabled);assert.equal(f.sent.length,3);
 for(const [name,button] of Object.entries(panel.buttons))assert.equal(button.disabled,true,`NKN ${name} must be disabled after the actual app logout`);
}
for(const end of ['logout','hide','visibility']){
 const gate=deferred(),f=fixture({respond:async c=>{assert.equal(c.op,'nkn_status');return gate.promise;}}),panel=f.nkn();
 const reading=panel.buttons.read.onclick();await turn();assert.equal(f.sent.length,1);
 if(end==='visibility')f.visibility(true);else f[end]();gate.resolve(nknReport(1));await reading;
 assert.equal(panel.sample,null);assert(panel.buttons.remote_on.disabled);assert.doesNotMatch(panel.output.textContent,/服务：运行中/);assert.equal(f.sent.length,1);
 if(end==='logout')for(const [name,button] of Object.entries(panel.buttons))assert.equal(button.disabled,true,`Late NKN reply cannot enable ${name} after logout`);
}
{
 const f=fixture({respond:async()=>{throw Error('Corrupt pending must fail before IPC');}}),panel=f.nkn();
 f.saved.set('nkn-control-pending',{format:1,id:nknId,owner:'remote',enabled:true,expected_revision:'00',core_instance:nknContext,secret:'MUST-NOT-LEAK'});
 await panel.buttons.read.onclick();assert.equal(f.sent.length,0);assert.equal(panel.sample,null);assert(panel.buttons.remote_on.disabled);assert.doesNotMatch(panel.state.textContent,/MUST-NOT-LEAK/);assert(f.saved.get('nkn-control-pending'));panel.reset();
}
console.log('PASS: real app/NKN construction, exact pending/archive aliases and IndexedDB readback, passive-read no Remote ON, explicit manual-only request, corrupt pending, logout/pagehide/visibility late-response reset.');

{
 const f=fixture({respond:async c=>{if(c.op==='nkn_status')return nknReport(1);throw Error('Synthetic re-login denial');}}),panel=f.nkn();
 await panel.buttons.read.onclick();assert.equal(panel.buttons.read.disabled,false);
 await panel.loginButton.onclick();
 assert.equal(panel.sample,null);
 for(const [name,button] of Object.entries(panel.buttons))assert.equal(button.disabled,true,`Failed re-login cannot leave NKN ${name} available`);
 assert.equal(f.sent.filter(c=>c.op==='nkn_status').length,1,'Denied re-login cannot implicitly read NKN again');
}
console.log('PASS: NKN controls disabled after app logout, late response and denied re-login; management login remains available.');
