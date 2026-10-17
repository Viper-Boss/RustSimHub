// SPDX-License-Identifier: AGPL-3.0-only
// No modem or network. Exercise real orchestration and durable submit boundaries.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const bytes=await readFile(new URL('../crates/edge/src/web/call-test.js',import.meta.url));
const {CallTest}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const turn=()=>new Promise(resolve=>setImmediate(resolve));
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};};
function fixture(options={}){
 let pending=options.pending||null,loginCount=0,ending=false,endReads=0;const calls=[],states=[],timers=new Set(),archived=[],scheduled=[];
 const track={stopped:false,stop(){this.stopped=true;}};const stream={getTracks:()=>[track]};
 let primed=false;
 const audio={state:'closed',async input(){if(options.denied)throw Error('permission denied');return options.permission?options.permission.promise:stream;},async connect(context,provided){assert.equal(context.lease,null);assert.equal(provided.stream,stream);assert(!calls.some(c=>c.op==='prepare_call'));if(options.primeFailure)throw Error('remote DTLS failed');if(options.connect)await options.connect.promise;primed=true;this.state='ready';},async hold(){assert(primed);},async start(context,provided){assert.equal(provided.stream,stream);assert.equal(context.lease.call_id,'synthetic-call');assert(primed);this.state=options.mediaClosed?'closed':'active';if(options.peerEnded)ended=true;},async stop(){this.state='closed';}};
 const env={crypto:webcrypto,performance,clearTimeout(id){clearTimeout(id);timers.delete(id);},setTimeout(fn,ms){scheduled.push(ms);const id=setTimeout(()=>{timers.delete(id);fn();},ms===27000?40:ms===3000?30:1);timers.add(id);return id;}};
 const lease={call_id:'synthetic-call',line:2,boot:'a'.repeat(64),generation:1,expires_in_seconds:30};
 let ended=false;
 const test=new CallTest({audio,env,login:async()=>{loginCount++;},getPending:()=>pending,setPending:async value=>{pending=value;},archivePending:async value=>{if(options.archiveFailure)throw Error('receipt storage failed');archived.push(value);},context:value=>({token:'b'.repeat(64),lease:value}),onLease:()=>{},notify:(text,running)=>states.push({text,running}),request:async command=>{
  calls.push(command);
  switch(command.op){
   case 'call_status':if(ending&&options.hangupReadNever)return options.hangupReadNever.promise;return {available:true,active_calls:ended?0:options.busy?1:0};
   case 'line_status':return {line:2,status:'ready'};
   case 'call_operation':return {operation:{state:options.oldState||'prepared',call_id:options.oldCallId||null}};
   case 'cancel_prepared_call':assert.equal(command.operation_id,'old-op');if(options.cancelLost)throw Error('lost cancellation reply');return {operation:{state:options.cancelBusy?'submitting':'cancelled'}};
   case 'prepare_call':assert.equal(command.line_epoch,2);if(command.action==='dial'){assert(primed);assert.equal(command.payload,'10010');assert.equal(command.call_id,null);}else{assert.equal(command.call_id,lease.call_id);if(options.endOnHangupPrepare){ended=true;throw Error('call already ended');}}return {operation:{id:command.action+'-op',state:'prepared'}};
   case 'submit_call':
    assert.equal(pending.id,command.operation_id);assert.equal(pending.submit_started,true);
    if(command.operation_id==='dial-op'){assert.equal(command.lease,null);if(options.lost)throw Error('lost reply');if(options.submit)return options.submit.promise;return {operation:{state:'submitted'},lease};}
    assert.equal(command.urgent,true);ending=true;ended=!options.delayedHangup&&!options.neverEnded;if(options.lostHangup)throw Error('lost hangup reply');return {operation:{state:'submitted'}};
   case 'call_detail':if(command.call_id===options.oldCallId)return{state:options.oldEnded?'ended':'active',availability:options.oldUnavailable?'reconciling':'observed'};assert.equal(command.call_id,lease.call_id);if(ending&&options.delayedHangup&&++endReads>=3)ended=true;return {state:ended?'ended':'active',availability:'observed'};
   default:throw Error('Unexpected API: '+command.op);
  }
 }});
 return {test,track,stream,audio,calls,states,timers,archived,scheduled,pending:()=>pending,loginCount:()=>loginCount,close:()=>{for(const timer of timers)clearTimeout(timer);}};
}
{
 const f=fixture({delayedHangup:true,lostHangup:true});await f.test.start();
 assert.match(f.states.at(-1).text,/电话已挂断/);assert(!/尚未确认/.test(f.states.at(-1).text));
 assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='hangup-op').length,1);
 assert(f.scheduled.includes(27000));assert(!f.scheduled.includes(30000));assert(f.track.stopped);f.close();
}
{
 const f=fixture({neverEnded:true});await f.test.start();assert.match(f.states.at(-1).text,/挂断尚未确认/);
 assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='hangup-op').length,1);assert(f.track.stopped);f.close();
}
{
 const read=deferred(),f=fixture({hangupReadNever:read});await f.test.start();assert.match(f.states.at(-1).text,/挂断尚未确认/);
 const count=f.calls.length;read.resolve({available:true,active_calls:0});await turn();assert.equal(f.calls.length,count);assert(f.track.stopped);f.close();
}
for(const state of ['unknown','submitting']){
 const f=fixture({pending:{type:'call',action:'hangup',id:'old-op',state:'prepared',submit_started:true},oldState:state,oldCallId:'ended-old-call',oldEnded:true});
 await f.test.start();assert.equal(f.archived.length,1);assert.equal(f.archived[0].state,state);assert.equal(f.archived[0].submit_started,true);assert.equal(f.archived[0].ended_call_id,'ended-old-call');assert(!f.calls.some(c=>c.op==='submit_call'&&c.operation_id==='old-op'));assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='dial-op').length,1);f.close();
}
{
 const connect=deferred(),f=fixture({connect});const running=f.test.start();await turn();assert(!f.calls.some(c=>c.op==='prepare_call'));await f.test.stop();await running;connect.resolve();await turn();assert(f.track.stopped);assert(!f.calls.some(c=>c.op==='prepare_call'||c.op==='submit_call'));f.close();
}
{
 const f=fixture({pending:{type:'call',action:'hangup',id:'old-op',state:'prepared',submit_started:true},oldState:'prepared'});await f.test.start();assert.equal(f.archived.length,0);assert.equal(f.calls.filter(c=>c.op==='cancel_prepared_call').length,1);assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='dial-op').length,1);assert(!f.calls.some(c=>c.op==='submit_call'&&c.operation_id==='old-op'));f.close();
}
for(const options of [{cancelLost:true},{cancelBusy:true}]){
 const f=fixture({pending:{type:'call',action:'hangup',id:'old-op',submit_started:true},oldState:'prepared',...options});await f.test.start();assert.equal(f.calls.filter(c=>c.op==='submit_call').length,0);assert.equal(f.pending().id,'old-op');f.close();
}
for(const overrides of [{oldEnded:false},{oldUnavailable:true},{oldCallId:null},{archiveFailure:true},{pending:{type:'call',action:'dial',id:'old-op',submit_started:true}},{pending:{type:'sms',action:'send',id:'old-op',submit_started:true}}]){
 const f=fixture({pending:{type:'call',action:'hangup',id:'old-op',submit_started:true},oldState:'unknown',oldCallId:'ended-old-call',oldEnded:true,...overrides});await f.test.start();assert.equal(f.calls.filter(c=>c.op==='submit_call').length,0);assert.equal(f.pending().id,'old-op');assert(f.track.stopped);f.close();
}
{
 const f=fixture({primeFailure:true});await f.test.start();assert.equal(f.calls.filter(c=>c.op==='prepare_call'||c.op==='submit_call').length,0);assert(f.track.stopped);f.close();
}
{
 const f=fixture({mediaClosed:true,peerEnded:true});await f.test.start();assert.match(f.states.at(-1).text,/电话已结束/);assert(!/声音提前中断|尚未确认/.test(f.states.at(-1).text));assert.equal(f.calls.filter(c=>c.op==='prepare_call'&&c.action==='hangup').length,0);f.close();
}
{
 const f=fixture({mediaClosed:true});await f.test.start();assert.match(f.states.at(-1).text,/声音提前中断/);assert.match(f.states.at(-1).text,/电话已挂断/);f.close();
}
{
 const f=fixture({endOnHangupPrepare:true});await f.test.start();assert(!/尚未确认/.test(f.states.at(-1).text));assert.equal(f.calls.filter(c=>c.op==='submit_call').length,1);assert(f.track.stopped);f.close();
}
{
 const f=fixture({lostHangup:true});await f.test.start();assert(!/尚未确认/.test(f.states.at(-1).text));assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='hangup-op').length,1);assert(f.pending().submit_started);assert(f.track.stopped);f.close();
}
{
 const f=fixture();const running=f.test.start();await f.test.start();await running;
 assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='dial-op').length,1);
 assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='hangup-op').length,1);
 assert.equal(f.calls.filter(c=>c.op==='renew_call').length,0);assert(f.track.stopped);assert.equal(f.loginCount(),1);
 assert.match(f.states.at(-1).text,/电话已挂断/);assert.equal(f.test.running,false);f.close();
}
{
 const f=fixture({denied:true});await f.test.start();assert.equal(f.calls.length,0);assert.equal(f.loginCount(),0);f.close();
}
{
 const f=fixture({busy:true});await f.test.start();assert.equal(f.calls.filter(c=>c.op==='prepare_call').length,0);assert(f.track.stopped);f.close();
}
{
 const f=fixture({lost:true});await f.test.start();assert.equal(f.pending().submit_started,true);assert(f.track.stopped);
 await f.test.start();assert.equal(f.calls.filter(c=>c.op==='submit_call').length,1);assert.match(f.states.at(-1).text,/结果未确认/);f.close();
}
{
 const permission=deferred(),f=fixture({permission});const running=f.test.start();await turn();await f.test.stop();await running;
 permission.resolve(f.stream);await turn();assert(f.track.stopped);assert.equal(f.calls.length,0);f.close();
}
{
 const submit=deferred(),f=fixture({submit});const running=f.test.start();await turn();assert(f.calls.some(c=>c.op==='submit_call'));
 await f.test.stop();await running;submit.resolve({operation:{state:'submitted'},lease:{call_id:'late'}});await turn();
 assert.equal(f.calls.filter(c=>c.op==='submit_call').length,1);assert.equal(f.audio.state,'closed');assert(f.track.stopped);assert(f.pending().submit_started);f.close();
}
{
 const f=fixture({pending:{type:'call',action:'dial',id:'old-op',state:'submitted',submit_started:true},oldState:'submitted'});
 await f.test.start();assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='dial-op').length,1);f.close();
}
console.log('One-click call test passed: fixed target, single submit, durable unknown, no renewal, deadline hangup, permission cancellation and late responses');
