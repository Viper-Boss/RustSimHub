// SPDX-License-Identifier: AGPL-3.0-only
// Test WebCrypto with the exact browser proof module; no synthetic verification shortcut.
import {webcrypto} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const moduleBytes=await readFile(new URL('../crates/edge/src/web/proof.js',import.meta.url));
const {prove}=await import('data:text/javascript;base64,'+moduleBytes.toString('base64'));
const device=await crypto.subtle.generateKey('Ed25519',false,['sign','verify']);
const client=await crypto.subtle.generateKey('Ed25519',false,['sign','verify']);
const pin=new Uint8Array(await crypto.subtle.exportKey('raw',device.publicKey));
const pub=new Uint8Array(await crypto.subtle.exportKey('raw',client.publicKey));
async function challenge(overrides={}) {
 const transcript=new TextEncoder().encode(JSON.stringify({domain:'RustSimHub.Auth.v1',purpose:'login',instance:'a'.repeat(64),challenge_id:'b'.repeat(64),device_public_key:Array.from(pin),client_public_key:Array.from(pub),kind:'client',scopes:11,generation:1,...overrides}));
 return {id:'b'.repeat(64),transcript:Array.from(transcript),device_signature:Array.from(new Uint8Array(await crypto.subtle.sign('Ed25519',device.privateKey,transcript))),expires_in_seconds:60};
}
const c=await challenge();const signature=await prove(c,pin,pub,client.privateKey,'login');
assert(await crypto.subtle.verify('Ed25519',client.publicKey,new Uint8Array(signature),new Uint8Array(c.transcript)));
for(const overrides of [{purpose:'bootstrap'},{kind:'owner'},{generation:0},{client_public_key:Array(32).fill(0)},{device_public_key:Array(32).fill(0)},{instance:'x'},{challenge_id:'c'.repeat(64)},{extra:'injected'},{scopes:1024}]) {
 await assert.rejects(prove(await challenge(overrides),pin,pub,client.privateKey,'login'));
}
await assert.rejects(prove({...c,device_signature:Array(64).fill(0)},pin,pub,client.privateKey,'login'));
const pairing=await challenge({purpose:'pair',generation:0,scopes:11});
await prove(pairing,pin,pub,client.privateKey,'pair',['status.read','sms.read','calls.read']);
await assert.rejects(prove(pairing,pin,pub,client.privateKey,'pair',['status.read','sms.send']));
console.log('Browser proof tests passed: transcript bytes, pin, purpose, principal, scope and generation');
const bootstrap=await challenge({purpose:'bootstrap',kind:'owner',scopes:1023,generation:0});
const ownerProof=await prove(bootstrap,pin,pub,client.privateKey,'bootstrap',undefined,'owner');
assert(await crypto.subtle.verify('Ed25519',client.publicKey,new Uint8Array(ownerProof),new Uint8Array(bootstrap.transcript)));
await assert.rejects(prove(bootstrap,pin,pub,client.privateKey,'bootstrap'));
for(const overrides of [{generation:1},{scopes:11},{kind:'client'},{purpose:'recover_owner'}]) {
 await assert.rejects(prove(await challenge({purpose:'bootstrap',kind:'owner',scopes:1023,generation:0,...overrides}),pin,pub,client.privateKey,'bootstrap',undefined,'owner'));
}
await prove(await challenge({kind:'owner',scopes:1023}),pin,pub,client.privateKey,'login',undefined,'owner');
console.log('Browser first-owner proof tests passed: signed owner role, full scope, bootstrap generation and purpose');
const sessionToken='e'.repeat(64),digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(sessionToken))));
const continuing=await challenge({purpose:'session_renew',session_digest:digest});
const continuingProof=await prove(continuing,pin,pub,client.privateKey,'session_renew',undefined,'client',sessionToken);
assert(await crypto.subtle.verify('Ed25519',client.publicKey,new Uint8Array(continuingProof),new Uint8Array(continuing.transcript)));
await assert.rejects(prove(continuing,pin,pub,client.privateKey,'login'));
await assert.rejects(prove(continuing,pin,pub,client.privateKey,'session_renew',undefined,'client','f'.repeat(64)));
for(const overrides of [{session_digest:Array(32).fill(0)},{generation:0},{purpose:'login'},{session_digest:undefined}]){
 await assert.rejects(prove(await challenge({purpose:'session_renew',session_digest:digest,...overrides}),pin,pub,client.privateKey,'session_renew',undefined,'client',sessionToken));
}
await assert.rejects(prove(await challenge({session_digest:digest}),pin,pub,client.privateKey,'login'));
console.log('Browser continuation proof passed: exact session digest, pin, scope, purpose and generation.');
