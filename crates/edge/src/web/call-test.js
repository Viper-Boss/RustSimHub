// SPDX-License-Identifier: AGPL-3.0-only
// A user click authorizes one fixed 10010 call. No retry, resume or lease renewal.
export async function archiveEndedOperation({pending,request,archivePending,setPending,current}){
 current();
 if(pending?.type!=='call'||!['dial','hangup'].includes(pending.action)||!pending.submit_started)throw Error('只能归档已提交且已结束的不确定呼叫');
 const result=await request({op:'call_operation',operation_id:pending.id});current();const operation=result.operation;
 if(operation?.id!==pending.id||operation.action!==pending.action||!['unknown','submitting'].includes(operation.state)||!operation.call_id)throw Error('当前操作不符合归档条件；请查询结果');
 const status=await request({op:'call_status'});current();if(status.available!==true||status.active_calls!==0)throw Error('仍有未结束或未确认的呼叫，不能归档');
 const call=await request({op:'call_detail',call_id:operation.call_id});current();
 if(call.id!==operation.call_id||call.state!=='ended'||call.availability!=='observed')throw Error('尚未确认这笔呼叫已结束，不能归档');
 await archivePending({...pending,state:operation.state,ended_call_id:operation.call_id,manual_archive:true});current();
 await setPending(null);current();
}
function matchingLease(result,line){
 const l=result?.lease;
 return l&&l.call_id===result.operation?.call_id&&typeof l.call_id==='string'&&l.call_id.length>0&&l.line===line&&Number.isSafeInteger(l.generation)&&l.generation>0&&/^[a-f0-9]{64}$/.test(l.boot)&&Number.isInteger(l.expires_in_seconds)&&l.expires_in_seconds>0&&l.expires_in_seconds<=30?l:null;
}
export class CallTest {
 constructor(options){Object.assign(this,options);this.env=options.env||globalThis;this.running=false;this.lease=null;}
 say(text){this.notify(text,this.running);}
 check(){if(this.cancelled)throw Error('测试已停止');}
 async step(promise){const value=await Promise.race([promise,this.aborted]);this.check();return value;}
 async pause(ms){await this.step(new Promise(resolve=>{this.waitTimer=this.env.setTimeout(resolve,ms);}));}
 async observedEnded(lease){
  try{const status=await this.request({op:'call_status'});if(status.available!==true)return false;const call=await this.request({op:'call_detail',call_id:lease.call_id});return call.state==='ended'&&call.availability==='observed';}
  catch{return false;}
 }
 async confirmEnded(lease){
  // Read-only reconciliation may lag the single hangup response. Bound the
  // whole observation, including stalled HTTP; never submit another hangup.
  let expired=false,timer;
  const timeout=new Promise(resolve=>{timer=this.env.setTimeout(()=>{expired=true;resolve(false);},3000);});
  const observing=(async()=>{
   for(let attempt=0;attempt<6&&!expired;attempt++){
    try{
     const status=await this.request({op:'call_status'});if(expired)return false;
     if(status.available===true){
      const call=await this.request({op:'call_detail',call_id:lease.call_id});if(expired)return false;
      if(call.state==='ended'&&call.availability==='observed')return true;
     }
    }catch{if(expired)return false;}
    if(attempt<5&&!expired)await new Promise(resolve=>this.env.setTimeout(resolve,250));
   }
   return false;
  })();
  try{return await Promise.race([observing,timeout]);}
  finally{expired=true;this.env.clearTimeout(timer);}
 }
 async start(){
  if(this.running)return;
  const generation=(this.generation||0)+1;this.generation=generation;
  this.running=true;this.cancelled=false;this.finishing=null;this.lease=null;this.stream=null;this.dialStarted=false;this.error=null;this.endedObserved=false;this.callActiveAt=null;this.audioDelayMs=null;this.uncertainDial=null;
  this.aborted=new Promise((_,reject)=>{this.abort=()=>reject(Error('测试已停止'));});this.aborted.catch(()=>{});
  this.say('正在准备测试，请允许麦克风…');
  try{
   // Ask before dialing so a slow permission prompt cannot consume the call.
   const input=this.audio.input().then(stream=>{if(this.cancelled||generation!==this.generation){for(const track of stream.getTracks())track.stop();}else this.stream=stream;return stream;});
   await this.step(input);await this.step(this.login());
   const old=this.getPending();
   if(old){
    const result=await this.step(this.request({op:old.type==='call'?'call_operation':'sms_operation',operation_id:old.id}));
    const state=old.type==='call'?result.operation?.state:result.state;
    if(!['submitted','cancelled'].includes(state)&&!(state==='prepared'&&!old.submit_started)){
     if(old.type==='call'&&old.action==='hangup'&&state==='prepared'){
      const cancelled=await this.step(this.request({op:'cancel_prepared_call',operation_id:old.id}));
      if(cancelled.operation?.state!=='cancelled')throw Error('旧挂断的撤销结果未确认；不会再次拨号');
     }else{
     // An old hangup may stay uncertain across a Core restart. Keep its receipt
     // instead of resubmitting it; only an exact, currently observed ended call
     // permits a NEW user-requested test. Unknown dial/SMS still block.
     const callId=result.operation?.call_id;
     if(old.type!=='call'||old.action!=='hangup'||!callId||!['prepared','submitting','unknown'].includes(state)||typeof this.archivePending!=='function'||!await this.step(this.observedEnded({call_id:callId})))throw Error('上一笔操作结果未确认；请在手动调试中查询，不会再次拨号');
     await this.step(this.archivePending({...old,state,ended_call_id:callId}));
     }
    }
    if(old.prior_operation){
     if(typeof this.archivePending!=='function')throw Error('不确定拨号记录尚未保存，不能开始新测试');
     await this.step(this.archivePending({...old,cleanup_receipt:true}));
    }
    await this.step(this.setPending(null));
   }
   const status=await this.step(this.request({op:'call_status'}));
   if(status.available!==true||status.active_calls!==0)throw Error('设备有未结束或未确认的呼叫，请先结束它');
   const line=await this.step(this.request({op:'line_status'}));
   if(line.status!=='ready')throw Error('SIM 尚未就绪');
   this.say('正在提前连接加密声音，完成后自动拨打 10010…');
   await this.step(this.audio.connect(this.context(null),{stream:this.stream}));
   const prepared=await this.step(this.request({op:'prepare_call',line_epoch:line.line,idempotency_key:this.env.crypto.randomUUID(),action:'dial',call_id:null,payload:'10010'}));
   if(prepared.operation?.state!=='prepared')throw Error('测试拨号未准备成功');
   const pending={type:'call',action:'dial',id:prepared.operation.id,state:'prepared',submit_started:false,test10010:true};
   await this.step(this.setPending({...pending}));
   // Durable marker precedes the only submission, including lost replies/reload.
   pending.submit_started=true;await this.step(this.setPending({...pending}));
   // Keep the 30-second device limit; reserve 3 seconds for one hangup and
   // observer confirmation instead of racing lease expiry at second 30.
   this.dialStarted=true;this.deadline=(this.env.performance?.now()??Date.now())+27000;
   this.deadlineTimer=this.env.setTimeout(()=>{void this.stop();},27000);
   this.say('正在拨打 10010，约 30 秒后自动结束…');
   const result=await this.step(this.request({op:'submit_call',operation_id:pending.id,lease:null,urgent:false}));
   // An uncertain dial may still own a valid lease. Retain only the exact
   // returned call binding for one bounded hangup, never to dial again or open PCM.
   this.lease=matchingLease(result,line.line);if(this.lease)this.onLease(this.lease);
   pending.state=result.operation?.state||'unknown';await this.step(this.setPending({...pending}));
   if(pending.state!=='submitted'||!this.lease){this.uncertainDial={...pending};throw Error('拨号结果未确认；不会重拨，请查询当前操作');}
   while(true){
    // Refresh the trusted observer, then inspect this exact call, not a history page.
    await this.step(this.request({op:'call_status'}));
    const call=await this.step(this.request({op:'call_detail',call_id:this.lease.call_id}));
    if(call.state==='active'&&call.availability==='observed'){this.callActiveAt=this.env.performance?.now()??Date.now();break;}
    if(call.state==='ended')throw Error('10010 已结束，未连接声音');
    await this.step(this.audio.hold());this.say('声音已准备好，等待 10010 接通…');await this.pause(500);
   }
   this.say('已接通，正在自动连接声音…');
   await this.step(this.audio.start(this.context(this.lease),{stream:this.stream}));
   this.audioDelayMs=(this.env.performance?.now()??Date.now())-this.callActiveAt;
   while(true){
    const left=Math.max(0,Math.ceil((this.deadline-(this.env.performance?.now()??Date.now()))/1000));
    this.say(this.audio.state==='active'?`声音已连接 · ${left} 秒后自动结束`:'声音已中断；正在结束本次测试');
    if(this.audio.state!=='active'){
     this.endedObserved=await this.step(this.observedEnded(this.lease));
     if(this.endedObserved)break;
     throw Error('声音提前中断'+(this.audio.lastMessage?'：'+this.audio.lastMessage:''));
    }
    await this.pause(500);
   }
  }catch(error){if(!this.cancelled)this.error=error.message||'测试失败';}
  finally{await this.finish(true);}
 }
 stop(hangup=true){if(!this.running)return Promise.resolve();this.cancelled=true;this.abort();return this.finish(hangup);}
 finish(hangup){
  if(this.finishing)return this.finishing;
  this.finishing=(async()=>{
   this.env.clearTimeout(this.waitTimer);this.env.clearTimeout(this.deadlineTimer);
   const lease=this.lease;this.lease=null;this.onLease(null);
   if(this.stream)for(const track of this.stream.getTracks())track.stop();this.stream=null;
   // Close the microphone immediately; server cleanup must not delay this.
   const closing=this.audio.stop();
   let confirmed=this.endedObserved,alreadyEnded=confirmed;
   if(hangup&&lease){
    if(!confirmed)try{
    this.say('正在挂断并关闭声音…');
    const prepared=await this.request({op:'prepare_call',line_epoch:lease.line,idempotency_key:this.env.crypto.randomUUID(),action:'hangup',call_id:lease.call_id,payload:''});
    if(prepared.operation?.state!=='prepared')throw Error('挂断未准备成功');
    const pending={type:'call',action:'hangup',id:prepared.operation.id,state:'prepared',submit_started:true,test10010:true};
    if(this.uncertainDial)pending.prior_operation=this.uncertainDial;
    await this.setPending({...pending});
    const result=await this.request({op:'submit_call',operation_id:pending.id,lease,urgent:true});
    pending.state=result.operation?.state||'unknown';await this.setPending({...pending});
   }catch{ /* Never retry an uncertain submit. The unrenewed Core lease expires. */ }
    // A rejected prepare or a lost submit reply must not skip read-only proof.
    if(!confirmed)confirmed=await this.confirmEnded(lease);
   }
   await closing;this.running=false;
   const cleanup=this.dialStarted&&!confirmed?'；挂断尚未确认，设备租约到期后清理，请勿立即重拨':'';
   const ended=alreadyEnded?'电话已结束':'电话已挂断';
   this.say((this.error?'测试停止：'+this.error+(confirmed?'；'+ended:''):confirmed?'测试已结束，'+ended:'测试已停止')+cleanup+(Number.isFinite(this.audioDelayMs)?`；观察接通后接音 ${(this.audioDelayMs/1000).toFixed(1)} 秒`:''));
   this.error=null;
  })();return this.finishing;
 }
}
