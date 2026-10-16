// SPDX-License-Identifier: AGPL-3.0-only
// No recording, persistent credentials, automatic dialing or automatic reconnect.
export class AudioLink {
 constructor(audio,notify=()=>{},env=globalThis){this.audio=audio;this.notify=notify;this.env=env;this.generation=0;this.controllers=new Set();this.waiters=new Set();this.context=null;this.session=null;this.pc=null;this.stream=null;this.state='closed';this.muted=false;}
 current(g){if(g!==this.generation)throw Error('声音连接已取消');}
 say(state,text){this.state=state;this.notify(text,state);}
 async send(context,session,command,g,cleanup=false){
  const controller=new this.env.AbortController();if(!cleanup)this.controllers.add(controller);
  const timer=this.env.setTimeout(()=>controller.abort(),cleanup?1500:7000);const id=this.env.crypto.randomUUID();
  try{
   const response=await this.env.fetch('/api/media/v1',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:1,id,token:context.token,lease:context.lease,command:{...command,session}}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error',keepalive:cleanup});
   if(!cleanup)this.current(g);if(!response.ok)throw Error('声音请求未获确认；请检查通话、音频权限与局域网连接');
   const value=await response.json();if(!cleanup)this.current(g);
   if(value.protocol!==1||value.id!==id||value.result?.session!==session)throw Error('声音响应不匹配');
   return value.result;
  }finally{this.env.clearTimeout(timer);this.controllers.delete(controller);}
 }
 wait(ms,g){return new Promise((resolve,reject)=>{const cancel=()=>{this.env.clearTimeout(timer);this.waiters.delete(cancel);reject(Error('声音连接已取消'));};const timer=this.env.setTimeout(()=>{this.waiters.delete(cancel);try{this.current(g);resolve();}catch(error){reject(error);}},ms);this.waiters.add(cancel);});}
 ice(pc,g){
  if(pc.iceGatheringState==='complete')return Promise.resolve();
  return new Promise((resolve,reject)=>{
   const finish=(error)=>{this.env.clearTimeout(timer);pc.removeEventListener('icegatheringstatechange',changed);this.waiters.delete(cancel);if(error)reject(error);else resolve();};
   const changed=()=>{if(pc.iceGatheringState==='complete')finish();};const cancel=()=>finish(Error('声音连接已取消'));
   const timer=this.env.setTimeout(()=>finish(Error('声音地址收集超时；没有自动重连')),4000);
   pc.addEventListener('icegatheringstatechange',changed);this.waiters.add(cancel);
   try{this.current(g);changed();}catch(error){finish(error);}
  });
 }
 async input(){
  if(this.env.isSecureContext!==true||!this.env.navigator?.mediaDevices?.getUserMedia||!this.env.RTCPeerConnection||!this.env.RTCRtpSender?.getCapabilities)throw Error('请使用受信任的 HTTPS 和支持语音的浏览器');
  return this.env.navigator.mediaDevices.getUserMedia({audio:{echoCancellation:true,noiseSuppression:true,autoGainControl:true},video:false});
 }
 async start(context,{stream:provided=null}={}){
  const closing=this.stop(),g=this.generation;
  try{
  await closing;this.current(g);
  if(this.env.isSecureContext!==true)throw Error('请使用受信任的 HTTPS 和支持语音的浏览器');
  if(!context?.token||!context.lease)throw Error('请先登录并取得当前通话的控制租约');
  this.context={token:context.token,lease:{...context.lease}};this.session=this.env.crypto.randomUUID().replaceAll('-','');
  this.say('opening','正在等待麦克风权限…');
   const stream=provided||await this.input();
   if(g!==this.generation){for(const track of stream.getTracks())track.stop();throw Error('声音连接已取消');}
   this.stream=stream;const tracks=stream.getAudioTracks();if(tracks.length!==1)throw Error('未取得单个麦克风输入');
   tracks[0].enabled=!this.muted;
   const pc=new this.env.RTCPeerConnection({iceServers:[],bundlePolicy:'max-bundle',rtcpMuxPolicy:'require'});this.pc=pc;
   const transceiver=pc.addTransceiver(tracks[0],{direction:'sendrecv'});
   const codecs=this.env.RTCRtpSender.getCapabilities('audio')?.codecs?.filter(c=>c.mimeType.toLowerCase()==='audio/opus')||[];
   if(!codecs.length||typeof transceiver.setCodecPreferences!=='function')throw Error('此浏览器暂不能协商所需的音频格式');transceiver.setCodecPreferences(codecs);
   tracks[0].onended=()=>{if(g===this.generation){void this.stop();this.say('closed','麦克风已关闭');}};
   pc.ontrack=event=>{if(g!==this.generation)return;this.audio.srcObject=event.streams[0]||new this.env.MediaStream([event.track]);this.audio.play().catch(()=>{if(g===this.generation)this.say(this.state,'浏览器暂停了播放，请点击“播放声音”');});};
   pc.onconnectionstatechange=()=>{if(g===this.generation&&['failed','closed','disconnected'].includes(pc.connectionState)){void this.stop();this.say('closed','声音连接已断开；可检查网络后手动重连');}};
   const offer=await pc.createOffer();this.current(g);await pc.setLocalDescription(offer);this.current(g);await this.ice(pc,g);this.current(g);
   const sdp=pc.localDescription?.sdp;if(!sdp||sdp.length>16384)throw Error('声音协商信息无效');
   this.say('handshaking','正在建立加密声音连接…');
   const answer=await this.send(this.context,this.session,{op:'prepare',offer:sdp},g);if(typeof answer.answer!=='string'||!answer.answer.length)throw Error('声音协商未获得有效应答');
   await pc.setRemoteDescription({type:'answer',sdp:answer.answer});this.current(g);
   const now=()=>this.env.performance?.now()??Date.now();const end=now()+8000;let snapshot;
   do{snapshot=await this.send(this.context,this.session,{op:'poll'},g);if(snapshot.phase==='verified')break;if(snapshot.phase!=='handshaking'||now()>=end)throw Error('声音握手未完成；没有自动重试');await this.wait(200,g);}while(true);
   // Exactly one activation. A lost reply is closed, never resubmitted.
   snapshot=await this.send(this.context,this.session,{op:'activate'},g);
   while(snapshot.phase==='authorizing'){await this.wait(200,g);snapshot=await this.send(this.context,this.session,{op:'poll'},g);if(now()>=end)throw Error('声音授权未完成；没有自动重试');}
   if(snapshot.phase!=='active'||snapshot.pcm_opened!==true)throw Error('当前通话的声音未就绪');
   this.say('active','声音已连接');void this.monitor(g);return snapshot;
  }catch(error){if(provided)for(const track of provided.getTracks())track.stop();if(g===this.generation){void this.stop();this.say('closed',error.message||'声音连接失败');}throw error;}
 }
 async monitor(g){
  try{while(g===this.generation){await this.wait(1000,g);const snapshot=await this.send(this.context,this.session,{op:'poll'},g);if(snapshot.phase!=='active'||snapshot.failure){void this.stop();this.say('closed','声音连接已结束；请检查当前通话后手动重连');return;}}}
  catch{if(g===this.generation){void this.stop();this.say('closed','声音连接失去确认，麦克风已关闭');}}
 }
 stop(){
  const context=this.context,session=this.session;++this.generation;this.context=null;this.session=null;
  for(const controller of this.controllers)controller.abort();this.controllers.clear();for(const cancel of [...this.waiters])cancel();this.waiters.clear();
  const pc=this.pc;this.pc=null;if(pc){for(const t of pc.getTransceivers())try{t.stop();}catch{}try{pc.close();}catch{}}
  const stream=this.stream;this.stream=null;if(stream)for(const track of stream.getTracks())track.stop();
  if(this.audio.srcObject)for(const track of this.audio.srcObject.getTracks())track.stop();this.audio.pause();this.audio.srcObject=null;this.state='closed';
  if(context&&session)return this.send(context,session,{op:'stop'},this.generation,true).catch(()=>{});return Promise.resolve();
 }
 mute(muted){this.muted=!!muted;if(this.stream)for(const track of this.stream.getAudioTracks())track.enabled=!this.muted;}
 async play(){if(this.audio.srcObject)await this.audio.play();}
}
