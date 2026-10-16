// SPDX-License-Identifier: AGPL-3.0-only
// A user click authorizes one fixed 10010 call. No retry, resume or lease renewal.
export class CallTest {
 constructor(options){Object.assign(this,options);this.env=options.env||globalThis;this.running=false;this.lease=null;}
 say(text){this.notify(text,this.running);}
 check(){if(this.cancelled)throw Error('测试已停止');}
 async step(promise){const value=await Promise.race([promise,this.aborted]);this.check();return value;}
 async pause(ms){await this.step(new Promise(resolve=>{this.waitTimer=this.env.setTimeout(resolve,ms);}));}
 async start(){
  if(this.running)return;
  const generation=(this.generation||0)+1;this.generation=generation;
  this.running=true;this.cancelled=false;this.finishing=null;this.lease=null;this.stream=null;this.dialStarted=false;this.error=null;
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
    if(!['submitted','cancelled'].includes(state)&&!(state==='prepared'&&!old.submit_started))throw Error('上一笔操作结果未确认；请在手动调试中查询，不会再次拨号');
    await this.step(this.setPending(null));
   }
   const status=await this.step(this.request({op:'call_status'}));
   if(status.available!==true||status.active_calls!==0)throw Error('设备有未结束或未确认的呼叫，请先结束它');
   const line=await this.step(this.request({op:'line_status'}));
   if(line.status!=='ready')throw Error('SIM 尚未就绪');
   const prepared=await this.step(this.request({op:'prepare_call',line_epoch:line.line,idempotency_key:this.env.crypto.randomUUID(),action:'dial',call_id:null,payload:'10010'}));
   if(prepared.operation?.state!=='prepared')throw Error('测试拨号未准备成功');
   const pending={type:'call',action:'dial',id:prepared.operation.id,state:'prepared',submit_started:false,test10010:true};
   await this.step(this.setPending({...pending}));
   // Durable marker precedes the only submission, including lost replies/reload.
   pending.submit_started=true;await this.step(this.setPending({...pending}));
   this.dialStarted=true;this.deadline=(this.env.performance?.now()??Date.now())+30000;
   this.deadlineTimer=this.env.setTimeout(()=>{void this.stop();},30000);
   this.say('正在拨打 10010，约 30 秒后自动结束…');
   const result=await this.step(this.request({op:'submit_call',operation_id:pending.id,lease:null,urgent:false}));
   pending.state=result.operation?.state||'unknown';await this.step(this.setPending({...pending}));
   if(pending.state!=='submitted'||!result.lease)throw Error('拨号结果未确认；不会重拨，请查询当前操作');
   this.lease=result.lease;this.onLease(this.lease);
   while(true){
    // Refresh the trusted observer, then inspect this exact call, not a history page.
    await this.step(this.request({op:'call_status'}));
    const call=await this.step(this.request({op:'call_detail',call_id:this.lease.call_id}));
    if(call.state==='active'&&call.availability==='observed')break;
    if(call.state==='ended')throw Error('10010 已结束，未连接声音');
    this.say('等待 10010 接通…');await this.pause(500);
   }
   this.say('已接通，正在自动连接声音…');
   await this.step(this.audio.start(this.context(this.lease),{stream:this.stream}));
   while(true){
    const left=Math.max(0,Math.ceil((this.deadline-(this.env.performance?.now()??Date.now()))/1000));
    this.say(this.audio.state==='active'?`声音已连接 · ${left} 秒后自动结束`:'声音已中断；正在结束本次测试');
    if(this.audio.state!=='active')throw Error('声音提前中断'+(this.audio.lastMessage?'：'+this.audio.lastMessage:''));
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
   let confirmed=false;
   if(hangup&&lease){try{
    this.say('正在挂断并关闭声音…');
    const prepared=await this.request({op:'prepare_call',line_epoch:lease.line,idempotency_key:this.env.crypto.randomUUID(),action:'hangup',call_id:lease.call_id,payload:''});
    if(prepared.operation?.state!=='prepared')throw Error('挂断未准备成功');
    const pending={type:'call',action:'hangup',id:prepared.operation.id,state:'prepared',submit_started:true,test10010:true};
    await this.setPending({...pending});
    const result=await this.request({op:'submit_call',operation_id:pending.id,lease,urgent:true});
    pending.state=result.operation?.state||'unknown';await this.setPending({...pending});
    await this.request({op:'call_status'});
    const call=await this.request({op:'call_detail',call_id:lease.call_id});confirmed=call.state==='ended'&&call.availability==='observed';
   }catch{ /* Never retry an uncertain submit. The unrenewed Core lease expires. */ }}
   await closing;this.running=false;
   const cleanup=this.dialStarted&&!confirmed?'；挂断尚未确认，设备租约到期后清理，请勿立即重拨':'';
   this.say((this.error?'测试停止：'+this.error+(confirmed?'；电话已挂断':''):confirmed?'测试已结束，电话已挂断':'测试已停止')+cleanup);
   this.error=null;
  })();return this.finishing;
 }
}
