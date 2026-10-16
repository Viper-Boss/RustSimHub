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
 const input={enabled:true,stopped:false,stop(){this.stopped=true;}};
 const output={stopped:false,stop(){this.stopped=true;}};
 const stream={getTracks:()=>[input],getAudioTracks:()=>[input]};const audio={srcObject:null,paused:false,play:async()=>{},pause(){this.paused=true;}};
 const calls=[],peers=[];let getMedia=0,pollBusy=options.pollBusy||0;
 class Peer {
  constructor(config){assert.deepEqual(config.iceServers,[]);this.iceGatheringState='complete';this.connectionState='new';this.localDescription=null;this.closed=false;this.stopped=false;peers.push(this);}
  addTransceiver(track,init){assert.equal(track,input);assert.equal(init.direction,'sendrecv');const transceiver={setCodecPreferences(codecs){assert.equal(codecs.length,1);assert.equal(codecs[0].mimeType,'audio/opus');},stop:()=>{this.stopped=true;}};this.transceiver=transceiver;return transceiver;}
  getTransceivers(){return[this.transceiver];}
  async createOffer(){return{type:'offer',sdp:'synthetic_sdp'};}
  async setLocalDescription(value){this.localDescription=value;}
  async setRemoteDescription(value){assert.equal(value.type,'answer');this.connectionState='connected';this.ontrack?.({streams:[{getTracks:()=>[output]}],track:output});}
  close(){this.closed=true;this.connectionState='closed';this.onconnectionstatechange?.();}
 }
 const env={isSecureContext:true,crypto:webcrypto,AbortController,RTCPeerConnection:Peer,RTCRtpSender:{getCapabilities:()=>({codecs:[{mimeType:'audio/opus'},{mimeType:'audio/PCMU'}]})},navigator:{mediaDevices:{getUserMedia:async constraints=>{getMedia++;assert.equal(constraints.video,false);return options.permission?options.permission.promise:stream;}}},MediaStream:class{},setTimeout:(fn,ms)=>setTimeout(fn,ms===200?1:ms),clearTimeout};
 env.fetch=async(path,request)=>{
  assert.equal(path,'/api/media/v1');assert.equal(request.credentials,'omit');assert.equal(request.cache,'no-store');const body=JSON.parse(request.body);calls.push(body.command.op);assert(!('peer_key' in body.command)&&!('ticket' in body.command));
  if(options.activateError&&body.command.op==='activate')throw Error('synthetic lost reply');
  if(body.command.op==='poll'&&pollBusy-->0)return{ok:false,status:options.pollStatus||429};
  if(body.command.op==='activate'&&options.activateStatus)return{ok:false,status:options.activateStatus};
  const phase={prepare:'handshaking',poll:'verified',activate:'active',stop:'closed'}[body.command.op];
  return{ok:true,json:async()=>({protocol:1,id:options.wrongId?'wrong':body.id,result:{session:body.command.session,phase,answer:body.command.op==='prepare'?'synthetic_answer':null,pcm_opened:phase==='active',failure:null}})};
 };
 const states=[];const link=new AudioLink(audio,(text,state)=>states.push(state),env);
 return{link,input,output,stream,audio,calls,peers,env,getMedia:()=>getMedia,states};
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
 const f=setup();f.env.isSecureContext=false;await assert.rejects(f.link.start(context),/HTTPS/);assert.equal(f.getMedia(),0);assert.equal(f.calls.length,0);
}
{
 const f=setup();await f.link.start(context);f.peers[0].connectionState='disconnected';f.peers[0].onconnectionstatechange();await turn();assert(f.input.stopped);assert.equal(f.link.state,'closed');assert.equal(f.calls.filter(c=>c==='activate').length,1);
}
console.log('Web media lifecycle passed: single activation, late permission cancellation, mute, close, lost replies, identity and secure-context gates');
