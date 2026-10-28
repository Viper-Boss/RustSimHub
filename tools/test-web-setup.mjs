// SPDX-License-Identifier: AGPL-3.0-only
// First-setup page against a mock device that issues Core-shaped transcripts and verifies the
// page's Ed25519 signatures with Node WebCrypto: create Owner, log in, list networks, wrong password.
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
const subtle=webcrypto.subtle,hex=b=>Buffer.from(b).toString('hex'),enc=new TextEncoder();
function documentStub(){
 const els=new Map();
 const make=id=>({id,hidden:false,disabled:false,value:'',type:'',checked:false,textContent:'',dataset:{},children:[],
  setAttribute(){},focus(){},append(c){this.children.push(c);},replaceChildren(){this.children=[];},onclick:null,onchange:null});
 return {els,getElementById:id=>{if(!els.has(id))els.set(id,make(id));return els.get(id);},createElement:()=>make('')};
}
async function run({password,ownerExists=false}){
 const device=await subtle.generateKey({name:'Ed25519'},true,['sign','verify']);
 const devicePk=new Uint8Array(await subtle.exportKey('raw',device.publicKey));
 let owner=ownerExists?'preset':null,pending=new Map(),log=[];
 async function verify(c,signature){
  const p=pending.get(c);pending.delete(c);if(!p)return false;
  const key=await subtle.importKey('raw',p.client,{name:'Ed25519'},false,['verify']);
  return subtle.verify('Ed25519',key,new Uint8Array(signature),p.transcript);
 }
 function challenge(client,purpose){
  const id=hex(webcrypto.getRandomValues(new Uint8Array(32)));
  const t={domain:'RustSimHub.Auth.v1',purpose,instance:'a'.repeat(64),challenge_id:id,device_public_key:Array.from(devicePk),client_public_key:client,kind:'owner',scopes:1023,generation:purpose==='bootstrap'?0:1};
  const transcript=enc.encode(JSON.stringify(t));pending.set(id,{client:new Uint8Array(client),transcript});
  return {id,expires_in_seconds:60,transcript:Array.from(transcript),device_signature:Array(64).fill(0)};
 }
 async function handle(cmd){
  log.push(cmd.op);
  switch(cmd.op){
   case 'identity':return {public_key:Array.from(devicePk),signature_algorithm:'ed25519',protocol:2,owner_setup_available:!owner};
   case 'setup_open_challenge':if(owner)return {error:'forbidden'};return challenge(cmd.public_key,'bootstrap');
   case 'setup_open_finish':{const ok=await verify(cmd.challenge_id,cmd.signature);if(!ok)return {error:'forbidden'};owner='created';return {ok:true,owner_initialized:true};}
   case 'challenge':return challenge(cmd.public_key,'login');
   case 'authenticate':{const p=pending.get(cmd.challenge_id);const ok=await verify(cmd.challenge_id,cmd.signature);
    if(!ok||!owner)return {error:'forbidden'};if(owner==='created')owner=hex(p.client);if(owner!==hex(p.client)&&owner!=='preset')return {error:'forbidden'};return {token:'b'.repeat(64),expires_in_seconds:900};}
   case 'wifi_networks':assert.equal(cmd.token,'b'.repeat(64));return {schema:1,source:'before_setup_ap',networks:[{ssid:'Home',strength:70,selectable:true},{ssid:'Open',strength:40,selectable:false}]};
   default:return {available:true,state:'idle'};
  }
 }
 const doc=documentStub();
 globalThis.document=doc;globalThis.localStorage={m:new Map(),getItem(k){return this.m.get(k)??null;},setItem(k,v){this.m.set(k,v);},removeItem(k){this.m.delete(k);}};
 globalThis.fetch=async(url,init)=>{assert.equal(url,'/api/v2');assert.equal(init.headers['X-RustSimHub-Request'],'2');const body=JSON.parse(init.body);
  const v=await handle(body.command);
  // Real Core envelope (errors inside result), so unwrapping bugs cannot hide.
  return {ok:true,json:async()=>({protocol:2,id:body.id,result:v})};};
 const mod=await import('../crates/edge/src/web/setup.js?'+Math.random());void mod;
 await new Promise(r=>setTimeout(r,50));
 const $=id=>doc.getElementById(id);
 $('setup-username').value=' Admin ';$('setup-password').value=password;$('setup-confirm').value=password;
 await $('setup-account-go').onclick();
 return {doc,$,log,owner};
}
// 1) Fresh device: create Owner (bootstrap signature verified), log in, list only selectable networks.
let r=await run({password:'correct horse'});
assert.deepEqual(r.log.slice(0,5),['identity','setup_open_challenge','setup_open_finish','challenge','authenticate']);
assert.equal(r.$('setup-wifi').hidden,false);
assert.equal(r.$('setup-networks').children.length,1);
assert.match(r.$('setup-networks-note').textContent,/开启热点前/);
// Refresh rebuilds the list instead of appending duplicates.
await r.$('setup-networks-refresh').onclick();await new Promise(x=>setTimeout(x,20));
assert.equal(r.$('setup-networks').children.length,1);
assert.equal(r.log.filter(o=>o==='wifi_networks').length,2);
assert.equal(r.$('setup-password').value,'');
// 2) Too-short password never reaches the device.
r=await run({password:'short'});
assert.deepEqual(r.log,['identity']);
assert.match(r.$('setup-notice').textContent,/8–64/);
// 3) Owner exists: login mode only (no open claim attempted).
r=await run({password:'correct horse',ownerExists:true});
assert.ok(!r.log.includes('setup_open_challenge'));
assert.equal(r.$('setup-confirm-label').hidden,true);
console.log('setup page 3/3 flows passed');
