// SPDX-License-Identifier: AGPL-3.0-only
// Execute the production app and formatter, with synthetic IPC/IndexedDB only.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const bytes=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager,smsListText,smsDetailText}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const id='00000000-0000-4000-8000-000000000001',other='00000000-0000-4000-8000-000000000002';
const turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;}
 set innerHTML(_){throw Error('Message data must never be parsed as HTML');}
 append(...values){this.children.push(...values);}replaceChildren(...values){this.children=values;this.textContent='';}addEventListener(){}
}
function fixture({respond,onPut,consent=true}={}){
 const elements=new Map(),sent=[],saved=new Map(),confirmations=[];
 const $=name=>{if(!elements.has(name))elements.set(name,new Element());return elements.get(name);};
 const document={hidden:false,getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}};
 let state='submitted';
 const context={document,crypto:webcrypto,TextEncoder,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener(){},
  indexedDB:{open:()=>({})},SmsManager,smsListText,smsDetailText,DailyPhone:class{constructor(){this.managing=false;this.working=false;}reset(){}start(){}},
  AudioLink:class{constructor(){this.state='closed';}async stop(){}},
  ConfigurationIndicator:class{async set(){}},CallTest:class{async stop(){}},NetworkPanel:class{start(){}stop(){}},EventSync:class{reset(){}},
  confirm:text=>{confirmations.push(text);return consent;},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body),c=body.command;sent.push(c);const result=respond?await respond(c):c.op==='line_status'?{status:'ready',line:7}:c.op==='prepare_sms'?{id,state:'prepared'}:{id,state};return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={dailySendSms,submit,query,closeSms,inboxPage,act,showPending,loginAs:()=>{token="synthetic-session";},setDb:value=>{db=value;},setPending:value=>{pending=value;showPending();},pending:()=>pending,logout:()=>$("logout").onclick()};',context);
 context.fixture.setDb({transaction(){
  const tx={};tx.objectStore=()=>({get:key=>{const request={};queueMicrotask(()=>{request.result=saved.get(key);request.onsuccess?.();});return request;},put:(value,key)=>{
   const snapshot=structuredClone(value);const completion=onPut?.(key,snapshot);queueMicrotask(async()=>{try{await completion;saved.set(key,snapshot);tx.oncomplete?.();}catch{tx.onerror?.();}});
  }});return tx;
 }});
 context.fixture.loginAs();$('recipient').value='12025550123';$('text').value='合成测试 <img src=x> 🙂';
 return {...context.fixture,$,sent,saved,confirmations,setState:value=>{state=value;}};
}
{
 const f=fixture();await f.$('send-sms').onclick();
 assert.deepEqual(f.sent.map(c=>c.op),['line_status','prepare_sms','submit_sms','sms_operation']);
 assert.equal(f.confirmations.length,1);assert.match(f.confirmations[0],/12025550123/);
 assert.equal(f.saved.get('pending').submit_started,true);assert.equal(f.pending().state,'submitted');
 assert.match(f.$('sms-send-state').textContent,/已提交/);assert.equal(f.$('send-sms').disabled,true);
 await f.$('sms-close').onclick();assert.equal(f.pending(),null);assert.equal(f.$('text').value,'');assert.equal(f.$('recipient').value,'');assert.equal(f.saved.get('sms-operation-receipts')[0].id,id);
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,1);
}
{
 const gate=deferred();const f=fixture({respond:async c=>c.op==='line_status'?gate.promise:c.op==='prepare_sms'?{id,state:'prepared'}:{id,state:'submitted'}});
 const first=f.$('send-sms').onclick();await turn();await f.$('send-sms').onclick();assert.equal(f.confirmations.length,1);
 gate.resolve({status:'ready',line:7});await first;assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,1);
}
{
 const f=fixture({consent:false});await f.dailySendSms();assert.equal(f.sent.length,0);assert.equal(f.pending(),null);
}
for(const text of ['', '\0', '🙂'.repeat(1025)]){
 const f=fixture();f.$('text').value=text;await assert.rejects(f.dailySendSms(),/字节/);assert.equal(f.sent.length,0);assert.equal(f.confirmations.length,0);
}
{
 const f=fixture({respond:async c=>c.op==='line_status'?{status:'ready',line:7}:{id:'-'.repeat(36),state:'prepared'}});
 await assert.rejects(f.dailySendSms(),/准备结果/);assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0);assert.equal(f.pending(),null);
}
for(const stage of ['draft','marker']){
 const f=fixture({onPut:(key,value)=>{if(key==='pending'&&value.submit_started===(stage==='marker'))throw Error('synthetic storage failure');}});
 await assert.rejects(f.dailySendSms(),/storage/);assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0);
}
{
 const f=fixture({respond:async c=>{if(c.op==='line_status')return {status:'ready',line:7};if(c.op==='prepare_sms')return {id,state:'prepared'};throw Error('synthetic submit timeout');}});
 await assert.rejects(f.dailySendSms(),/timeout/);assert.equal(f.pending().submit_started,true);
 await assert.rejects(f.dailySendSms(),/当前操作/);await assert.rejects(f.closeSms(),/尚未确认/);
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,1);assert.equal(f.saved.get('pending').submit_started,true);
}
{
 const f=fixture({respond:async c=>c.op==='line_status'?{status:'ready',line:7}:c.op==='prepare_sms'?{id,state:'prepared'}:c.op==='submit_sms'?{id,state:'unknown'}:{id:other,state:'submitted'}});
 await assert.rejects(f.dailySendSms(),/响应不匹配/);assert.notEqual(f.pending().state,'submitted');assert.equal(f.pending().id,id);
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,1);
}
for(const stage of ['line_status','prepare_sms']){
 const gate=deferred();const f=fixture({respond:async c=>c.op===stage?gate.promise:c.op==='line_status'?{status:'ready',line:7}:{id,state:'prepared'}});
 const sending=f.dailySendSms(),rejected=assert.rejects(sending);await turn();f.logout();gate.resolve(stage==='line_status'?{status:'ready',line:7}:{id,state:'prepared'});await rejected;
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0);assert.equal(f.pending(),null);
}
for(const marker of [false,true]){
 const gate=deferred();const f=fixture({onPut:(key,value)=>key==='pending'&&value.submit_started===marker?gate.promise:undefined});
 const sending=f.dailySendSms(),rejected=assert.rejects(sending);await turn();f.logout();gate.resolve();await rejected;
 assert.equal(f.sent.filter(c=>c.op==='submit_sms').length,0);
}
{
 const gate=deferred();const f=fixture({respond:()=>gate.promise});const sending=f.dailySendSms(),rejected=assert.rejects(sending,/草稿/);f.$('text').value='合成新草稿';gate.resolve({status:'ready',line:7});await rejected;
 assert.equal(f.sent.filter(c=>c.op==='prepare_sms').length,0);
}
{
 const f=fixture();f.setPending({type:'call',id:other,state:'unknown',submit_started:true});await assert.rejects(f.dailySendSms(),/当前操作/);await assert.rejects(f.closeSms());assert.equal(f.pending().id,other);assert.equal(f.sent.length,0);
}
{
 const f=fixture();f.setPending({type:'sms',action:'send',id,state:'prepared',submit_started:false});await f.closeSms();assert.equal(f.pending(),null);assert.equal(f.sent.length,0);assert.equal(f.confirmations.length,1);assert.notEqual(f.$('text').value,'');assert.equal(f.saved.get('sms-operation-receipts')[0].local_discarded,true);
}
{
 const hostile='<img src=x onerror=alert(1)>',detail={summary:{id,line:1},sender:hostile,timestamp:'合成时间',text:hostile+'\n🙂',data:[]};
 assert.match(smsListText({display:{sender:hostile,timestamp:'time',preview:hostile}}),/<img/);assert.match(smsDetailText(detail),/<img/);
 for(const invalid of [{...detail,sender:'x'.repeat(257)},{...detail,text:'x'.repeat(4097)},{...detail,data:[256]},{...detail,data:[-1]},{...detail,data:[1.5]}])assert.throws(()=>smsDetailText(invalid),/详情/);
 const f=fixture({respond:async c=>c.op==='inbox'?{messages:[{id,line:1,display:{sender:hostile,timestamp:'合成时间',preview:hostile}}],cursor:{db_epoch:other,seq:'1'}}:c.op==='sms_detail'?detail:{ok:true}});
 await f.inboxPage(true);assert.match(f.$('inbox').children[0].children[0].textContent,/<img/);assert.equal(f.sent.filter(c=>c.op==='notification_read').length,0,'List previews do not mark all messages read');
 await f.$('inbox').children[0].children[1].onclick();assert.match(f.$('sms-detail').textContent,/<img/);assert.equal(f.sent.filter(c=>c.op==='notification_read').length,1);
}
console.log('PASS: production daily SMS, single confirmation and submit, durable markers, unknown receipts, storage/logout/draft races, safe content and list previews; synthetic inputs only.');
