// SPDX-License-Identifier: AGPL-3.0-only
// Production renewal clock and real app/proof/phone wiring; synthetic IPC only.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import vm from 'node:vm';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
async function module(name){const b=await readFile(new URL('../crates/edge/src/web/'+name,import.meta.url));return import('data:text/javascript;base64,'+b.toString('base64'));}
const {SessionKeeper}=await module('session.js'),{DailyPhone}=await module('phone.js'),{prove,hex,unhex}=await module('proof.js');
const token='a'.repeat(64),ok={renewed:true,token:'b'.repeat(64),expires_in_seconds:900,recent_owner_login_refreshed:false};
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const turn=()=>new Promise(resolve=>setImmediate(resolve));
function clock(){let now=0,next=1;const timers=new Map();return {performance:{now:()=>now},setTimeout(fn,ms){const id=next++;timers.set(id,{fn,end:now+ms});return id;},clearTimeout(id){timers.delete(id);},setInterval(){return 0;},clearInterval(){},advance(ms){now+=ms;for(const [id,t] of [...timers])if(t.end<=now){timers.delete(id);t.fn();}},timers};}
{
 const c=clock(),requests=[];let current=token;const k=new SessionKeeper({env:c,token:()=>current,apply:(old,next)=>{assert.equal(old,current);current=next;},renew:async value=>{requests.push(value);c.advance(1000);return {...ok,token:(requests.length+1).toString(16).padStart(64,'0')};}});k.start(token,900);
 await k.ensure();assert.equal(requests.length,0);
 for(let i=0;i<4;i++){c.advance(720000);await k.ensure();assert.equal(k.deadline,c.performance.now()-1000+900000);}
 assert.equal(requests.length,4);assert.equal(c.timers.size,0);assert.equal(k.sessionToken,current);assert.notEqual(current,token);assert.equal(new Set(requests).size,4);
}
{
 const c=clock(),gate=deferred();let requests=0,current=token;const k=new SessionKeeper({env:c,token:()=>current,apply:(_,next)=>{current=next;},renew:async()=>{requests++;return gate.promise;}});k.start(token,900);c.advance(720001);
 const first=k.ensure(),second=k.ensure();assert.equal(requests,1);gate.resolve(ok);await Promise.all([first,second]);assert.equal(requests,1);
}
for(const response of [{...ok,recent_owner_login_refreshed:true},{...ok,expires_in_seconds:1800},{...ok,token},{...ok,token:'x'.repeat(64)},{renewed:false}]){
 const c=clock();let requests=0;const k=new SessionKeeper({env:c,token:()=>token,renew:async()=>{requests++;return response;}});k.start(token,900);c.advance(720001);const original=k.deadline;
 await assert.rejects(k.ensure(),/未获确认/);assert.equal(k.deadline,original);await assert.rejects(k.ensure());assert.equal(requests,1);
}
{
 const c=clock();let requests=0;const k=new SessionKeeper({env:c,token:()=>token,renew:async()=>{requests++;return ok;}});k.start(token,900);c.advance(900000);await assert.rejects(k.ensure(),/过期/);assert.equal(requests,0);
}
for(const stop of ['timeout','reset','replace']){
 const c=clock(),gate=deferred();let current=token;const k=new SessionKeeper({env:c,token:()=>current,renew:()=>gate.promise});k.start(token,900);c.advance(720001);
 const first=k.ensure(),rejected=assert.rejects(first);if(stop==='timeout')c.advance(45000);else if(stop==='reset')k.reset();else current='b'.repeat(64);
 if(stop!=='timeout')gate.resolve(ok);await rejected;if(stop==='timeout')gate.resolve(ok);await turn();assert.equal(c.timers.size,0);
 if(stop==='reset'){k.start(token,900);assert.equal(k.failed,false);assert.equal(k.deadline,c.performance.now()+900000);}
}
class Element{constructor(){this.value='';this.textContent='';this.children=[];}append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;}}
const callId='00000000-0000-4000-8000-000000000001',lease={call_id:callId,line:2,boot:'e'.repeat(64),generation:1,expires_in_seconds:30};
const device=await crypto.subtle.generateKey('Ed25519',false,['sign','verify']),client=await crypto.subtle.generateKey('Ed25519',false,['sign','verify']);
const pin=new Uint8Array(await crypto.subtle.exportKey('raw',device.publicKey)),pub=new Uint8Array(await crypto.subtle.exportKey('raw',client.publicKey));
async function appFixture({denied=false,wait=null,owner=false}={}){
 const c=clock(),elements=new Map(),sent=[];const $=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const document={getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}};
 let transcript;const audio={state:'active',stops:0,starts:0,context:{token,lease},async stop(){this.stops++;this.state='closed';},async start(){this.starts++;},updateAuthentication(old,next){assert.equal(this.context.token,old);this.context={...this.context,token:next};},mute(){},async play(){}};
 const context={document,crypto:webcrypto,TextEncoder,TextDecoder,AbortController,AbortSignal,setTimeout:c.setTimeout,clearTimeout:c.clearTimeout,setInterval:c.setInterval,clearInterval:c.clearInterval,performance:c.performance,addEventListener(){},indexedDB:{open:()=>({})},DailyPhone,SessionKeeper,prove,hex,unhex,
  AudioLink:class{constructor(){return audio;}},ConfigurationIndicator:class{async set(){}},CallTest:class{async stop(){}},NetworkPanel:class{stop(){}start(){}},EventSync:class{reset(){}},SmsManager:class{reset(){}},
  fetch:async(_url,options)=>{
   const body=JSON.parse(options.body),command=body.command;sent.push(command);let result;
   if(command.op==='session_challenge'){
    if(wait)await wait.promise;
    const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(token))));
    transcript=new TextEncoder().encode(JSON.stringify({domain:'RustSimHub.Auth.v1',purpose:'session_renew',instance:'b'.repeat(64),challenge_id:'c'.repeat(64),device_public_key:Array.from(pin),client_public_key:Array.from(pub),kind:owner?'owner':'client',scopes:owner?1023:59,generation:1,session_digest:digest}));
    result={id:'c'.repeat(64),expires_in_seconds:60,transcript:Array.from(transcript),device_signature:Array.from(new Uint8Array(await crypto.subtle.sign('Ed25519',device.privateKey,transcript)))};
   }else if(command.op==='renew_session'){
    assert.equal(command.token,token);assert.equal(command.challenge_id,'c'.repeat(64));assert(await crypto.subtle.verify('Ed25519',client.publicKey,new Uint8Array(command.signature),transcript));
    result=denied?{error:'synthetic revoked'}:ok;
   }else if(command.op==='line_status'){assert.equal(command.token,ok.token);result={line:2,status:'ready'};}
   else if(command.op==='call_status')result={available:true,active_calls:1,live_calls:[{id:callId,line:2,state:'active',direction:1,modem_state:4,availability:'observed'}]};
   else if(command.op==='call_detail')result={id:callId,line:2,state:'active',direction:1,modem_state:4,availability:'observed',number:'synthetic'};
   else if(command.op==='renew_call')result={lease:{...lease}};
   else throw Error('Unexpected operation '+command.op);
   return {ok:true,json:async()=>({protocol:2,id:body.id,result})};
  },
 };
 const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={inject:(t,k,p,l)=>{token=t;key=k;publicKey=p;lease=l;sessionKeeper.start(t,900);dailyPhone.enabled=true;dailyPhone.managing=true;dailyPhone.confirmedControl=true;dailyPhone.lease=l;dailyPhone.lastRenew=0;dailyPhone.audioAttempted=true;},phone:()=>dailyPhone,keeper:()=>sessionKeeper,token:()=>token,lease:()=>lease,logout:()=>$("logout").onclick()};',context);
 $('pin').value=hex(pin);context.fixture.inject(token,client,pub,{...lease});
 return {...context.fixture,c,$,audio,sent};
}
for(const owner of [false,true]){
 const f=await appFixture({owner});const originalContext=f.audio.context;f.c.advance(720001);await f.phone().refresh();
 assert.deepEqual(f.sent.map(c=>c.op),['session_challenge','renew_session','line_status','call_status','renew_call','call_detail']);
 assert.equal(f.token(),ok.token);assert.equal(f.lease().generation,1);assert.equal(f.audio.stops,0);assert.equal(f.audio.starts,0);assert.equal(f.audio.context.lease,originalContext.lease);assert.equal(f.audio.context.token,ok.token);assert.equal(f.keeper().failed,false);
}
{
 const f=await appFixture({denied:true});f.c.advance(720001);await f.phone().refresh();assert.equal(f.lease(),null);assert.equal(f.audio.state,'closed');assert.equal(f.sent.filter(c=>c.op==='renew_call').length,0);
 await assert.rejects(f.keeper().ensure());assert.equal(f.sent.filter(c=>c.op==='renew_session').length,1);
}
for(const cancellation of ['logout','timeout']){
 const wait=deferred(),f=await appFixture({wait});f.c.advance(720001);const refreshing=f.phone().refresh();await turn();
 if(cancellation==='logout')f.logout();else f.c.advance(45000);
 await refreshing;wait.resolve();await turn();await turn();assert.equal(f.sent.filter(c=>c.op==='renew_session'||c.op==='renew_call').length,0);assert.equal(f.audio.state,'closed');
}
const routes=await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url),'utf8');assert(routes.includes('"/session.js"')&&routes.includes('include_str!("web/session.js")'));
console.log('PASS: signed continuation clock, conservative deadline, single flight, timeout/logout/replay guards, and real app/proof/phone wiring without media reconnect or new call submission; synthetic only.');
