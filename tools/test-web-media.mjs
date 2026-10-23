// SPDX-License-Identifier: AGPL-3.0-only
// Browser lifecycle behavior only. This does not claim real WebRTC/PCM acceptance.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const bytes=await readFile(new URL('../crates/edge/src/web/media.js',import.meta.url));
const {AudioLink}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const turn=()=>new Promise(resolve=>setImmediate(resolve));
const context={token:'a'.repeat(64),lease:{call_id:'synthetic',boot:'b'.repeat(64),generation:1,line:1,expires_in_seconds:30}};
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
function setup(options={}){
 const input={enabled:true,stopped:false,readyState:'live',stop(){this.stopped=true;this.readyState='ended';}};
 const output={stopped:false,stop(){this.stopped=true;}};
 const stream={getTracks:()=>[input],getAudioTracks:()=>[input]};const audio={srcObject:null,paused:false,play:async()=>{},pause(){this.paused=true;}};
 const calls=[],peers=[],requests=[];let getMedia=0,pollBusy=options.pollBusy||0,monitorBusy=options.monitorBusy||0,activated=false,forcedPhase=null;
 class Peer {
  constructor(config){assert.deepEqual(config.iceServers,[]);this.iceGatheringState=options.slowIce?'gathering':'complete';this.connectionState='new';this.localDescription=null;this.closed=false;this.stopped=false;this.listeners=new Set();peers.push(this);}
  addTransceiver(track,init){assert.equal(track,input);assert.equal(init.direction,'sendrecv');const transceiver={setCodecPreferences(codecs){assert.equal(codecs.length,1);assert.equal(codecs[0].mimeType,'audio/opus');},stop:()=>{this.stopped=true;}};this.transceiver=transceiver;return transceiver;}
  getTransceivers(){return[this.transceiver];}
  async createOffer(){this.offers=(this.offers||0)+1;return options.offer?options.offer.promise:{type:'offer',sdp:'synthetic_sdp'};}
  addEventListener(name,fn){assert.equal(name,'icegatheringstatechange');this.listeners.add(fn);}
  removeEventListener(name,fn){assert.equal(name,'icegatheringstatechange');this.listeners.delete(fn);}
  async setLocalDescription(value){this.localDescription=value;}
  async setRemoteDescription(value){assert.equal(value.type,'answer');this.connectionState='connected';this.ontrack?.({streams:[{getTracks:()=>[output]}],track:output});}
  close(){this.closed=true;this.connectionState='closed';this.onconnectionstatechange?.();}
 }
 const env={isSecureContext:true,crypto:webcrypto,AbortController,RTCPeerConnection:Peer,RTCRtpSender:{getCapabilities:()=>({codecs:[{mimeType:'audio/opus'},{mimeType:'audio/PCMU'}]})},navigator:{mediaDevices:{getUserMedia:async constraints=>{getMedia++;assert.equal(constraints.video,false);return options.permission?options.permission.promise:stream;}}},MediaStream:class{},setTimeout:(fn,ms)=>setTimeout(fn,ms===200||ms===1000&&options.fastMonitor?1:ms),clearTimeout};
 env.fetch=async(path,request)=>{
  assert.equal(path,'/api/media/v1');assert.equal(request.credentials,'omit');assert.equal(request.cache,'no-store');const body=JSON.parse(request.body);calls.push(body.command.op);requests.push(body);assert(!('peer_key' in body.command)&&!('ticket' in body.command));
  if(!activated&&body.command.op!=='stop')assert.equal(input.enabled,false);
  if(body.command.op==='connect'&&options.connectWait)await options.connectWait.promise;
  if(options.activateError&&body.command.op==='activate')throw Error('synthetic lost reply');
  if(body.command.op==='poll'&&pollBusy-->0)return{ok:false,status:options.pollStatus||429};
  if(body.command.op==='activate'&&options.activateStatus)return{ok:false,status:options.activateStatus};
  if(body.command.op==='poll'&&activated&&monitorBusy-->0)return{ok:false,status:options.monitorStatus||429};
  const phase=forcedPhase||(body.command.op==='poll'&&activated?'active':{connect:'handshaking',prepare:'handshaking',poll:'verified',activate:'active',stop:'closed'}[body.command.op]);
  if(body.command.op==='activate')activated=true;
  return{ok:true,json:async()=>({protocol:1,id:options.wrongId?'wrong':body.id,result:{session:body.command.session,phase,answer:['prepare','connect'].includes(body.command.op)?'synthetic_answer':null,pcm_opened:phase==='active',failure:null}})};
 };
 const states=[];const link=new AudioLink(audio,(text,state)=>states.push(state),env);
 return{link,input,output,stream,audio,calls,peers,env,requests,phase:value=>{forcedPhase=value;},getMedia:()=>getMedia,states};
}
{
 const f=setup();await f.link.connect({token:context.token,lease:null},{stream:f.stream});assert.equal(f.link.state,'ready');assert.equal(f.input.enabled,false);f.link.mute(false);assert.equal(f.input.enabled,false);await f.link.hold();assert(f.requests.every(r=>r.lease===null));assert.equal(f.calls.filter(c=>c==='activate').length,0);
 await f.link.start(context,{stream:f.stream});assert.equal(f.link.state,'active');assert(f.input.enabled);assert.equal(f.peers.length,1);assert.equal(f.peers[0].offers,1);assert.equal(f.calls.filter(c=>c==='connect').length,1);assert.equal(f.calls.filter(c=>c==='prepare').length,0);assert.equal(f.calls.filter(c=>c==='activate').length,1);await f.link.stop();
}
{
 const connectWait=deferred(),f=setup({connectWait});const connecting=f.link.connect({token:context.token,lease:null},{stream:f.stream}).catch(e=>e);await turn();await f.link.stop();connectWait.resolve();assert.match((await connecting).message,/取消/);assert.equal(f.link.state,'closed');assert(f.input.stopped&&f.peers[0].closed);assert.equal(f.calls.filter(c=>c==='activate').length,0);
}
{
 const f=setup();await f.link.connect({token:context.token,lease:null},{stream:f.stream});f.phase('closed');await assert.rejects(f.link.hold(),/过期/);assert(f.input.stopped&&f.peers[0].closed);assert.equal(f.calls.filter(c=>c==='activate').length,0);
}
{
 const f=setup();await f.link.connect({token:context.token,lease:null},{stream:f.stream});await assert.rejects(f.link.start({...context,token:'c'.repeat(64)},{stream:f.stream}),/麦克风/);assert(f.input.stopped);assert.equal(f.calls.filter(c=>c==='activate').length,0);
}
{
 const f=setup();await f.link.prime(f.stream);assert.equal(f.link.state,'primed');assert.equal(f.calls.length,0);assert.equal(f.getMedia(),0);assert.equal(f.link.context,null);
 const peer=f.peers[0];await f.link.start(context,{stream:f.stream});assert.equal(f.peers.length,1);assert.equal(peer.offers,1);assert.equal(f.calls.filter(c=>c==='activate').length,1);assert.equal(f.input.stopped,false);await f.link.stop();assert(peer.closed&&f.input.stopped);
}
{
 const f=setup({slowIce:true});const priming=f.link.prime(f.stream).catch(error=>error);await turn();const peer=f.peers[0];assert.equal(peer.listeners.size,1);
 const late=[...peer.listeners];await f.link.stop();peer.iceGatheringState='complete';for(const changed of late)changed();assert.match((await priming).message,/取消/);assert.equal(peer.listeners.size,0);assert.equal(f.calls.length,0);assert.equal(f.link.prepared,null);assert(peer.closed&&f.input.stopped);
}
{
 const offer=deferred(),f=setup({offer});const priming=f.link.prime(f.stream).catch(error=>error);await turn();await f.link.stop();offer.resolve({type:'offer',sdp:'late'});
 assert.match((await priming).message,/取消/);assert.equal(f.calls.length,0);assert.equal(f.link.prepared,null);assert(f.input.stopped&&f.peers[0].closed);
}
{
 const f=setup();f.link.mute(true);await f.link.start(context);assert.equal(f.input.enabled,false);assert.equal(f.link.state,'active');assert.equal(f.calls.filter(c=>c==='activate').length,1);
 f.link.mute(false);assert.equal(f.input.enabled,true);await f.link.stop();assert(f.input.stopped&&f.output.stopped&&f.peers[0].closed&&f.peers[0].stopped);assert.equal(f.audio.srcObject,null);assert.equal(f.link.state,'closed');assert.equal(f.calls.filter(c=>c==='stop').length,1);
}
{
 const f=setup();const result=f.link.start(context).catch(error=>error);await f.link.stop();assert.match((await result).message,/取消/);assert.equal(f.getMedia(),0);assert.equal(f.calls.length,0);
}
{
 const permission=deferred(),f=setup({permission});const result=f.link.start(context).catch(error=>error);await turn();assert.equal(f.getMedia(),1);await f.link.stop();permission.resolve(f.stream);assert.match((await result).message,/取消/);assert(f.input.stopped);assert.equal(f.peers.length,0);assert.equal(f.calls.filter(c=>c!=='stop').length,0);
}
{
 const f=setup({activateError:true});await assert.rejects(f.link.start(context));await turn();assert(f.input.stopped&&f.peers[0].closed);assert.equal(f.calls.filter(c=>c==='activate').length,1);assert.equal(f.calls.filter(c=>c==='stop').length,1);assert.equal(f.link.state,'closed');
}
{
 const f=setup({wrongId:true});await assert.rejects(f.link.start(context),/不匹配/);await turn();assert(f.input.stopped&&f.peers[0].closed);assert.equal(f.calls.filter(c=>c==='activate').length,0);
}
{
 const f=setup({pollBusy:2});await f.link.start(context);assert.equal(f.link.state,'active');assert.equal(f.calls.filter(c=>c==='poll').length,3);assert.equal(f.calls.filter(c=>c==='activate').length,1);await f.link.stop();
}
{
 const f=setup({pollBusy:10});await assert.rejects(f.link.start(context),/HTTP 429/);assert.equal(f.calls.filter(c=>c==='poll').length,3);assert.equal(f.calls.filter(c=>c==='activate').length,0);assert(f.input.stopped);
}
{
 const f=setup({pollBusy:10,pollStatus:403});await assert.rejects(f.link.start(context),/HTTP 403/);assert.equal(f.calls.filter(c=>c==='poll').length,1);assert(f.input.stopped);
}
{
 const f=setup({activateStatus:429});await assert.rejects(f.link.start(context),/HTTP 429/);assert.equal(f.calls.filter(c=>c==='activate').length,1);assert(f.input.stopped);
}
{
 const f=setup({monitorBusy:2,fastMonitor:true});await f.link.start(context);await new Promise(resolve=>setTimeout(resolve,180));assert.equal(f.link.state,'active');assert.equal(f.calls.filter(c=>c==='activate').length,1);assert.equal(f.calls.filter(c=>c==='stop').length,0);await f.link.stop();
}
{
 const f=setup({monitorBusy:2,monitorStatus:403,fastMonitor:true});await f.link.start(context);await new Promise(resolve=>setTimeout(resolve,20));assert.equal(f.link.state,'closed');assert(f.input.stopped);assert.match(f.link.lastMessage,/HTTP 403/);assert.equal(f.calls.filter(c=>c==='activate').length,1);
}
{
 const f=setup();f.env.isSecureContext=false;await assert.rejects(f.link.start(context),/HTTPS/);assert.equal(f.getMedia(),0);assert.equal(f.calls.length,0);
}
{
 const f=setup();await f.link.start(context);f.peers[0].connectionState='disconnected';f.peers[0].onconnectionstatechange();await turn();assert(f.input.stopped);assert.equal(f.link.state,'closed');assert.equal(f.calls.filter(c=>c==='activate').length,1);
}
{
 const f=setup();await f.link.start(context);const pc=f.link.pc,session=f.link.session,g=f.link.generation,stream=f.link.stream;
 f.link.updateAuthentication(context.token,'e'.repeat(64));assert.equal(f.link.pc,pc);assert.equal(f.link.session,session);assert.equal(f.link.generation,g);assert.equal(f.link.stream,stream);assert.equal(f.link.state,'active');
 await f.link.poll(g);assert.equal(f.requests.at(-1).token,'e'.repeat(64));assert.equal(f.calls.filter(c=>c==='activate').length,1);
 assert.throws(()=>f.link.updateAuthentication(context.token,'f'.repeat(64)),/已变化/);assert.equal(f.link.context.token,'e'.repeat(64));await f.link.stop();
}
{
 const f=setup();await f.link.connect({token:context.token,lease:null},{stream:f.stream});f.link.updateAuthentication(context.token,'e'.repeat(64));
 await f.link.start({...context,token:'e'.repeat(64)},{stream:f.stream});assert.equal(f.peers.length,1);assert.equal(f.calls.filter(c=>c==='connect').length,1);assert.equal(f.calls.filter(c=>c==='prepare').length,0);assert.equal(f.calls.filter(c=>c==='activate').length,1);await f.link.stop();
}
console.log('Web media lifecycle passed: single activation, authentication rotation without reconnect, late permission cancellation, mute, close, lost replies, identity and secure-context gates');
