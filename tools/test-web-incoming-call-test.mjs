// SPDX-License-Identifier: AGPL-3.0-only
// Production orchestrator with synthetic calls/media; no network or hardware.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const code=await readFile(new URL('../crates/edge/src/web/call-test.js',import.meta.url));
const {CallTest}=await import('data:text/javascript;base64,'+code.toString('base64'));
const id='00000000-0000-4000-8000-000000000001',old='00000000-0000-4000-8000-000000000002',foreign='00000000-0000-4000-8000-000000000003';
const turn=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {resolve,promise};}
const row=(record=id,state='ringing_in',extra={})=>({id:record,line:2,direction:1,state,modem_state:state==='ended'?7:state==='active'?4:3,availability:'observed',...extra});
function fixture(options={}){
 const calls=[],states=[],scheduled=[],timers=new Set();let pending=null,statusReads=0,lineReads=0,accepted=false,ended=false,hangup=false,primed=false,starts=0;
 const track={readyState:'live',enabled:true,stopped:false,stop(){this.stopped=true;this.readyState='ended';}},stream={getTracks:()=>[track],getAudioTracks:()=>[track]};
 const lease={call_id:id,line:2,boot:'a'.repeat(64),generation:1,expires_in_seconds:30};
 const env={crypto:webcrypto,performance,setTimeout(fn,ms){scheduled.push(ms);const t=setTimeout(()=>{timers.delete(t);fn();},ms===120000?70:ms===27000?30:ms===3000?20:1);timers.add(t);return t;},clearTimeout(t){clearTimeout(t);timers.delete(t);}};
 const audio={state:'closed',async input(){if(options.denied)throw Error('microphone denied');return options.input?options.input.promise:stream;},async prime(s){assert.equal(s,stream);assert(!accepted);track.enabled=false;primed=true;this.state='primed';},async connect(){throw Error('incoming wait must not reserve a remote preconnection');},async start(context,{stream:s}){assert.equal(s,stream);assert(accepted&&primed);assert.equal(context.lease.call_id,id);starts++;this.state='active';if(options.peerEnded){ended=true;this.state='closed';}},async stop(){this.state='closed';}};
 const test=new CallTest({audio,env,login:async()=>{},context:value=>({token:'synthetic',lease:value}),onLease:()=>{},getPending:()=>pending,setPending:async value=>{if(options.persistFail&&value?.submit_started)throw Error('storage failed');pending=value;},archivePending:async()=>{},notify:(text,running)=>states.push({text,running}),request:async c=>{
  calls.push(c);
  if(c.op==='call_status'){
   statusReads++;
   if(options.waitReply&&statusReads>1&&!accepted)return options.waitReply.promise;
   if(statusReads===1)return {available:true,active_calls:0,live_calls:options.initial||[row(old,'ended')]};
   if(accepted)return {available:true,active_calls:ended?0:1,live_calls:[row(id,ended?'ended':'active')]};
   if(options.revoked)throw Error('authorization revoked');
   if(options.unavailable)return {available:false,active_calls:0,live_calls:[]};
   if(options.badCatalogue)return {available:true,active_calls:0,live_calls:options.badCatalogue};
   if(options.waitForever)return {available:true,active_calls:0,live_calls:[]};
   const target=options.target||row();return {available:true,active_calls:options.multiple?2:1,live_calls:options.multiple?[target,row(foreign)]:[target]};
  }
  if(c.op==='line_status')return {status:'ready',line:options.lineChanged&&++lineReads>1?3:2};
  if(c.op==='call_detail'){assert.equal(c.call_id,id);return row(id,ended||options.gone?'ended':accepted?'active':'ringing_in');}
  if(c.op==='prepare_call'){
   assert.equal(c.line_epoch,2);assert.equal(c.call_id,id);assert.equal(c.payload,'');assert(['accept','hangup'].includes(c.action),'never dial an incoming test');
   if(c.action==='accept'){assert(primed);return {operation:{id:'accept-op',call_id:options.preparedWrong?foreign:id,state:'prepared'}};}
   hangup=true;return {operation:{id:'hangup-op',call_id:id,state:'prepared'}};
  }
  if(c.op==='submit_call'){
   assert.equal(pending.id,c.operation_id);assert.equal(pending.submit_started,true);assert.equal(pending.test_incoming,true);assert.notEqual(pending.test10010,true);
   if(c.operation_id==='accept-op'){assert.equal(c.lease,null);assert.equal(c.urgent,false);accepted=true;if(options.lostAccept)throw Error('accept reply lost');return {operation:{id:'accept-op',call_id:options.wrongLease?foreign:id,state:options.unknown?'unknown':'submitted'},lease:options.noLease?null:{...lease,call_id:options.wrongLease?foreign:id}};}
   assert(hangup);assert.equal(c.lease.call_id,id);assert.equal(c.urgent,true);ended=true;return {operation:{id:'hangup-op',call_id:id,state:'submitted'}};
  }
  if(c.op==='call_operation')return {operation:{id:'accept-op',call_id:id,state:'unknown'}};
  throw Error('unexpected command '+c.op);
 }});
 return {test,audio,track,calls,states,scheduled,starts:()=>starts,pending:()=>pending,close:()=>{for(const t of timers)clearTimeout(t);}};
}
{
 const f=fixture();await f.test.start('incoming');assert.equal(f.starts(),1);assert.match(f.states.at(-1).text,/测试已结束，电话已挂断/);
 assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='accept-op').length,1);assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='hangup-op').length,1);
 assert(f.scheduled.includes(120000)&&f.scheduled.includes(27000));assert(!f.calls.some(c=>c.op==='renew_call'||c.action==='dial'));assert(f.track.stopped);assert.equal(f.audio.state,'closed');f.close();
}
for(const options of [{denied:true},{waitForever:true},{unavailable:true},{revoked:true},{lineChanged:true},{multiple:true},{gone:true},{target:row(old)},{target:row(id,'dialing',{direction:2,modem_state:1})},{target:row(id,'ringing_in',{availability:'reconciling'})},{badCatalogue:[row(),row()]},{badCatalogue:Array(65).fill(row())},{preparedWrong:true},{persistFail:true}]){
 const f=fixture(options);await f.test.start('incoming');assert.equal(f.calls.filter(c=>c.op==='submit_call').length,0,JSON.stringify(options));assert.equal(f.starts(),0);if(!options.denied)assert(f.track.stopped);f.close();
}
for(const options of [{lostAccept:true},{unknown:true},{wrongLease:true},{noLease:true}]){
 const f=fixture(options);await f.test.start('incoming');assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='accept-op').length,1);assert.equal(f.starts(),0);
 if(options.unknown){assert.equal(f.calls.filter(c=>c.action==='hangup').length,1);assert.equal(f.pending().prior_operation.action,'accept');}
 else{assert(!f.calls.some(c=>c.action==='hangup'),'no lease, no authority to hang up a guessed call');assert.equal(f.pending().action,'accept');assert.equal(f.pending().submit_started,true);}
 assert(f.track.stopped);if(options.lostAccept){await f.test.start('incoming');assert.equal(f.calls.filter(c=>c.op==='submit_call'&&c.operation_id==='accept-op').length,1,'unknown accept blocks another automatic accept');}f.close();
}
{
 const f=fixture({peerEnded:true});await f.test.start('incoming');assert.match(f.states.at(-1).text,/电话已结束/);assert(!f.calls.some(c=>c.action==='hangup'));assert(f.track.stopped);f.close();
}
{
 const gate=deferred(),f=fixture({waitReply:gate});const running=f.test.start('incoming');await turn();await f.test.start();await f.test.stop();await running;const count=f.calls.length;gate.resolve({available:true,active_calls:1,live_calls:[row()]});await turn();assert.equal(f.calls.length,count);assert(!f.calls.some(c=>c.op==='prepare_call'));assert(f.track.stopped);f.close();
}
{
 const gate=deferred(),f=fixture({input:gate});const running=f.test.start('incoming');await turn();await f.test.stop();await running;gate.resolve({getTracks:()=>[f.track]});await turn();assert(f.track.stopped);assert.equal(f.calls.length,0);f.close();
}
console.log('Incoming one-click: fresh live catalogue, no history selection/dial/renewal, muted local wait, exact one accept/cleanup, timeout, cancellation, line/owner boundaries and lost replies passed');
