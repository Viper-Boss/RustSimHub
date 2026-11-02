// SPDX-License-Identifier: AGPL-3.0-only
// Browser-only real WASM/WebCrypto/native peer interop; no Core business claims.
import init,{RecoverySessionClient} from '/wasm/rustsimhub_recovery_session.js';
import {openBoundSession,openStoredRecoverySession} from '/configurator/src/crypto/secure-session.js';
import {getOrCreateBinding,rememberGrant} from '/configurator/src/security/recovery-binding.js';
import {createRecoveryBackend,UUID} from '/configurator/src/production-backend.js';
import {fragments} from '/configurator/src/gatt-framing.js';
const assert=(ok,message)=>{if(!ok)throw Error(message);};
const hex=bytes=>Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');
const unhex=s=>Uint8Array.from(s.match(/../g)||[],v=>parseInt(v,16));
async function peer(message){const response=await fetch('/peer',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(message)});const value=await response.json();if(value.error)throw Error('synthetic native peer rejected');return value;}
async function keys(){const k=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);return {privateKey:k.privateKey,publicKey:hex(new Uint8Array(await crypto.subtle.exportKey('raw',k.publicKey)))};}
async function fixture({pinChange=null,privateChange=null,hook=null,purpose=1}={}){
 const identity=await keys(),trusted=await peer({op:'reset'});let step=0,applicationCalls=0,closed=0;
 const options={wasm:{RecoverySessionClient},devicePin:pinChange?pinChange(trusted.device_pin):trusted.device_pin,clientPublicKey:identity.publicKey,privateKey:privateChange||identity.privateKey,generation:'1',purpose,
  closeTransport:()=>closed++,exchange:async(kind,wire)=>{
   const op=kind===2?'request':['hello','finish_noise','client_binding'][step++];if(kind===2)applicationCalls++;
   const execute=async bytes=>unhex((await peer({op,wire:hex(bytes),now:0})).wire);
   return hook?hook({kind,op,wire,execute}):execute(wire);
  }};
 return {options,identity,get calls(){return applicationCalls;},get closed(){return closed;}};
}
async function rejects(action,message){let failed=false;try{await action();}catch{failed=true;}assert(failed,message);}
async function gattFixture({tamper=false}={}){
 const trusted=await peer({op:'reset'}),realm='11111111-1111-4111-8111-111111111111';
 let binding=await getOrCreateBinding(trusted.device_pin,realm);
 await rememberGrant(trusted.device_pin,realm,binding.revision,{device_pin:trusted.device_pin,realm,public_key:binding.public_key,grant:{id:crypto.randomUUID(),generation:'1',active:true}});
 let step=0,inputId=1,outputId=1,part=[],messages=0,selections=0,connects=0,disconnections=0;
 class Characteristic extends EventTarget {
  async startNotifications(){return this;}
  async writeValueWithResponse(frame){
   assert(frame.length<=20,'nonconservative ATT packet');const v=new DataView(frame.buffer,frame.byteOffset,frame.byteLength);
   assert(v.getUint16(4,true)===inputId&&v.getUint16(6,true)===part.length,'input ordering mismatch');part.push(frame.slice(12));
   if(part.length!==v.getUint16(8,true))return;
   const wire=new Uint8Array(part.reduce((n,b)=>n+b.length,0));let offset=0;for(const b of part){wire.set(b,offset);offset+=b.length;}
   assert(wire.length===v.getUint16(10,true),'input assembly length');part=[];inputId++;messages++;
   const kind=frame[3],op=kind===2?'request':['hello','finish_noise','client_binding'][step++];
   const reply=unhex((await peer({op,wire:hex(wire),now:0})).wire);
   if(tamper&&kind===2)reply[0]^=1;
   const destination=kind===1?auth:result;
   for(const bytes of fragments(kind===1?1:3,outputId++,reply)){
    destination.value=new DataView(bytes.buffer);destination.dispatchEvent(new Event('characteristicvaluechanged'));
   }
  }
 }
 const auth=new Characteristic(),command=new Characteristic(),result=new Characteristic();
 const service={getCharacteristic:async uuid=>{assert([UUID.auth,UUID.command,UUID.result].includes(uuid),'unexpected characteristic');return {[UUID.auth]:auth,[UUID.command]:command,[UUID.result]:result}[uuid];}};
 const device=new EventTarget();device.gatt={connect:async()=>{connects++;return {getPrimaryService:async uuid=>{assert(uuid===UUID.service,'unexpected service');return service;},disconnect(){disconnections++;}};},disconnect(){disconnections++;}};
 const bluetooth={requestDevice(options){selections++;assert(JSON.stringify(options)===JSON.stringify({filters:[{services:[UUID.service]}]}),'broad Bluetooth selection');return Promise.resolve(device);}};
 const backend=createRecoveryBackend({wasm:{RecoverySessionClient},realmForDevice:async pin=>{assert(pin===trusted.device_pin,'unexpected pin lookup');return realm;},bluetooth});
 return {backend,pin:trusted.device_pin,get messages(){return messages;},get selections(){return selections;},get connects(){return connects;},get disconnections(){return disconnections;}};
}
export async function runChecks(){
 await init({module_or_path:'/wasm/rustsimhub_recovery_session_bg.wasm'});
 const cases=[];const test=async(name,fn)=>{await fn();cases.push(name);};
 await test('actual WebCrypto nonexportable key; true native/WASM Unicode empty and maximum messages',async()=>{
  const f=await fixture();await rejects(()=>crypto.subtle.exportKey('pkcs8',f.identity.privateKey),'private key exported');
  const s=await openBoundSession(f.options);assert(s.cryptoBound,'not bound');
  for(const bytes of [new Uint8Array(),new TextEncoder().encode('合成跨端消息'),new Uint8Array(1024).fill(37)]){
   const out=await s.request(1,bytes);assert(hex(out)===hex(bytes),'native/WASM roundtrip mismatch');
  }assert(f.calls===3,'hidden IO/retry');s.close();await rejects(()=>s.request(1,new Uint8Array()),'closed source used');assert(f.closed===1,'close not idempotent');
 });
 await test('ordinary NKN purpose stays separate and carries encrypted normal-control records',async()=>{
  const f=await fixture({purpose:2});const s=await openBoundSession(f.options);const data=new TextEncoder().encode('synthetic normal auth request');assert(hex(await s.request(7,data))===hex(data),'normal purpose failed');s.close();
 });
 await test('wrong pinned device fails before any application message',async()=>{
  const f=await fixture({pinChange:pin=>pin.slice(0,62)+(pin.endsWith('00')?'01':'00')});await rejects(()=>openBoundSession(f.options),'wrong pin accepted');assert(f.calls===0&&f.closed===1,'pin failure did not close');
 });
 await test('tampered encrypted device binding fails before client authority is adopted',async()=>{
  const f=await fixture({hook:async({op,wire,execute})=>{const reply=await execute(wire);if(op==='finish_noise')reply[reply.length-1]^=1;return reply;}});await rejects(()=>openBoundSession(f.options),'binding tamper accepted');assert(f.calls===0,'application after bad binding');
 });
 await test('wrong actual Ed25519 private key cannot sign client binding',async()=>{
  const wrong=await keys(),f=await fixture({privateChange:wrong.privateKey});await rejects(()=>openBoundSession(f.options),'wrong signer accepted');assert(f.calls===0&&f.closed===1,'bad signer did not close');
 });
 await test('request tamper is rejected by native peer with no retry',async()=>{
  const f=await fixture({hook:async({kind,wire,execute})=>{const changed=new Uint8Array(wire);if(kind===2)changed[0]^=1;return execute(changed);}});const s=await openBoundSession(f.options);await rejects(()=>s.request(1,new Uint8Array([1])),'request tamper accepted');assert(f.calls===1&&!s.cryptoBound,'tamper retried/remained live');
 });
 await test('response tamper closes actual WASM cipher',async()=>{
  const f=await fixture({hook:async({kind,wire,execute})=>{const reply=await execute(wire);if(kind===2)reply[0]^=1;return reply;}});const s=await openBoundSession(f.options);await rejects(()=>s.request(1,new Uint8Array([1])),'response tamper accepted');assert(!s.cryptoBound&&f.calls===1,'response failure remained live');
 });
 await test('old response replay fails expected nonce and request association',async()=>{
  let first=null;const f=await fixture({hook:async({kind,wire,execute})=>{const reply=await execute(wire);if(kind!==2)return reply;if(first)return first;first=new Uint8Array(reply);return reply;}});const s=await openBoundSession(f.options);await s.request(1,new Uint8Array([1]));await rejects(()=>s.request(1,new Uint8Array([2])),'replay accepted');assert(f.calls===2&&!s.cryptoBound,'replay caused retry');
 });
 await test('reflected outbound ciphertext cannot be a device response',async()=>{
  const f=await fixture({hook:({kind,wire,execute})=>kind===2?new Uint8Array(wire):execute(wire)});const s=await openBoundSession(f.options);await rejects(()=>s.request(1,new Uint8Array([3])),'direction reflection accepted');assert(!s.cryptoBound,'reflection did not retire');
 });
 await test('foreign-session response rejected despite equal record positions',async()=>{
  let old;const a=await fixture({hook:async({kind,wire,execute})=>{const reply=await execute(wire);if(kind===2)old=new Uint8Array(reply);return reply;}});const one=await openBoundSession(a.options);await one.request(1,new Uint8Array([1]));one.close();
  const b=await fixture({hook:({kind,wire,execute})=>kind===2?old:execute(wire)});const two=await openBoundSession(b.options);await rejects(()=>two.request(1,new Uint8Array([1])),'foreign cipher accepted');assert(!two.cryptoBound,'foreign cipher stayed open');
 });
 await test('cancel after sending discards actual late ciphertext and never resubmits',async()=>{
  const f=await fixture({hook:async({kind,wire,execute})=>{const reply=await execute(wire);if(kind===2)await new Promise(r=>setTimeout(r,70));return reply;}});const s=await openBoundSession(f.options),abort=new AbortController();const request=s.request(1,new Uint8Array([7]),{signal:abort.signal});setTimeout(()=>abort.abort(),10);await rejects(()=>request,'cancelled late response accepted');assert(f.calls===1&&!s.cryptoBound,'cancel retried or revived');
 });
 await test('local body limit performs no application IO',async()=>{
  const f=await fixture(),s=await openBoundSession(f.options);await rejects(()=>s.request(1,new Uint8Array(1025)),'oversized input admitted');assert(f.calls===0,'overlimit sent');s.close();
 });
 await test('CSPRNG denial has no fallback or transport attempt',async()=>{
  const f=await fixture(),original=crypto.getRandomValues.bind(crypto);crypto.getRandomValues=()=>{throw Error('synthetic entropy denial');};try{await rejects(()=>openBoundSession(f.options),'entropy fallback');}finally{crypto.getRandomValues=original;}assert(f.calls===0&&f.closed===1,'entropy failure IO');
 });
 await test('monotonic clock rollback retires established session before sending',async()=>{
  const f=await fixture();let now=1000;const s=await openBoundSession({...f.options,clock:()=>now});now=999;await rejects(()=>s.request(1,new Uint8Array()),'clock rollback accepted');assert(f.calls===0&&!s.cryptoBound,'clock failure did not retire');
 });
 await test('real IndexedDB nonexportable binding signs through stored recovery adapter',async()=>{
  const trusted=await peer({op:'reset'}),realm='11111111-1111-4111-8111-111111111111';let binding=await getOrCreateBinding(trusted.device_pin,realm);
  binding=await rememberGrant(trusted.device_pin,realm,binding.revision,{device_pin:trusted.device_pin,realm,public_key:binding.public_key,grant:{id:'22222222-2222-4222-8222-222222222222',generation:'1',active:true}});
  let step=0;const s=await openStoredRecoverySession({wasm:{RecoverySessionClient},devicePin:trusted.device_pin,realm,exchange:async(kind,wire)=>unhex((await peer({op:kind===2?'request':['hello','finish_noise','client_binding'][step++],wire:hex(wire),now:0})).wire)});
  assert(hex(await s.request(1,new Uint8Array([8])))==='08','stored key interop failed');s.close();
  await rememberGrant(trusted.device_pin,realm,binding.revision,{device_pin:trusted.device_pin,realm,public_key:binding.public_key,grant:{id:binding.grant.id,generation:'2',active:false}});
  let calls=0;await rejects(()=>openStoredRecoverySession({wasm:{RecoverySessionClient},devicePin:trusted.device_pin,realm,exchange:()=>calls++}),'inactive local binding used');assert(calls===0,'inactive metadata caused IO');
 });
 await test('production Web Bluetooth factory uses actual stored key and bounded T5 fragments with native crypto',async()=>{
  const f=await gattFixture();const connection=f.backend.connectFromGesture({expectedPin:f.pin});assert(f.selections===1&&f.connects===0,'selection was not synchronous before await');await connection;
  assert(JSON.stringify(await f.backend.readStatus({}))==='{}','synthetic echo read mismatch');
  const id=crypto.randomUUID(),trial='a'.repeat(64);
  const begin=await f.backend.beginWifi({ssid:'合成 Wi-Fi',passphrase:'fixture-only-credential',requestId:id});assert(begin.ssid==='合成 Wi-Fi'&&!('request_id' in begin),'body broke original Core schema');
  const confirm=await f.backend.confirmWifi({trialId:trial,requestId:crypto.randomUUID()});assert(confirm.trial_id===trial&&Object.keys(confirm).length===1,'confirm schema');
  const query=await f.backend.queryOperation({targetRequestId:id});assert(query.target_request_id===id,'query changed original UUID');
  assert(f.messages===7,'unexpected handshake/application count or retry');await f.backend.disconnect();assert(f.disconnections===1,'transport not retired');await rejects(()=>f.backend.readStatus({}),'closed backend used');
 });
 await test('production Bluetooth adapter rejects actual tampered native ciphertext and does not retry',async()=>{
  const f=await gattFixture({tamper:true});await f.backend.connectFromGesture({expectedPin:f.pin});await rejects(()=>f.backend.readStatus({}),'tampered backend reply accepted');assert(f.messages===4&&f.disconnections===1,'tamper retried or transport remained open');
 });
 await test('late Bluetooth selection after disconnect never connects or creates a session',async()=>{
  let resolve,selected=false,connects=0,closed=0;
  const backend=createRecoveryBackend({wasm:{RecoverySessionClient},realmForDevice:()=>{throw Error('late realm lookup');},bluetooth:{requestDevice(){selected=true;return new Promise(r=>{resolve=r;});}}});
  const pending=backend.connectFromGesture({expectedPin:'a'.repeat(64)});assert(selected,'selection deferred');await backend.disconnect();resolve({gatt:{connect(){connects++;},disconnect(){closed++;}}});await rejects(()=>pending,'late selection revived');assert(connects===0&&closed===1,'late GATT operation');
 });
 return {passed:cases.length,cases,scope:'actual application crypto and production browser adapter; synthetic Bluetooth/native peer, no Core business or real device acceptance'};
}
globalThis.runRecoverySessionChecks=runChecks;
