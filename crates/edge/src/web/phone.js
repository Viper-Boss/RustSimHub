// SPDX-License-Identifier: AGPL-3.0-only
// Daily controls use the same durable operation journal, lease and AudioLink.
// Polling only observes/renews this client's lease; it never dials or answers.
const uuid=value=>typeof value==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const states=['prepared','submitting','submitted','cancelled','unknown'];
const terminal=value=>['submitted','cancelled'].includes(value?.state);
const names={outgoing_pending:'正在发起',dialing:'正在拨号',ringing_out:'对方响铃',ringing_in:'来电响铃',active:'通话中',held:'保持中',waiting:'等待中',ended:'已结束'};
function operation(result,id,line){
 const op=result?.operation;
 if(!uuid(op?.id)||op.id!==id||!states.includes(op.state)||!Number.isSafeInteger(op.line)||op.line<=0||line!==undefined&&op.line!==line||op.call_id!==null&&!uuid(op.call_id))throw Error('通话操作响应不匹配，请查询原操作');
 return op;
}
function boundLease(value,call,line){
 return value&&value.call_id===call&&value.line===line&&/^[a-f0-9]{64}$/.test(value.boot)&&Number.isSafeInteger(value.generation)&&value.generation>0&&Number.isInteger(value.expires_in_seconds)&&value.expires_in_seconds>0&&value.expires_in_seconds<=30?{...value}:null;
}
export class DailyPhone {
 constructor(options){
  Object.assign(this,options);this.env=options.env||globalThis;this.ensureSession=options.ensureSession||(()=>Promise.resolve());this.$=id=>this.document.getElementById(id);this.generation=0;this.action=0;this.enabled=false;this.working=false;this.managing=false;this.calls=[];
  this.$('phone-dial').onclick=()=>this.run(()=>this.begin('dial'));
  this.$('phone-hangup').onclick=()=>this.hangup();
  this.$('phone-refresh').onclick=()=>void this.refresh().catch(error=>this.say(error.message));
  this.$('phone-query').onclick=()=>this.run(()=>this.query());
  this.$('phone-close').onclick=()=>this.run(()=>this.close());
  this.$('phone-connect').onclick=()=>this.run(()=>this.connect());
  this.$('phone-tone-send').onclick=()=>this.run(()=>this.begin('dtmf',this.lease?.call_id));
  this.$('phone-muted').onchange=()=>this.audio.mute(this.$('phone-muted').checked);
  this.$('phone-play').onclick=()=>void this.audio.play().catch(()=>this.say('浏览器暂停播放，请检查声音设备'));
  this.render();
 }
 now(){return this.env.performance?.now()??Date.now();}
 current(g,a){if(!this.enabled||g!==this.generation||a!==undefined&&a!==this.action)throw Error('本次电话操作已停止');}
 say(text){this.$('phone-state').textContent=text;}
 token(){return this.context(null)?.token;}
 closeMedia(){if(this.stream)for(const track of this.stream.getTracks())track.stop();this.stream=null;return this.audio.stop();}
 reset(){
  this.enabled=false;++this.generation;++this.action;this.working=false;this.managing=false;this.confirmedControl=false;this.lease=null;this.setLease(null);this.calls=[];this.snapshot=null;this.polling=null;this.audioAttempted=false;this.observedFingerprint=null;this.number=null;
  this.env.clearInterval(this.timer);void this.closeMedia();this.say('请先登录，来电会在这里显示。');this.render();
 }
 start(){this.reset();this.enabled=true;this.say('已登录，等待来电或输入号码拨号。');this.timer=this.env.setInterval(()=>{void this.refresh().catch(error=>this.say(error.message));},2000);void this.refresh().catch(error=>this.say(error.message));this.render();}
 async run(fn){
  if(this.working)return;
  if(!this.enabled||!this.token()){this.say('请先登录');return;}
  if(this.blocked()){this.say('其他操作正在进行，请稍后再试');return;}
  const g=this.generation,a=++this.action;this.working=true;this.render();
  try{await fn();}catch(error){if(g===this.generation&&a===this.action){if(!this.managing)void this.closeMedia();this.say(error.message||'操作未确认，请查询原操作');}}
  finally{if(g===this.generation&&a===this.action){this.working=false;this.render();}}
 }
 async requestCurrent(command,g,a){this.current(g,a);const result=await this.request(command);this.current(g,a);return result;}
 async persist(value,g,a){this.current(g,a);await this.setPending(value);this.current(g,a);}
 async snapshotNow(g,a){
  const line=await this.requestCurrent({op:'line_status'},g,a);
  if(line.status!=='ready'||!Number.isSafeInteger(line.line)||line.line<=0)throw Error('SIM 尚未就绪');
  const status=await this.requestCurrent({op:'call_status'},g,a);
  if(status.available!==true||!Array.isArray(status.live_calls)||status.live_calls.length>64)throw Error('电话状态暂未确认，请稍后刷新');
  const seen=new Set();
  for(const call of status.live_calls){
   if(!uuid(call?.id)||seen.has(call.id)||call.line!==line.line||call.availability!=='observed'||![0,1,2].includes(call.direction)||!Number.isInteger(call.modem_state)||call.modem_state<0||call.modem_state>7||!names[call.state])throw Error('实时电话目录已变化，请刷新');
   seen.add(call.id);
  }
  const live=status.live_calls.filter(call=>call.modem_state!==7);
  if(status.active_calls!==live.length)throw Error('电话数量尚未确认，请刷新');
  return {line:line.line,calls:live,all:status.live_calls};
 }
 async detail(id,snapshot,g,a){
  const result=await this.requestCurrent({op:'call_detail',call_id:id},g,a);
  if(result.id!==id||result.line!==snapshot.line||result.availability!=='observed'||typeof result.number!=='string'||result.number.length>256)throw Error('当前呼叫详情已变化，请刷新');
  return result;
 }
 async refresh(){
  if(!this.enabled||!this.token()||this.working||this.blocked()||this.polling)return;
  const g=this.generation,a=this.action,marker={};this.polling=marker;
  try{
   if(this.managing&&this.confirmedControl){await this.ensureSession();this.current(g,a);}
   const snapshot=await this.snapshotNow(g,a);this.current(g,a);this.snapshot=snapshot;this.calls=snapshot.calls;
   const fingerprint=snapshot.calls.map(call=>call.id+':'+call.modem_state).join(',');
   if(fingerprint!==this.observedFingerprint){this.observedFingerprint=fingerprint;this.say(snapshot.calls.length?snapshot.calls.some(call=>call.modem_state===3)?'有来电，请接听或拒接。':snapshot.calls.some(call=>call.modem_state===4)?'电话已接通。':'正在等待电话接通…':'已登录，当前没有通话。');}
   if(this.lease){
    const own=snapshot.calls.find(call=>call.id===this.lease.call_id);
    if(snapshot.line!==this.lease.line)throw Error('SIM 线路已变化；声音已关闭，请核对原操作');
    if(!own){
     const ended=snapshot.all.find(call=>call.id===this.lease.call_id&&call.modem_state===7);
     if(!ended)throw Error('当前通话失去确认；声音已关闭，请查询原操作');
     this.lease=null;this.managing=false;this.setLease(null);void this.closeMedia();this.say('电话已结束');
    }else{
     if(this.confirmedControl&&this.now()-this.lastRenew>=8000){
      const old=this.lease,result=await this.requestCurrent({op:'renew_call',lease:old},g,a);
      const renewed=boundLease(result.lease,old.call_id,old.line);
      if(!renewed||renewed.boot!==old.boot||renewed.generation!==old.generation)throw Error('控制租约失去确认；声音已关闭');
      this.lease=renewed;this.setLease(renewed);this.lastRenew=this.now();
     }
     if(this.confirmedControl&&own.state==='active'&&own.modem_state===4&&!this.audioAttempted){
      this.audioAttempted=true;
      try{await this.audio.start(this.context(this.lease),{stream:this.stream});this.current(g,a);}
      catch(error){this.current(g,a);this.say('电话已接通，声音未连接：'+error.message+'；可手动连接声音');}
     }else if(['ready'].includes(this.audio.state)){
      try{await this.audio.hold();this.current(g,a);}catch(error){this.current(g,a);this.audioAttempted=true;void this.closeMedia();this.say('声音准备已结束；接通后可手动连接声音');}
     }
    }
   }
   const selected=this.lease?this.calls.find(call=>call.id===this.lease.call_id):this.calls[0];
   if(selected){const info=await this.detail(selected.id,snapshot,g,a);this.current(g,a);this.number={id:info.id,value:info.number};}
   else this.number=null;
   this.render();
  }catch(error){
   if(g===this.generation&&a===this.action&&this.enabled){this.calls=[];this.snapshot=null;this.observedFingerprint=null;if(this.lease){this.lease=null;this.managing=false;this.setLease(null);void this.closeMedia();}this.say(error.message||'电话状态暂未确认');this.render();}
  }finally{if(this.polling===marker)this.polling=null;}
 }
 async slot(g,a,cleanup=false){
  const old=this.getPending();if(!old)return null;
  if(old.type!=='call')throw Error('请先完成或查询短信操作');
  const result=await this.requestCurrent({op:'call_operation',operation_id:old.id},g,a);
  const op=operation(result,old.id,old.line);
  if(terminal(op)&&!old.prior_operation){
   await this.archiveKnown({...old,state:op.state});this.current(g,a);await this.persist(null,g,a);return null;
  }
  if(cleanup&&this.lease&&op.call_id===this.lease.call_id&&old.action!=='hangup')return {...old,state:op.state,call_id:op.call_id};
  throw Error('上一笔电话操作需要查询或关闭，未再次提交');
 }
 async input(g,a){
  if(!this.audioReady())throw Error('此浏览器尚未启用通话声音');
  this.say('请允许麦克风，声音准备完成后才会拨号或接听');
  const stream=await this.audio.input();
  try{this.current(g,a);}catch(error){for(const track of stream.getTracks())track.stop();throw error;}
  this.stream=stream;return stream;
 }
 async begin(action,target=null){
  const g=this.generation,a=this.action;
  if(['dial','accept','dtmf'].includes(action)){await this.ensureSession();this.current(g,a);}
  const payload=action==='dial'?this.$('phone-number').value.trim():action==='dtmf'?this.$('phone-tone').value.toUpperCase():'';
  if(action==='dial'&&(payload.length>32||!/^\+?[0-9]{3,32}$/.test(payload)))throw Error('请输入有效的拨出号码');
  if(action==='dtmf'&&!/^[0-9ABCD*#]$/.test(payload))throw Error('每次发送一个按键：0–9、*、#、A–D');
  const prior=await this.slot(g,a,action==='hangup');
  let snapshot=await this.snapshotNow(g,a);
  if(action==='dial'&&snapshot.calls.length)throw Error('还有未结束的电话，请先处理');
  const incoming=()=>snapshot.calls.find(call=>call.id===target&&call.direction===1&&call.modem_state===3&&call.state==='ringing_in');
  if(action==='accept'||action==='reject'){if(!incoming()||snapshot.calls.length!==1||this.lease)throw Error('这通来电已变化，请刷新后核对');}
  if(['dial','accept'].includes(action)){
   const stream=await this.input(g,a);
   await this.ensureSession();this.current(g,a);
   if(action==='dial')await this.audio.connect(this.context(null),{stream});else await this.audio.prime(stream);
   this.current(g,a);snapshot=await this.snapshotNow(g,a);
   if(action==='dial'&&(snapshot.calls.length||this.$('phone-number').value.trim()!==payload))throw Error('号码或电话状态已变化，未拨号');
   if(action==='accept'&&!incoming())throw Error('来电已经结束或被接听，未再次接听');
  }
  if(['hangup','dtmf'].includes(action)&&(!this.lease||target!==this.lease.call_id||snapshot.line!==this.lease.line||!snapshot.calls.some(call=>call.id===target)))throw Error('当前电话控制权未确认');
  if(action==='dtmf'&&!snapshot.calls.some(call=>call.id===target&&call.modem_state===4&&call.state==='active'))throw Error('请等待通话接通后再发送按键');
  const actual=action==='reject'?'hangup':action;
  const prepared=await this.requestCurrent({op:'prepare_call',line_epoch:snapshot.line,idempotency_key:this.env.crypto.randomUUID(),action:actual,call_id:target,payload},g,a);
  const op=operation(prepared,prepared.operation?.id,snapshot.line);
  if(op.state!=='prepared'||action!=='dial'&&op.call_id!==target)throw Error('电话操作未准备成功');
  let pending={type:'call',action:actual,id:op.id,line:snapshot.line,call_id:op.call_id,state:'prepared',submit_started:false,daily_phone:true,...(action==='reject'?{rejection:true}:{}),...(prior?{prior_operation:prior}:{})};
  await this.persist({...pending},g,a);
  pending.submit_started=true;await this.persist({...pending},g,a);
  this.say({dial:'正在拨号…',accept:'正在接听…',reject:'正在拒接…',hangup:'正在挂断…',dtmf:'正在发送按键…'}[action]);
  const result=await this.requestCurrent({op:'submit_call',operation_id:op.id,lease:action==='reject'?null:this.lease||null,urgent:actual==='hangup'},g,a);
  const submitted=operation(result,op.id,snapshot.line);
  const lease=boundLease(result.lease,submitted.call_id,snapshot.line);
  pending={...pending,state:submitted.state,call_id:submitted.call_id};await this.persist(pending,g,a);
  if(['dial','accept'].includes(action)&&lease){this.lease=lease;this.managing=true;this.setLease(lease);this.lastRenew=this.now();this.confirmedControl=submitted.state==='submitted';this.audioAttempted=!this.confirmedControl;}
  if(submitted.state!=='submitted'){void this.closeMedia();throw Error('本次电话操作结果未确认，请查询；不会自动重试');}
  if(['dial','accept'].includes(action)&&!lease)throw Error('本次控制权尚未确认，请查询原操作；不会再次拨号或接听');
  if(actual==='hangup'){
   this.lease=null;this.managing=false;this.setLease(null);void this.closeMedia();
   this.say(action==='reject'?'来电已拒接':'电话已挂断');
  }else this.say(action==='dtmf'?'按键已发送':'已提交，正在观察接通状态…');
 }
 hangup(){
  // Cancel setup/permission work immediately; a late permission grant or reply
  // cannot submit or overwrite a newer hangup. Lost submits remain journaled.
  if(this.working){++this.action;this.working=false;void this.closeMedia();if(!this.lease){this.say('已停止本地准备；若提交已发出，请查询原操作，设备租约到期后清理');this.render();return Promise.resolve();}}
  if(!this.lease){void this.closeMedia();this.say('没有可确认控制权的通话；请查询当前操作');return Promise.resolve();}
  const target=this.lease.call_id,g=this.generation;void this.closeMedia();return this.run(async()=>{try{await this.begin('hangup',target);}finally{if(g===this.generation&&this.lease?.call_id===target){this.lease=null;this.managing=false;this.setLease(null);void this.closeMedia();}}});
 }
 async query(){
  const g=this.generation,a=this.action,old=this.getPending();if(old?.type!=='call')throw Error('没有待查询的电话操作');
  const snapshot=await this.snapshotNow(g,a);const result=await this.requestCurrent({op:'call_operation',operation_id:old.id},g,a);
  const op=operation(result,old.id,old.line??snapshot.line);await this.persist({...old,state:op.state,call_id:op.call_id,line:op.line},g,a);
  const lease=boundLease(result.lease,op.call_id,snapshot.line);
  if(lease){this.lease=lease;this.managing=true;this.setLease(lease);this.lastRenew=this.now();this.confirmedControl=op.state==='submitted';this.audioAttempted=true;}
  this.say('操作状态：'+({submitted:'已提交',cancelled:'已取消',prepared:'尚未提交',unknown:'结果未确认',submitting:'结果待确认'}[op.state])+'；没有重复提交');
 }
 async close(){
  const g=this.generation,a=this.action,old=this.getPending();if(old?.type!=='call'||this.managing)throw Error('请先结束电话并查询结果');
  const result=await this.requestCurrent({op:'call_operation',operation_id:old.id},g,a);let op=operation(result,old.id,old.line);
  if(op.state==='prepared'){
   const result=await this.requestCurrent({op:'cancel_prepared_call',operation_id:old.id},g,a);op=operation(result,old.id,old.line);if(op.state!=='cancelled')throw Error('取消结果尚未确认');
   await this.persist({...old,state:'cancelled'},g,a);
  }
  if(old.prior_operation||!terminal(op)){
   const snapshot=await this.snapshotNow(g,a);if(snapshot.calls.length||!uuid(op.call_id))throw Error('电话尚未确认结束，请保留原操作');
   const detail=await this.requestCurrent({op:'call_detail',call_id:op.call_id},g,a);
   if(detail.id!==op.call_id||!(detail.state==='ended'&&detail.availability==='observed'||detail.state==='reconciling'&&detail.availability==='reconciling'&&detail.modem_state===7))throw Error('原电话尚未确认结束，请保留操作');
   await this.archivePending({...old,state:op.state,ended_call_id:op.call_id,manual_archive:true});this.current(g,a);await this.persist(null,g,a);this.say('已核对电话结束并保留不确定回执；没有再次提交');return;
  }
  await this.slot(g,a);this.say('已保存回执并关闭本次电话操作');
 }
 async connect(){
  const g=this.generation,a=this.action,lease=this.lease;if(!lease||!this.confirmedControl)throw Error('请先查询并确认当前电话控制权');
  await this.ensureSession();this.current(g,a);
  const snapshot=await this.snapshotNow(g,a);if(snapshot.line!==lease.line||!snapshot.calls.some(call=>call.id===lease.call_id&&call.modem_state===4))throw Error('请等待电话接通');
  try{const stream=await this.input(g,a);await this.ensureSession();this.current(g,a);await this.audio.start(this.context(lease),{stream});this.current(g,a);this.audioAttempted=true;this.say('声音已连接');}
  catch(error){if(g===this.generation&&a===this.action)void this.closeMedia();throw error;}
 }
 render(){
  const online=this.enabled&&!!this.token(),blocked=this.blocked(),pending=this.getPending();
  this.$('phone-dial').disabled=!online||blocked||this.working||this.managing||!!this.calls.length;
  this.$('phone-hangup').disabled=!online||!this.lease&&!this.working;
  this.$('phone-query').disabled=!online||blocked||this.working||pending?.type!=='call';
  this.$('phone-close').disabled=!online||blocked||this.working||this.managing||pending?.type!=='call';
  this.$('phone-connect').disabled=!online||blocked||this.working||!this.lease||!this.confirmedControl||this.audio.state==='active';
  this.$('phone-tone-send').disabled=!online||blocked||this.working||!this.lease||!this.calls.some(call=>call.id===this.lease.call_id&&call.modem_state===4);
  this.$('phone-muted').checked=this.audio.muted===true;
  const list=this.$('phone-live');list.replaceChildren();
  for(const call of this.calls){
   const row=this.document.createElement('div');row.className='phone-call';const label=this.document.createElement('p');
   const number=this.number?.id===call.id?this.number.value:'未读取号码';label.textContent=`${call.direction===1?'来电':'呼出'} · ${number||'未知号码'} · ${names[call.state]}`;row.append(label);
   if(call.direction===1&&call.modem_state===3){for(const [action,text] of [['accept','接听'],['reject','拒接']]){const button=this.document.createElement('button');button.textContent=text;button.disabled=!online||blocked||this.working||this.managing;button.onclick=()=>this.run(()=>this.begin(action,call.id));row.append(button);}}
   if(this.lease?.call_id===call.id){const sound=this.document.createElement('p');sound.textContent=this.audio.state==='active'?'声音已连接':this.audio.lastMessage||'正在准备声音';row.append(sound);}
   list.append(row);
  }
 }
}
