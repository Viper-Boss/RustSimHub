// SPDX-License-Identifier: AGPL-3.0-only
// Exercise the shipped catch-up module and app wiring; no network or hardware.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const bytes=await readFile(new URL('../crates/edge/src/web/event-sync.js',import.meta.url));
const {EventSync}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const smsBytes=await readFile(new URL('../crates/edge/src/web/sms-management.js',import.meta.url));
const {SmsManager}=await import('data:text/javascript;base64,'+smsBytes.toString('base64'));
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};};
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
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;}
 replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}addEventListener(){}
}
function browserFixture(respond){
 const elements=new Map(),sent=[];
 const document={hidden:false,getElementById(id){if(!elements.has(id))elements.set(id,new Element());return elements.get(id);},createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}};
 const context={document,crypto:webcrypto,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener(){},
  indexedDB:{open:()=>({})},EventSync,SmsManager,
  AudioLink:class {constructor(){this.state='closed';}async stop(){}},
  ConfigurationIndicator:class {async set(){}},
  CallTest:class {async stop(){}},NetworkPanel:class {start(){}stop(){}},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body);sent.push(body.command);const result=await respond(body.command);return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={sync,refresh,eventSync,inboxPage,loginAs:()=>{token="synthetic";},logout:()=>$("logout").onclick()};',context);
 context.fixture.loginAs();return {...context.fixture,elements,sent};
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
console.log('PASS: bounded catch-up, retry/cursor isolation, real app refresh/logout wiring, and late deleted-detail suppression; no network or hardware.');
