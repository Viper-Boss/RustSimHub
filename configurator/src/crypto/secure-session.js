// SPDX-License-Identifier: AGPL-3.0-only
// Actual WASM protocol; browser signing keys remain non-exportable WebCrypto
// handles. The resulting cipher is not a Core/Owner authorization capability.
import {readBinding,loadSigningKey} from '../security/recovery-binding.js';
const fail=code=>{throw Object.assign(new Error(code),{code});};
const raw=s=>{
 if(typeof s!=='string'||!/^[0-9a-f]{64}$/.test(s))fail('invalid_input');
 return Uint8Array.from(s.match(/../g),s=>parseInt(s,16));
};
const uuid=s=>{
 if(typeof s!=='string'||!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(s))fail('invalid_input');
 return Uint8Array.from(s.replaceAll('-','').match(/../g),s=>parseInt(s,16));
};
const stamp=clock=>{const n=clock();if(!Number.isSafeInteger(Math.floor(n))||n<0)fail('deadline');return BigInt(Math.floor(n));};
const keyValid=key=>typeof CryptoKey==='function'&&key instanceof CryptoKey&&key.type==='private'&&!key.extractable&&key.algorithm.name==='Ed25519'&&key.usages.join(',')==='sign';

// exchange(kind,wire,{signal}) is a finite already-owned transport. This module
// accepts no host/URL/key path and creates no socket or Bluetooth selection.
export async function openBoundSession({wasm,devicePin,clientPublicKey,privateKey,generation,purpose=1,exchange,closeTransport=()=>{},signal,clock=()=>performance.now()}){
 if(!wasm?.RecoverySessionClient||typeof exchange!=='function'||!keyValid(privateKey)||![1,2].includes(purpose))fail('unsupported');
 if(typeof generation!=='string'||! /^[1-9][0-9]{0,18}$/.test(generation)||BigInt(generation)>=9223372036854775807n)fail('not_preapproved');
 const device=raw(devicePin),client=raw(clientPublicKey),nonce=new Uint8Array(32);
 let inner=null,closed=false,busy=false,last=stamp(clock),signer=privateKey;
 const cancellation=new AbortController();
 const close=()=>{if(closed)return;closed=true;cancellation.abort();try{inner?.close();}finally{inner?.free();inner=null;signer=null;closeTransport();}};
 const check=()=>{if(closed||signal?.aborted)fail('transport_lost');let n;try{n=stamp(clock);}catch{close();fail('deadline');}if(n<last){close();fail('deadline');}last=n;return n;};
 const abort=()=>close();signal?.addEventListener('abort',abort,{once:true});
 const bounded=async(operation,deadline)=>{
  let timer;
  try{
   const left=deadline-check();if(left<=0n)fail('deadline');
   const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(Object.assign(Error('deadline'),{code:'deadline'}));close();},Number(left));});
   const result=await Promise.race([Promise.resolve().then(operation),timeout]);
   if(check()>=deadline)fail('deadline');
   return result;
  }finally{clearTimeout(timer);}
 };
 const exchangeAt=(kind,wire,deadline)=>bounded(async()=>{
  if(!(wire instanceof Uint8Array)||wire.byteLength>2048)fail('protocol_error');
  const reply=await exchange(kind,wire,{signal:cancellation.signal});
  if(!(reply instanceof Uint8Array)||reply.byteLength>2048)fail('protocol_error');
  return reply;
 },deadline);
 try{
  check();crypto.getRandomValues(nonce);
  inner=new wasm.RecoverySessionClient(device,client,purpose,BigInt(generation),nonce,last,last+300000n);
  const deadline=last+10000n;
  const two=await exchangeAt(1,inner.write_client_hello(check()),deadline);
  inner.read_handshake(two,check());
  const deviceBinding=await exchangeAt(1,inner.write_handshake(check()),deadline);
  inner.open_device_binding(deviceBinding,check());
  const message=inner.binding_message(check());
  const signature=new Uint8Array(await bounded(()=>crypto.subtle.sign({name:'Ed25519'},signer,message),deadline));
  const ack=await exchangeAt(1,inner.seal_client_binding(signature,check()),deadline);
  inner.open_binding_ack(ack,check());
  if(!inner.is_bound())fail('auth_failed');
 }catch(error){close();signal?.removeEventListener('abort',abort);throw Object.assign(Error('auth_failed'),{code:error?.code==='deadline'?'deadline':'auth_failed'});}
 return Object.freeze({
  get cryptoBound(){return !closed&&inner.is_bound();},
  async request(action,body,{requestId=crypto.randomUUID(),signal:requestSignal}={}){
   if(busy)fail('busy');check();if(!(body instanceof Uint8Array)||body.byteLength>1024)fail('invalid_input');
   busy=true;const requestAbort=()=>close();requestSignal?.addEventListener('abort',requestAbort,{once:true});
   const owned=new Uint8Array(body);
   try{
    if(requestSignal?.aborted)fail('transport_lost');
    const deadline=check()+3000n;
    const wire=inner.seal_request(uuid(requestId),action,owned,check());
    const reply=await exchangeAt(2,wire,deadline);
    return inner.open_response(reply,check());
   }catch(error){close();throw Object.assign(Error('unknown'),{code:error?.code==='deadline'?'deadline':'unknown'});}
   finally{owned.fill(0);busy=false;requestSignal?.removeEventListener('abort',requestAbort);}
  },
  close(){signal?.removeEventListener('abort',abort);close();},
 });
}

export async function openStoredRecoverySession({wasm,devicePin,realm,exchange,closeTransport,signal,clock}){
 const binding=await readBinding(devicePin,realm);
 if(!binding?.grant?.active)fail('not_preapproved');
 const privateKey=await loadSigningKey(devicePin,realm,binding.revision);
 return openBoundSession({wasm,devicePin,clientPublicKey:binding.public_key,privateKey,generation:binding.grant.generation,purpose:1,exchange,closeTransport,signal,clock});
}
