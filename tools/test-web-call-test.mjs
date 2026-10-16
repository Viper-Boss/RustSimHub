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
 let pending=options.pending||null,loginCount=0;const calls=[],states=[],timers=new Set();
 const track={stopped:false,stop(){this.stopped=true;}};const stream={getTracks:()=>[track]};
 const audio={state:'closed',async input(){if(options.denied)throw Error('permission denied');return options.permission?options.permission.promise:stream;},async start(context,provided){assert.equal(provided.stream,stream);assert.equal(context.lease.call_id,'synthetic-call');this.state='active';},async stop(){this.state='closed';}};
 const env={crypto:webcrypto,performance,clearTimeout(id){clearTimeout(id);timers.delete(id);},setTimeout(fn,ms){const id=setTimeout(()=>{timers.delete(id);fn();},ms===30000?40:1);timers.add(id);return id;}};
 const lease={call_id:'synthetic-call',line:2,boot:'a'.repeat(64),generation:1,expires_in_seconds:30};
 let ended=false;
 const test=new CallTest({audio,env,login:async()=>{loginCount++;},getPending:()=>pending,setPending:async value=>{pending=value;},context:value=>({token:'b'.repeat(64),lease:value}),onLease:()=>{},notify:(text,running)=>states.push({text,running}),request:async command=>{
  calls.push(command);
  switch(command.op){
   case 'call_status':return {available:true,active_calls:ended?0:options.busy?1:0};
   case 'line_status':return {line:2,status:'ready'};
   case 'call_operation':return {operation:{state:options.oldState||'prepared'}};
   case 'prepare_call':assert.equal(command.line_epoch,2);if(command.action==='dial'){assert.equal(command.payload,'10010');assert.equal(command.call_id,null);}else assert.equal(command.call_id,lease.call_id);return {operation:{id:command.action+'-op',state:'prepared'}};
   case 'submit_call':
    assert.equal(pending.id,command.operation_id);assert.equal(pending.submit_started,true);
    if(command.operation_id==='dial-op'){assert.equal(command.lease,null);if(options.lost)throw Error('lost reply');if(options.submit)return options.submit.promise;return {operation:{state:'submitted'},lease};}
    assert.equal(command.urgent,true);ended=true;return {operation:{state:'submitted'}};
   case 'call_detail':assert.equal(command.call_id,lease.call_id);return {state:ended?'ended':'active',availability:'observed'};
   default:throw Error('Unexpected API: '+command.op);
  }
 }});
 return {test,track,stream,audio,calls,states,timers,pending:()=>pending,loginCount:()=>loginCount,close:()=>{for(const timer of timers)clearTimeout(timer);}};
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
