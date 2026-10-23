// SPDX-License-Identifier: AGPL-3.0-only
// Exercise the shipped catch-up module and app wiring; no network or hardware.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const bytes=await readFile(new URL('../crates/edge/src/web/event-sync.js',import.meta.url));
const {EventSync}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const smsBytes=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager,smsListText,smsDetailText}=await import('data:text/javascript;base64,'+smsBytes.toString('base64'));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const turn=()=>new Promise(r=>setImmediate(r));
const epoch='00000000-0000-4000-8000-000000000001';
function page(after,count=0,db=epoch){const start=BigInt(after?.seq||'0');return {events:Array.from({length:count},(_,i)=>({seq:String(start+BigInt(i)+1n)})),cursor:{db_epoch:db,seq:String(start+BigInt(count))}};}
function fixture(request){const fetched=[],reloaded=[];const sync=new EventSync({request:async c=>{fetched.push(c);return request(c);},reload:{sms:async()=>reloaded.push('sms'),calls:async()=>reloaded.push('calls')}});return {sync,fetched,reloaded};}
{
 const f=fixture(async c=>page(c.after,c.op==='inbox_events'?(c.after?5:32):0));
 const result=await f.sync.run();assert.equal(result.sms.count,37);assert.equal(result.sms.more,false);assert.deepEqual(f.reloaded,['sms']);
 assert.deepEqual(f.fetched.map(c=>c.op),['inbox_events','inbox_events','call_events']);assert.equal(f.sync.cursors.sms.seq,'37');
 f.fetched.length=0;await f.sync.run();assert.equal(f.fetched[0].after.seq,'37');
}
{
 const f=fixture(async c=>page(c.after,c.op==='inbox_events'?32:0));const result=await f.sync.run();
 assert.equal(result.sms.count,256);assert.equal(result.sms.more,true);assert.equal(f.fetched.length,9);assert.deepEqual(f.reloaded,['sms']);
 await f.sync.run();assert.equal(f.sync.cursors.sms.seq,'512');
}
{
 let fail=true;const f=fixture(async c=>{if(c.op==='inbox_events'&&c.after&&fail)throw Error('offline');return page(c.after,c.op==='inbox_events'&&!c.after?32:0);});
 await assert.rejects(f.sync.run(),/offline/);assert.equal(f.sync.cursors.sms,null);assert.deepEqual(f.reloaded,[]);
 fail=false;await f.sync.run();assert.equal(f.fetched[2].after,null);assert.equal(f.sync.cursors.sms.seq,'32');
}
{
 const f=fixture(async c=>page(c.after,c.op==='inbox_events'?1:0));let fail=true;
 f.sync.reload.sms=async()=>{if(fail)throw Error('view failed');};
 await assert.rejects(f.sync.run(),/view failed/);assert.equal(f.sync.cursors.sms,null);
 fail=false;await f.sync.run();assert.equal(f.sync.cursors.sms.seq,'1');assert.equal(f.fetched[1].after,null);
}
{
 let fail=true;const f=fixture(async c=>{if(c.op==='call_events'&&fail)throw Error('calls denied');return page(c.after,c.op==='inbox_events'&&!c.after?1:0);});
 await assert.rejects(f.sync.run(),/calls denied/);assert.equal(f.sync.cursors.sms.seq,'1');assert.equal(f.sync.cursors.calls,null);
 fail=false;await f.sync.run();assert.equal(f.fetched[2].after.seq,'1');assert.deepEqual(f.reloaded,['sms']);
}
{
 const gate=deferred(),f=fixture(()=>gate.promise);const run=f.sync.run();await turn();f.sync.reset();gate.resolve(page(null,1));
 await assert.rejects(run,/取消/);assert.deepEqual(f.reloaded,[]);assert.equal(f.sync.cursors.sms,null);
}
{
 const gate=deferred(),f=fixture(async c=>page(c.after,1));f.sync.reload.sms=()=>gate.promise;
 const run=f.sync.run();await turn();f.sync.reset();gate.resolve();await assert.rejects(run,/取消/);assert.equal(f.sync.cursors.sms,null);
}
{
 const old=deferred(),next=deferred();let n=0;const f=fixture(()=>++n===1?old.promise:next.promise);
 const first=f.sync.run();await assert.rejects(f.sync.run(),/进行/);assert.equal(f.fetched.length,1);
 f.sync.reset();const second=f.sync.run();old.resolve(page(null));await assert.rejects(first,/取消/);
 await assert.rejects(f.sync.run(),/进行/);next.resolve(page(null));await second;
}
for(const malformed of [
 {events:[],cursor:{db_epoch:epoch,seq:'1'}},
 {events:[{seq:'2'},{seq:'1'}],cursor:{db_epoch:epoch,seq:'1'}},
 {events:[{seq:'1'},{seq:'1'}],cursor:{db_epoch:epoch,seq:'1'}},
 {events:[{seq:'1'}],cursor:{db_epoch:epoch,seq:'2'}},
 {events:[{seq:'01'}],cursor:{db_epoch:epoch,seq:'01'}},
 {events:[{seq:'9223372036854775808'}],cursor:{db_epoch:epoch,seq:'9223372036854775808'}},
 {events:Array(33).fill({seq:'1'}),cursor:{db_epoch:epoch,seq:'1'}},
 {events:[null],cursor:{db_epoch:epoch,seq:'1'}},
 {events:[],cursor:{db_epoch:'',seq:'0'}},
 {events:[]},
]){
 const f=fixture(async()=>malformed);await assert.rejects(f.sync.run());assert.equal(f.sync.cursors.sms,null);assert.deepEqual(f.reloaded,[]);
}
{
 const f=fixture(async c=>page(c.after,c.op==='inbox_events'&&!c.after?32:0,c.after?'changed':epoch));
 await assert.rejects(f.sync.run(),/事件库已变化/);assert.equal(f.sync.cursors.sms,null);assert.deepEqual(f.reloaded,[]);
}
{
 // Sequence numbers are decimal strings, including values beyond JS Number precision.
 const f=fixture(async c=>page(c.after,c.op==='inbox_events'?1:0));
 f.sync.cursors.sms={db_epoch:epoch,seq:'9007199254740992'};await f.sync.run();assert.equal(f.sync.cursors.sms.seq,'9007199254740993');
}
{
 const f=fixture(async c=>page(c.after));f.sync.cursors.sms={db_epoch:epoch,seq:'19'};f.sync.reset();await f.sync.run();assert.equal(f.fetched[0].after,null);
}

