// SPDX-License-Identifier: AGPL-3.0-only
// Production controller with synthetic IPC, durable writes, microphone and clock.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import vm from 'node:vm';
const bytes=await readFile(new URL('../crates/edge/src/web/phone.js',import.meta.url));
const {DailyPhone}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(match=>match[1]);assert.equal(new Set(ids).size,ids.length);
for(const match of bytes.toString().matchAll(/\$\('([^']+)'\)/g))assert(ids.includes(match[1]),'Production HTML must contain '+match[1]);
const routes=await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url),'utf8');assert(routes.includes('"/phone.js"')&&routes.includes('include_str!("web/phone.js")'));
const uuid=n=>`00000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
const callId=uuid(1),line=2;
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const turn=()=>new Promise(resolve=>setImmediate(resolve));
class Element{
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.checked=false;}
 set innerHTML(_){throw Error('Call data must not be parsed as HTML');}
 append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;}
}
function fixture(options={}){
 const elements=new Map(),sent=[],saved=[],receipts=[],uncertain=[],tracks=[],audioSteps=[];
 const $=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const document={getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}};
 let pending=options.pending||null,lease=null,now=0,next=10,owned=false;
 const records=new Map();let calls=options.incoming?[{id:callId,line,direction:1,state:'ringing_in',modem_state:3,availability:'observed',number:'<img src=x> 合成号码'}]:[];
 const grant=()=>({call_id:callId,line,boot:'a'.repeat(64),generation:1,expires_in_seconds:30});
 const audio={state:'closed',muted:false,
  async input(){audioSteps.push('input');if(options.input)return options.input.promise;const track={stopped:false,stop(){this.stopped=true;}};tracks.push(track);return {getTracks:()=>[track],getAudioTracks:()=>[track]};},
  async prime(){audioSteps.push('prime');this.state='primed';},
  async connect(context){assert.equal(context.lease,null);audioSteps.push('connect');this.state='ready';},
  async hold(){audioSteps.push('hold');},
  async start(context){assert.equal(context.lease.call_id,callId);audioSteps.push('start');this.state='active';},
  async stop(){audioSteps.push('stop');this.state='closed';},mute(value){this.muted=value;},async play(){audioSteps.push('play');},
 };
 async function backend(c){
  switch(c.op){
   case 'line_status':return {status:'ready',line};
   case 'call_status':return {available:true,active_calls:calls.filter(x=>x.modem_state!==7).length,live_calls:calls.map(x=>({...x}))};
   case 'call_detail':return {...calls.find(x=>x.id===c.call_id)};
   case 'prepare_call':{
    const id=uuid(next++),op={id,line,call_id:c.call_id,state:'prepared'};records.set(id,{...op,action:c.action});return {operation:{...op},lease:null};
   }
   case 'submit_call':{
    if(!options.skipMarkerCheck)assert(saved.some(p=>p?.id===c.operation_id&&p.submit_started),'Client marker must be durable before hardware submission');
    const op=records.get(c.operation_id);assert(op);
    if(op.action==='dial'){assert.equal(audio.state,'ready');calls=[{id:callId,line,direction:2,state:'active',modem_state:4,availability:'observed',number:'12025550123'}];op.call_id=callId;owned=true;}
    if(op.action==='accept'){assert.equal(audio.state,'primed');calls[0]={...calls[0],state:'active',modem_state:4};owned=true;}
    if(op.action==='hangup'){calls[0]={...calls[0],state:'ended',modem_state:7};owned=false;}
    op.state=options.unknown&&op.action==='dial'?'unknown':'submitted';
    if(options.submit)return options.submit.promise;
    return {operation:{id:op.id,line,call_id:op.call_id,state:op.state},lease:owned?grant():null};
   }
   case 'call_operation':{
    const op=records.get(c.operation_id);assert(op);return {operation:{id:op.id,line,call_id:op.call_id,state:op.state},lease:owned?grant():null};
   }
   case 'cancel_prepared_call':{
    const op=records.get(c.operation_id);assert.equal(op.state,'prepared');op.state='cancelled';return {operation:{id:op.id,line,call_id:op.call_id,state:'cancelled'},lease:null};
   }
   case 'renew_call':assert(owned);return {lease:{...grant(),...options.renewOverride}};
   default:throw Error('Unexpected synthetic command '+c.op);
  }
 }
 const phone=new DailyPhone({document,env:{crypto:webcrypto,performance:{now:()=>now},setInterval:()=>1,clearInterval(){}},audio,
  audioReady:()=>true,context:value=>({token:'synthetic-session',lease:value}),blocked:()=>options.blocked===true,
  getPending:()=>pending,setPending:async value=>{pending=structuredClone(value);await options.onSave?.(value);saved.push(structuredClone(value));},
  archiveKnown:async value=>receipts.push(structuredClone(value)),archivePending:async value=>uncertain.push(structuredClone(value)),setLease:value=>{lease=value;},
  request:async c=>{sent.push(structuredClone(c));const result=await options.respond?.(c,{backend,calls,records});return result??backend(c);},
 });
 phone.enabled=true;$('phone-number').value='12025550123';phone.render();
 return {phone,document,backend,$,sent,saved,receipts,uncertain,tracks,audio,audioSteps,records,pending:()=>pending,lease:()=>lease,advance:value=>{now+=value;},setCalls:value=>{calls=value;},setOwned:value=>{owned=value;}};
}
{
 const f=fixture();await f.$('phone-dial').onclick();assert.equal(f.sent.filter(x=>x.op==='submit_call').length,1);assert(f.phone.managing);
 assert.equal(f.audio.state,'ready','PCM activation waits for observed ACTIVE');await f.phone.refresh();assert.equal(f.audio.state,'active');
 f.advance(8001);await f.phone.refresh();assert.equal(f.sent.filter(x=>x.op==='renew_call').length,1);
 f.$('phone-tone').value='a';await f.$('phone-tone-send').onclick();assert.equal(f.sent.filter(x=>x.op==='prepare_call'&&x.action==='dtmf')[0].payload,'A');
 f.$('phone-muted').checked=true;f.$('phone-muted').onchange();assert.equal(f.audio.muted,true);
 await f.$('phone-hangup').onclick();assert.equal(f.sent.filter(x=>x.op==='prepare_call'&&x.action==='hangup').length,1);assert.equal(f.lease(),null);assert(f.tracks.every(t=>t.stopped));
 await f.$('phone-close').onclick();assert.equal(f.pending(),null);assert.equal(f.receipts.length,3);
}
for(const action of ['accept','reject']){
 const f=fixture({incoming:true});await f.phone.refresh();assert.equal(f.sent.filter(x=>x.op==='prepare_call').length,0,'Observation never answers');
 const row=f.$('phone-live').children[0];assert.match(row.children[0].textContent,/<img/);
 await row.children[action==='accept'?1:2].onclick();const prepared=f.sent.find(x=>x.op==='prepare_call');assert.equal(prepared.call_id,callId);assert.equal(prepared.action,action==='reject'?'hangup':'accept');
 if(action==='accept'){await f.phone.refresh();assert.equal(f.audio.state,'active');assert.equal(f.audioSteps.filter(x=>x==='prime').length,1);}
 else{assert.equal(f.audioSteps.filter(x=>x==='input').length,0);assert.equal(f.sent.find(x=>x.op==='submit_call').lease,null);assert.equal(f.phone.managing,false);}
}
{
 const input=deferred(),f=fixture({input});const first=f.$('phone-dial').onclick();await turn();await f.$('phone-dial').onclick();assert.equal(f.audioSteps.filter(x=>x==='input').length,1);
 await f.$('phone-hangup').onclick();const track={stopped:false,stop(){this.stopped=true;}};input.resolve({getTracks:()=>[track]});await first;
 assert(track.stopped);assert.equal(f.sent.filter(x=>x.op==='prepare_call'||x.op==='submit_call').length,0);
}
for(const marker of [false,true]){
 const f=fixture({onSave:value=>{if(value?.submit_started===marker)throw Error('synthetic storage failure');}});await f.$('phone-dial').onclick();assert.equal(f.sent.filter(x=>x.op==='submit_call').length,0);assert.match(f.$('phone-state').textContent,/storage/);
}
{
 const f=fixture({unknown:true});await f.$('phone-dial').onclick();const id=f.pending().id;await f.phone.refresh();f.advance(9000);await f.phone.refresh();
 assert.equal(f.audio.state,'closed');assert.equal(f.sent.filter(x=>x.op==='renew_call').length,0);assert(f.$('phone-connect').disabled);
 await f.$('phone-dial').onclick();assert.equal(f.sent.filter(x=>x.op==='submit_call').length,1);assert.equal(f.pending().id,id);
}
{
 const gate=deferred(),f=fixture({submit:gate});const first=f.$('phone-dial').onclick();await turn();assert(f.pending().submit_started);
 await f.$('phone-hangup').onclick();const old=f.pending().id;gate.resolve({operation:{id:old,line,call_id:callId,state:'submitted'},lease:{call_id:callId,line,boot:'a'.repeat(64),generation:1,expires_in_seconds:30}});await first;
 assert.equal(f.phone.managing,false);assert.equal(f.lease(),null);assert.equal(f.audio.state,'closed');assert.equal(f.sent.filter(x=>x.op==='submit_call').length,1);
}
{
 const f=fixture({renewOverride:{boot:'b'.repeat(64)}});await f.$('phone-dial').onclick();await f.phone.refresh();f.advance(8001);await f.phone.refresh();assert.equal(f.lease(),null);assert.equal(f.audio.state,'closed');assert.match(f.$('phone-state').textContent,/租约/);
}
{
 let held=false;const gate=deferred(),f=fixture({respond:async c=>{if(c.op==='call_status'&&!held){held=true;return gate.promise;}}});const stale=f.phone.refresh();await turn();await f.$('phone-dial').onclick();assert(f.phone.managing);
 gate.resolve({available:true,active_calls:0,live_calls:[]});await stale;assert(f.phone.managing);assert.equal(f.lease().call_id,callId);
}
for(const stage of ['prepare_call','submit_call']){
 const gate=deferred(),f=fixture({respond:async(c,{backend})=>c.op===stage?gate.promise:undefined});const first=f.$('phone-dial').onclick();await turn();f.phone.reset();
 gate.resolve(stage==='prepare_call'?{operation:{id:uuid(99),line,call_id:null,state:'prepared'},lease:null}:{operation:{id:uuid(10),line,call_id:callId,state:'submitted'},lease:null});await first;
 assert.equal(f.lease(),null);if(stage==='prepare_call')assert.equal(f.sent.filter(x=>x.op==='submit_call').length,0);assert.equal(f.phone.enabled,false);
}
{
 const f=fixture({respond:async(c,{backend,records})=>{if(c.op==='submit_call'&&records.get(c.operation_id)?.action==='hangup')throw Error('synthetic lost hangup reply');}});
 await f.$('phone-dial').onclick();await f.phone.refresh();await f.$('phone-hangup').onclick();assert.equal(f.lease(),null);assert.equal(f.phone.managing,false);assert.equal(f.audio.state,'closed');
 f.advance(9000);await f.phone.refresh();assert.equal(f.sent.filter(x=>x.op==='renew_call').length,0);assert.equal(f.sent.filter(x=>x.op==='submit_call').length,2);assert(f.pending().submit_started);
}
{
 const f=fixture();await f.$('phone-dial').onclick();f.setCalls([{id:callId,line,direction:2,state:'dialing',modem_state:1,availability:'observed',number:'12025550123'}]);
 f.audio.hold=async()=>{throw Error('synthetic preconnection expired');};await f.phone.refresh();assert.equal(f.audio.state,'closed');
 f.setCalls([{id:callId,line,direction:2,state:'active',modem_state:4,availability:'observed',number:'12025550123'}]);await f.phone.refresh();
 assert.equal(f.audioSteps.filter(x=>x==='start').length,0,'Expired preconnection never automatically reconnects');await f.$('phone-connect').onclick();assert.equal(f.audio.state,'active');
}
{
 const f=fixture({unknown:true});await f.$('phone-dial').onclick();await f.$('phone-hangup').onclick();assert(f.pending().prior_operation);
 await f.$('phone-close').onclick();assert.equal(f.pending(),null);assert.equal(f.uncertain.length,1);assert.equal(f.uncertain[0].prior_operation.state,'unknown');
}
{
 let failed=false;const f=fixture({onSave:value=>{if(value?.submit_started&&!failed){failed=true;throw Error('marker failure');}}});await f.$('phone-dial').onclick();await f.$('phone-close').onclick();
 assert.equal(f.pending(),null);assert.equal(f.sent.filter(x=>x.op==='cancel_prepared_call').length,1);assert.equal(f.sent.filter(x=>x.op==='submit_call').length,0);
}
{
 const f=fixture({skipMarkerCheck:true}),saved=new Map(),sent=[];
 const context={document:f.document,crypto:webcrypto,TextEncoder,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval:()=>1,clearInterval(){},addEventListener(){},indexedDB:{open:()=>({})},DailyPhone,SessionKeeper:class{start(){}reset(){}async ensure(){}},
  AudioLink:class{constructor(){return f.audio;}},ConfigurationIndicator:class{async set(){}},CallTest:class{async stop(){}},NetworkPanel:class{stop(){}start(){}},EventSync:class{reset(){}},SmsManager:class{reset(){}},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body),c=body.command;sent.push(c);if(c.op==='submit_call')assert.equal(saved.get('pending').submit_started,true);return {ok:true,json:async()=>({protocol:2,id:body.id,result:await f.backend(c)})};},
 };
 const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={phone:()=>dailyPhone,loginAs:()=>{token="synthetic-session";audioReady=true;dailyPhone.enabled=true;},setDb:value=>{db=value;},logout:()=>$("logout").onclick()};',context);
 context.fixture.setDb({transaction(){const tx={};tx.objectStore=()=>({get:key=>{const request={};queueMicrotask(()=>{request.result=saved.get(key);request.onsuccess?.();});return request;},put:(value,key)=>{const copy=structuredClone(value);queueMicrotask(()=>{saved.set(key,copy);tx.oncomplete?.();});}});return tx;}});
 context.fixture.loginAs();await f.$('phone-dial').onclick();assert.equal(sent.filter(c=>c.op==='submit_call').length,1);assert(context.fixture.phone().managing);
 await f.$('prepare-sms').onclick();assert.equal(sent.filter(c=>c.op==='prepare_sms').length,0);assert.match(f.$('notice').textContent,/电话区/);
 context.fixture.logout();assert.equal(context.fixture.phone().enabled,false);assert.equal(f.audio.state,'closed');assert(saved.get('pending').submit_started);
}
console.log('PASS: production daily call controls, incoming/reject, preconnection, ACTIVE audio, lease renewal, durable single submits, uncertainty, cancellation/logout/storage/poll races; synthetic only.');
