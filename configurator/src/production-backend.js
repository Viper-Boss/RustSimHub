// SPDX-License-Identifier: AGPL-3.0-only
// Explicit candidate factory, never a global flag or mock auto-selection.
// Uses the actual Web Bluetooth API and public fixed characteristics. The
// trusted management adapter supplies the previously stored pin/realm binding.
import {openStoredRecoverySession} from './crypto/secure-session.js';
import {fragments,Receiver} from './gatt-framing.js';
import {pin,requestId,credentials,need} from './contracts.js';
export const UUID=Object.freeze({service:'002c85a2-c6d6-411c-b266-9e99fb4cdd0a',auth:'f443c973-2d89-4991-830f-634bd75dc3cd',command:'dc9899ec-c8f0-43cf-9202-ed5cdebd4562',result:'4faaacb4-d4aa-44df-afdd-5e474c9e7f86'});
export function createRecoveryBackend({wasm,realmForDevice,bluetooth=globalThis.navigator?.bluetooth,clock=()=>performance.now()}={}){
 need(wasm?.RecoverySessionClient&&typeof realmForDevice==='function'&&typeof bluetooth?.requestDevice==='function','unsupported');
 let epoch=0,active=null,connecting=false;
 const close=()=>{const s=active;if(!s)return;active=null;s.receiver.close();s.pending?.reject(Error('transport_lost'));s.pending=null;s.abort?.();for(const [c,h]of s.listeners)c.removeEventListener('characteristicvaluechanged',h);s.device.removeEventListener('gattserverdisconnected',s.disconnected);s.crypto?.close();s.device.gatt?.disconnect();};
 const current=(s)=>need(active===s&&s.epoch===epoch&&!s.signal?.aborted,'transport_lost');
 const exchange=async(s,kind,wire,{signal})=>{
  current(s);need(!s.pending,'busy');need(kind===1||kind===2,'protocol_error');
  const frames=fragments(kind,s.next++,wire);let resolve,reject;
  const reply=new Promise((r,j)=>{resolve=r;reject=j;});reply.catch(()=>{});
  const abort=()=>close();signal?.addEventListener('abort',abort,{once:true});
  s.pending={kind:kind===1?1:3,resolve,reject};
  try{
   if(signal?.aborted)throw Error('transport_lost');
   const characteristic=kind===1?s.auth:s.command;
   for(const frame of frames){current(s);await characteristic.writeValueWithResponse(frame);current(s);}
   const value=await reply;current(s);return value;
  }catch(error){close();throw error;}
  finally{signal?.removeEventListener('abort',abort);if(active===s)s.pending=null;}
 };
 const subscribe=async(s,characteristic,kind)=>{
  const handler=event=>{
   if(active!==s)return;
   try{
    current(s);need(s.pending?.kind===kind,'protocol_error');
    const view=event.target.value;need(view instanceof DataView,'protocol_error');
    const wire=s.receiver.receive(kind,new Uint8Array(view.buffer,view.byteOffset,view.byteLength).slice());
    if(wire){const pending=s.pending;s.pending=null;pending.resolve(wire);}
   }catch{close();}
  };
  characteristic.addEventListener('characteristicvaluechanged',handler);s.listeners.push([characteristic,handler]);
  await characteristic.startNotifications();current(s);
 };
 const request=async(action,body,{signal,requestId:id}={})=>{
  const s=active;need(s?.crypto,'unavailable');current(s);if(id!==undefined)need(requestId(id),'invalid_input');
  const bytes=new TextEncoder().encode(JSON.stringify(body));
  try{return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(await s.crypto.request(action,bytes,{signal,requestId:id})));}
  catch(error){close();throw error;}finally{bytes.fill(0);}
 };
 return Object.freeze({
  async connectFromGesture({expectedPin,signal}={}){
   need(pin(expectedPin)&&!connecting&&!active,'busy');need(!signal?.aborted,'transport_lost');connecting=true;const captured=++epoch;
   // Must remain before the first await: Web Bluetooth requires user activation.
   let selection;try{selection=bluetooth.requestDevice({filters:[{services:[UUID.service]}]});}catch(error){connecting=false;throw error;}
   let timer;
   try{
    const device=await selection;
    if(captured!==epoch||signal?.aborted){device.gatt?.disconnect();throw Error('transport_lost');}
    const s={epoch:captured,device,signal,next:1,receiver:new Receiver(clock),listeners:[],pending:null};active=s;
    s.disconnected=()=>close();device.addEventListener('gattserverdisconnected',s.disconnected);
    s.abort=()=>signal?.removeEventListener('abort',s.disconnected);signal?.addEventListener('abort',s.disconnected,{once:true});
    const deadline=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(Error('deadline'));close();},10000);});
    const setup=(async()=>{
     const realm=await realmForDevice(expectedPin);current(s);
     const server=await device.gatt.connect();if(active!==s){server.disconnect();throw Error('transport_lost');}current(s);
     const service=await server.getPrimaryService(UUID.service);current(s);
     s.auth=await service.getCharacteristic(UUID.auth);current(s);
     s.command=await service.getCharacteristic(UUID.command);current(s);
     s.result=await service.getCharacteristic(UUID.result);current(s);
     await subscribe(s,s.auth,1);await subscribe(s,s.result,3);
     s.crypto=await openStoredRecoverySession({wasm,devicePin:expectedPin,realm,signal,clock,exchange:(...args)=>exchange(s,...args),closeTransport:()=>{if(active===s)close();}});current(s);
    })();
    await Promise.race([setup,deadline]);
   }catch(error){if(captured===epoch)close();throw error;}
   finally{clearTimeout(timer);if(captured===epoch)connecting=false;}
  },
  readStatus:options=>request(1,{},options),
  beginWifi:({ssid,passphrase,requestId:id,signal})=>{const v=credentials(ssid,passphrase);need(requestId(id),'invalid_input');return request(2,v,{requestId:id,signal});},
  queryOperation:({targetRequestId,signal})=>{need(requestId(targetRequestId),'invalid_input');return request(3,{target_request_id:targetRequestId},{signal});},
  confirmWifi:({trialId,requestId:id,signal})=>{need(pin(trialId)&&requestId(id),'invalid_input');return request(4,{trial_id:trialId},{requestId:id,signal});},
  cancelWifi:({trialId,requestId:id,signal})=>{need(pin(trialId)&&requestId(id),'invalid_input');return request(5,{trial_id:trialId},{requestId:id,signal});},
  async disconnect(){++epoch;connecting=false;close();},
 });
}