// Execute the actual app functions and logout chain in a minimal DOM, with only
// unrelated browser/media services stubbed. EventSync remains the shipped class.
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
class Element {
 constructor(tag=''){this.tag=tag;this._value='';this.textContent='';this.children=[];this.disabled=false;}
 get value(){return this._value||(this.tag==='select'?this.children[0]?.value||'':'');}set value(v){this._value=v;}
 replaceChildren(...v){this.children=v;this.textContent='';if(this.tag==='select')this._value='';}append(...v){this.children.push(...v);}addEventListener(){}
}
function browserFixture(respond){
 const elements=new Map(),sent=[],testStarts=[];
 const document={hidden:false,getElementById(id){if(!elements.has(id))elements.set(id,new Element(['history-source','history-entity','call-id'].includes(id)?'select':''));return elements.get(id);},createElement:tag=>new Element(tag),querySelectorAll:()=>[],addEventListener(){}};
 const context={document,crypto:webcrypto,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener(){},
  indexedDB:{open:()=>({})},EventSync,SmsManager,smsListText,smsDetailText,DailyPhone:class{constructor(){this.managing=false;this.working=false;}reset(){}start(){}},
  AudioLink:class {constructor(){this.state='closed';}async stop(){}},
  ConfigurationIndicator:class {async set(){}},
  CallTest:class {async start(mode='outgoing'){testStarts.push(mode);}async stop(){}},NetworkPanel:class {start(){}stop(){}},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body);sent.push(body.command);const result=await respond(body.command);return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={sync,refresh,eventSync,inboxPage,callsPage,historyPage,loadHistory,clearHistory,loginAs:()=>{token="synthetic";},logout:()=>$("logout").onclick()};',context);
 context.fixture.loginAs();return {...context.fixture,elements,sent,testStarts};
}
{
 const f=browserFixture(async c=>{
  if(c.op==='inbox_events'||c.op==='call_events')return page(c.after,c.op==='inbox_events'&&!c.after?32:0);
  if(c.op==='inbox')return {messages:[{id:'synthetic',line:1}],cursor:{db_epoch:epoch,seq:'1'}};
  throw Error('unexpected command');
 });
 await f.sync();assert.equal(f.elements.get('inbox').children.length,1);assert.match(f.elements.get('events').textContent,/32 条短信事件/);
 assert.equal(f.sent.filter(c=>c.op==='inbox').length,1);f.logout();assert.equal(f.eventSync.cursors.sms,null);assert.equal(f.elements.get('inbox').children.length,0);
}
{
 let fail=true;const f=browserFixture(async c=>{
  if(c.op==='inbox_events'||c.op==='call_events')return page(c.after,c.op==='inbox_events'?1:0);
  if(c.op==='inbox'){if(fail)throw Error('offline');return {messages:[],cursor:{db_epoch:epoch,seq:'0'}};}
  throw Error('unexpected command');
 });
 await assert.rejects(f.sync(),/offline/);assert.equal(f.eventSync.cursors.sms,null);assert.match(f.elements.get('events').textContent,/保留原位置/);
 fail=false;await f.sync();assert.equal(f.sent.filter(c=>c.op==='inbox_events')[1].after,null);
}
{
 const gate=deferred(),f=browserFixture(()=>gate.promise);const running=f.sync();await turn();f.logout();gate.resolve(page(null,1));
 await assert.rejects(running);assert.equal(f.elements.get('inbox').children.length,0);assert.equal(f.elements.get('events').textContent,'');assert.equal(f.sent.length,1);
}
{
 const f=browserFixture(async c=>{
  if(c.op==='inbox_events'||c.op==='call_events')return page(c.after);
  if(c.op==='inbox')return {messages:[],cursor:{db_epoch:epoch,seq:'0'}};
  if(c.op==='calls')return {calls:[],cursor:{db_epoch:epoch,seq:'0'}};
  return {};
 });
 await f.sync();assert.ok(f.eventSync.cursors.sms);await f.refresh();assert.equal(f.eventSync.cursors.sms,null);
}
{
 const gate=deferred();let calls=0;const f=browserFixture(async c=>{
  if(c.op==='inbox')return {messages:++calls===1?[{id:'synthetic',line:1}]:[],cursor:{db_epoch:epoch,seq:'1'}};
  if(c.op==='sms_detail')return gate.promise;
  throw Error('unexpected command');
 });
 await f.inboxPage(true);const click=f.elements.get('inbox').children[0].children[1].onclick();await turn();
 await f.inboxPage(true);gate.resolve({text:'stale deleted synthetic'});await click;
 assert.equal(f.elements.get('sms-detail').textContent,'');assert.equal(f.sent.filter(c=>c.op==='notification_read').length,0);
}
const callPage=(ids=[],seq='0')=>({calls:ids.map(id=>({id,state:'active',availability:'observed'})),cursor:{db_epoch:epoch,seq}});
const historyPageResult=(ids=[],seq='0')=>({records:ids.map(id=>({id})),cursor:{db_epoch:epoch,seq}});
function historySelection(f,source='source-a',entity='sms'){f.elements.get('history-source').value=source;f.elements.get('history-entity').value=entity;}
{
 const old=deferred();let n=0;const f=browserFixture(()=>++n===1?old.promise:callPage(['new-call'],'2'));
 const first=f.callsPage(true),rejected=assert.rejects(first,/通话列表已变化/);await f.callsPage(true);old.resolve(callPage(['old-call'],'1'));await rejected;
 assert.equal(f.elements.get('calls').children.length,1);assert.match(f.elements.get('calls').children[0].children[0].textContent,/new-call/);
 assert.equal(f.elements.get('call-id').children.length,1);assert.equal(f.elements.get('call-id').children[0].value,'new-call');
}
for(const [method,op,result] of [['callsPage','calls',callPage],['inboxPage','inbox',(ids=[],seq='0')=>({messages:ids.map(id=>({id,line:1})),cursor:{db_epoch:epoch,seq}})]]){
 const gate=deferred();let n=0;const f=browserFixture(()=>++n===1?result(Array.from({length:16},(_,i)=>String(i)),'16'):gate.promise);
 await f[method](true);const next=f[method]();await assert.rejects(f[method](),/列表正在加载/);assert.equal(f.sent.length,2);
 const rejected=assert.rejects(next,/offline/);gate.reject(Error('offline'));await rejected;
 const retry=f[method]();assert.equal(f.sent[2].after.seq,'16');await assert.rejects(retry,/offline/);
 assert.equal(f.sent.filter(c=>c.op===op).length,3);
}
{
 const detail=deferred();let n=0;const f=browserFixture(async c=>{
  if(c.op==='calls')return callPage(++n===1?['old-call']:[],'1');
  if(c.op==='call_detail')return detail.promise;throw Error('unexpected read marker');
 });
 await f.callsPage(true);const click=f.elements.get('calls').children[0].children[1].onclick();await turn();await f.callsPage(true);
 detail.resolve({number:'stale synthetic'});await click;assert.equal(f.elements.get('call-detail').textContent,'');assert.equal(f.sent.filter(c=>c.op==='notification_read').length,0);
 f.elements.get('call-detail').textContent='synthetic private detail';f.logout();assert.equal(f.elements.get('call-detail').textContent,'');
}
{
 const f=browserFixture(async c=>c.op==='calls'?callPage(['current-call']):c.op==='call_detail'?{number:'synthetic current'}:{});
 await f.callsPage(true);await f.elements.get('calls').children[0].children[1].onclick();
 assert.match(f.elements.get('call-detail').textContent,/synthetic current/);assert.equal(f.sent.filter(c=>c.op==='notification_read').length,1);
 assert.equal(f.sent.at(-1).record_id,'current-call');
}
{
 const pending=deferred(),f=browserFixture(()=>pending.promise);historySelection(f);
 const first=f.historyPage(true),rejected=assert.rejects(first,/选择已变化/);historySelection(f,'source-b');pending.resolve(historyPageResult(['source-a-record'],'1'));await rejected;
 assert.equal(f.elements.get('history-list').children.length,0);
}
for(const fails of [false,true]){
 const old=deferred();const f=browserFixture(async c=>c.op==='history_detail'?old.promise:historyPageResult([c.source+'-record'],'1'));historySelection(f);
 await f.historyPage(true);const click=f.elements.get('history-list').children[0].children[1].onclick();await turn();
 historySelection(f,'source-b','calls');await f.historyPage(true);f.elements.get('history-detail').textContent='new source detail';
 if(fails)old.reject(Error('revoked old source'));else old.resolve({body:'old source secret'});await click;
 assert.equal(f.elements.get('history-detail').textContent,'new source detail');assert.match(f.elements.get('history-list').children[0].children[0].textContent,/source-b-record/);
}
{
 // Source/entity change invalidation happens before the global action guard,
 // even while a detail click is still occupying it.
 const gate=deferred(),f=browserFixture(async c=>c.op==='history_detail'?gate.promise:historyPageResult(['record']));historySelection(f);
 await f.historyPage(true);const click=f.elements.get('history-list').children[0].children[1].onclick();await turn();
 f.elements.get('history-entity').value='calls';await f.elements.get('history-entity').onchange();assert.equal(f.elements.get('history-list').children.length,0);
 gate.resolve({body:'old entity'});await click;assert.equal(f.elements.get('history-detail').textContent,'');
}
{
 const old=deferred();let n=0;const f=browserFixture(async c=>c.op==='history_sources'?(++n===1?old.promise:{sources:[{source:'source-b',entities:['sms']}]}):historyPageResult(['fresh'],'1'));
 const first=f.loadHistory(),rejected=assert.rejects(first,/历史来源已变化/);await f.loadHistory();old.resolve({sources:[{source:'source-a',entities:['calls']}]});await rejected;
 assert.equal(f.elements.get('history-source').value,'source-b');assert.equal(f.elements.get('history-entity').value,'sms');assert.match(f.elements.get('history-list').children[0].children[0].textContent,/fresh/);
}
{
 const gate=deferred();let n=0;const f=browserFixture(async()=>++n===1?historyPageResult(Array.from({length:16},(_,i)=>String(i)),'16'):gate.promise);historySelection(f);
 await f.historyPage(true);const next=f.historyPage();await assert.rejects(f.historyPage(),/列表正在加载/);assert.equal(f.sent.length,2);assert.equal(f.sent[1].after.seq,'16');
 gate.resolve(historyPageResult(['last'],'17'));await next;assert.equal(f.elements.get('history-list').children.length,17);assert.equal(f.elements.get('more-history').disabled,true);
}
{
 // A superseded catch-up reload is a failure, so its event cursor cannot
 // commit before a different, still-pending visible reload has succeeded.
 const old=deferred(),fresh=deferred();let n=0;const f=browserFixture(async c=>{
  if(c.op==='inbox_events')return page(c.after,1);
  if(c.op==='inbox')return ++n===1?old.promise:fresh.promise;
  throw Error('unexpected command');
 });
 const syncing=f.sync(),rejected=assert.rejects(syncing,/短信列表已变化/);await turn();const resetting=f.inboxPage(true);
 old.resolve({messages:[],cursor:{db_epoch:epoch,seq:'1'}});await rejected;assert.equal(f.eventSync.cursors.sms,null);
 fresh.resolve({messages:[],cursor:{db_epoch:epoch,seq:'2'}});await resetting;
}
{
 const old=deferred(),fresh=deferred();let n=0;const f=browserFixture(()=>++n===1?old.promise:fresh.promise);
 const first=f.callsPage(true),rejected=assert.rejects(first,/会话已结束/);f.logout();f.loginAs();const second=f.callsPage(true);
 old.resolve(callPage(['old']));await rejected;await assert.rejects(f.callsPage(),/列表正在加载/);assert.equal(f.sent.length,2);
 fresh.resolve(callPage(['new']));await second;assert.match(f.elements.get('calls').children[0].children[0].textContent,/new/);
}
{
 const current='00000000-0000-4000-8000-000000000001',prior='00000000-0000-4000-8000-000000000002';
 const makeDetail=(id,candidate)=>({summary:{id,line:2,duplicate_candidate:candidate,binary:false},sender:'synthetic sender',timestamp:'time',text:'synthetic comparison text',data:[]});
 const gate=deferred();const f=browserFixture(async c=>{
  if(c.op==='inbox')return {messages:[{id:current,line:2,duplicate_candidate:prior}],cursor:{db_epoch:epoch,seq:'1'}};
  if(c.op==='sms_detail')return c.sms_id===current?makeDetail(current,prior):gate.promise;
  if(c.op==='notification_read')return {};
  throw Error('unexpected command');
 });
 await f.inboxPage(true);assert.match(f.elements.get('inbox').children[0].children[0].textContent,/可能重复/);
 await f.elements.get('inbox').children[0].children[1].onclick();assert.equal(f.elements.get('sms-comparison').hidden,false);
 assert.equal(f.sent.filter(c=>c.op==='sms_detail').length,1,'opening current detail does not fetch candidate automatically');
 const comparison=f.elements.get('sms-compare').onclick();await turn();assert.equal(f.sent.at(-1).sms_id,prior);
 f.logout();gate.resolve(makeDetail(prior,null));await comparison;assert.equal(f.elements.get('sms-comparison').hidden,true);assert.equal(f.elements.get('sms-comparison-current').textContent,'');assert.equal(f.elements.get('sms-comparison-prior').textContent,'');
 assert.equal(f.sent.filter(c=>['delete_sms','restore_sms','deleted_sms_detail'].includes(c.op)).length,0);
}
{
 const id='00000000-0000-4000-8000-000000000001',prior='00000000-0000-4000-8000-000000000002';
 const gate=deferred();let n=0;const f=browserFixture(async c=>{
  if(c.op==='inbox')return {messages:[{id,line:2,duplicate_candidate:prior}],cursor:{db_epoch:epoch,seq:'1'}};
  if(c.op==='sms_detail')return ++n===1?{summary:{id,line:2,duplicate_candidate:prior},sender:'s',timestamp:'t',text:'body',data:[]}:gate.promise;
  return {};
 });
 await f.inboxPage(true);const button=f.elements.get('inbox').children[0].children[1];await button.onclick();assert.equal(f.elements.get('sms-compare').disabled,false);
 const next=button.onclick();await turn();assert.equal(f.elements.get('sms-comparison').hidden,true,'changing selection clears old comparison before network reply');assert.equal(f.elements.get('delete-sms').disabled,true);
 gate.resolve({summary:{id,line:2,duplicate_candidate:null},sender:'s',timestamp:'t',text:'fresh',data:[]});await next;assert.equal(f.elements.get('sms-comparison').hidden,true);
}
console.log('PASS: bounded catch-up and real-app SMS/calls/history pagination, candidate review wiring, stale selection/detail/source isolation, logout, retry and cursor commit races; no network or hardware.');

{
 const f=browserFixture(async()=>({}));await f.elements.get('test-incoming-start').onclick();await f.elements.get('test-start').onclick();assert.deepEqual(f.testStarts,['incoming','outgoing']);assert.equal(f.sent.length,0);
}
{
 const f=browserFixture(async()=>({calls:[{id:'old',state:'reconciling',availability:'reconciling'},{id:'unknown',state:'reconciling',availability:'observed'},{id:'live',state:'ringing_in',availability:'observed'}],cursor:{db_epoch:epoch,seq:'3'}}));await f.callsPage(true);assert.equal(f.elements.get('calls').children.length,3,'history remains visible');assert.deepEqual(f.elements.get('call-id').children.map(x=>x.value),['live'],'only current actionable state is offered');
}
