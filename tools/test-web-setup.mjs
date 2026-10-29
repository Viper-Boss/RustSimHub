// SPDX-License-Identifier: AGPL-3.0-only
// First-setup page against a mock device that issues Core-shaped transcripts and verifies the
// page's Ed25519 signatures with Node WebCrypto: create Owner, log in, list networks, wrong password.
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
const subtle=webcrypto.subtle,hex=b=>Buffer.from(b).toString('hex'),enc=new TextEncoder();
function documentStub(){
 const els=new Map();
 const make=id=>({id,hidden:false,disabled:false,value:'',type:'',checked:false,textContent:'',dataset:{},children:[],
  setAttribute(){},focus(){},append(c){this.children.push(c);},replaceChildren(...children){this.children=children;},onclick:null,onchange:null});
 return {els,getElementById:id=>{if(!els.has(id))els.set(id,make(id));return els.get(id);},createElement:()=>make('')};
}
async function run({password,ownerExists=false,catalogue=null,retainedRecord=null,scopedRecord=null,legacyRecord=null}){
 const device=await subtle.generateKey({name:'Ed25519'},true,['sign','verify']);
 const devicePk=new Uint8Array(await subtle.exportKey('raw',device.publicKey));
 const prior=scopedRecord||retainedRecord;
 let owner=ownerExists?'preset':null,pending=new Map(),log=[],parentHash=prior?.request_key_sha256??null,trialId=prior?.trials.at(-1)??null,commitId=null;
 const operationId='11111111-1111-4111-8111-111111111111';
 const parent=()=>({known:true,available:true,state:'ready',operation_id:operationId,request_key_sha256:parentHash,station_trial_api:true,station_save_api:true,credentials_returned:false,cellular_activation:false});
 const trial=()=>({known:true,operation_id:operationId,trial_id:trialId,state:'ready',authoritative_result:true,ap_return_verified:true,requires_confirmation:true,settings_saved:false,credentials_returned:false,cellular_activation:false});
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
   case 'wifi_networks':assert.equal(cmd.token,'b'.repeat(64));return catalogue||{schema:1,source:'before_setup_ap',networks:[{ssid:'Home',strength:70,selectable:true},{ssid:'Open',strength:40,selectable:false}]};
   case 'setup_ap_status':return {available:true,state:'ready',first_setup:true,owner_adoption_required:true,operation_id:operationId,station_trial_api:true,station_save_api:true,credentials_returned:false,cellular_activation:false};
   case 'setup_ap_start':parentHash=hex(await subtle.digest('SHA-256',enc.encode(cmd.idempotency_key)));return parent();
   case 'setup_ap_operation_status':assert.equal(cmd.request_key_sha256,parentHash);return parent();
   case 'setup_ap_wifi_begin':assert.equal(cmd.operation_id,operationId);assert.equal(cmd.ssid,'Home');assert.equal(cmd.passphrase,'wifi-password');trialId=cmd.trial_id;return trial();
   case 'setup_ap_wifi_status':assert.equal(cmd.trial_id,trialId);return trial();
   case 'setup_ap_wifi_confirm':assert.equal(cmd.trial_id,trialId);commitId=cmd.commit_id;return {known:true,operation_id:operationId,trial_id:trialId,commit_id:commitId,state:'saved',settings_saved:true,authoritative_result:true,credentials_returned:false,cellular_activation:false,not_resubmitted:true};
   default:throw Error('Unexpected first-setup command '+cmd.op);
  }
 }
 const doc=documentStub();doc.getElementById('setup-networks').append({value:'',textContent:'手动输入'});
 globalThis.document=doc;globalThis.localStorage={m:new Map(),getItem(k){return this.m.get(k)??null;},setItem(k,v){this.m.set(k,v);},removeItem(k){this.m.delete(k);}};
 if(retainedRecord)globalThis.localStorage.setItem('rustsimhub-setup-ap-wifi-pending',JSON.stringify(retainedRecord));
 if(legacyRecord)globalThis.localStorage.setItem('rustsimhub-setup-ap-wifi-pending',legacyRecord);
 if(scopedRecord)globalThis.localStorage.setItem('rustsimhub-setup-ap-wifi-pending:'+hex(devicePk),JSON.stringify(scopedRecord));
 globalThis.fetch=async(url,init)=>{assert.equal(url,'/api/v2');assert.equal(init.headers['X-RustSimHub-Request'],'2');const body=JSON.parse(init.body);
  const v=await handle(body.command);
  // Real Core envelope (errors inside result), so unwrapping bugs cannot hide.
  return {ok:true,json:async()=>({protocol:2,id:body.id,result:v})};};
 const mod=await import('../crates/edge/src/web/setup.js?'+Math.random());void mod;
 await new Promise(r=>setTimeout(r,50));
 const $=id=>doc.getElementById(id);
 $('setup-username').value=' Admin ';$('setup-password').value=password;$('setup-confirm').value=password;
 await $('setup-account-go').onclick();
 return {doc,$,log,owner,devicePk};
}
// 1) Fresh device: create Owner (bootstrap signature verified), log in, list only selectable networks.
let r=await run({password:'correct horse'});
assert.deepEqual(r.log.slice(0,5),['identity','setup_open_challenge','setup_open_finish','challenge','authenticate']);
assert.equal(r.$('setup-wifi').hidden,false);
assert.doesNotMatch(r.$('ap-wifi-state').textContent,/其他测试|通话/,'completed account setup must release its own busy guard before opening the Wi-Fi controller');
assert.equal(r.$('setup-networks').children.length,2);
assert.match(r.$('setup-networks-note').textContent,/开启热点前/);
// Refresh rebuilds the list instead of appending duplicates.
await r.$('setup-networks-refresh').onclick();await new Promise(x=>setTimeout(x,20));
assert.equal(r.$('setup-networks').children.length,2);
assert.equal(r.log.filter(o=>o==='wifi_networks').length,2);
assert.equal(r.$('setup-password').value,'');
// Complete the real shared AP controller's adoption -> trial -> explicit save path.
r.$('ap-wifi-ssid').value='Home';r.$('ap-wifi-passphrase').value='wifi-password';
await r.$('ap-wifi-begin').onclick();
assert.equal(r.log.filter(o=>o==='setup_ap_start').length,1);
assert.equal(r.log.filter(o=>o==='setup_ap_wifi_begin').length,1);
assert.equal(r.$('ap-wifi-state').dataset.phase,'trial_ready');
const scopedKey='rustsimhub-setup-ap-wifi-pending:'+hex(r.devicePk);
const retainedTrial=JSON.parse(globalThis.localStorage.getItem(scopedKey));
await r.$('ap-wifi-confirm').onclick();
assert.equal(r.log.filter(o=>o==='setup_ap_wifi_confirm').length,1);
assert.equal(r.$('ap-wifi-state').dataset.phase,'saved');
assert(![...globalThis.localStorage.m.values()].some(value=>value.includes('wifi-password')));

// Logging back into a pending trial must query it after login, not let its own
// account busy flag falsely report another test/call and leave the panel stuck.
r=await run({password:'correct horse',ownerExists:true,retainedRecord:retainedTrial});
assert.equal(r.$('ap-wifi-state').dataset.phase,'trial_ready');
assert.doesNotMatch(r.$('ap-wifi-state').textContent,/其他测试|通话/);
assert.equal(r.log.filter(op=>op==='setup_ap_wifi_status').length,1);
assert(r.log.every(op=>!['setup_ap_start','setup_ap_wifi_begin','setup_ap_wifi_confirm'].includes(op)));

// A fresh, signed Owner claim on a new device must keep an earlier unscoped
// browser operation without letting it block or be replayed for the new device.
const legacy=JSON.stringify(retainedTrial);
r=await run({password:'correct horse',legacyRecord:legacy});
assert.equal(globalThis.localStorage.getItem('rustsimhub-setup-ap-wifi-pending'),legacy);
const newKey='rustsimhub-setup-ap-wifi-pending:'+hex(r.devicePk);
assert.equal(globalThis.localStorage.getItem(newKey),'null');
assert.equal(r.$('ap-wifi-state').dataset.phase,'idle');
assert(!r.log.includes('setup_ap_wifi_status'));
r.$('ap-wifi-ssid').value='Home';r.$('ap-wifi-passphrase').value='wifi-password';
await r.$('ap-wifi-begin').onclick();
assert.equal(r.$('ap-wifi-state').dataset.phase,'trial_ready');
assert.equal(globalThis.localStorage.getItem('rustsimhub-setup-ap-wifi-pending'),legacy);
assert.equal(JSON.parse(globalThis.localStorage.getItem(newKey)).trials.length,1);
assert.equal(r.log.filter(op=>op==='setup_ap_wifi_begin').length,1);

// A returning Owner's scoped record takes priority over an unrelated legacy
// record. Automatic recovery remains read-only and keeps both stored records.
const foreignLegacy=JSON.stringify({...retainedTrial,request_key_sha256:'c'.repeat(64),trials:['22222222-2222-4222-8222-222222222222']});
r=await run({password:'correct horse',ownerExists:true,scopedRecord:retainedTrial,legacyRecord:foreignLegacy});
assert.equal(r.$('ap-wifi-state').dataset.phase,'trial_ready');
assert.equal(globalThis.localStorage.getItem('rustsimhub-setup-ap-wifi-pending'),foreignLegacy);
assert.deepEqual(JSON.parse(globalThis.localStorage.getItem('rustsimhub-setup-ap-wifi-pending:'+hex(r.devicePk))),retainedTrial);
assert.deepEqual(r.log.filter(op=>op.startsWith('setup_ap_')),['setup_ap_wifi_status']);

// 2) Too-short password never reaches the device.
r=await run({password:'short'});
assert.deepEqual(r.log,['identity']);
assert.match(r.$('setup-notice').textContent,/8–64/);
// 3) Owner exists: login mode only (no open claim attempted).
r=await run({password:'correct horse',ownerExists:true});
assert.ok(!r.log.includes('setup_open_challenge'));
assert.equal(r.$('setup-confirm-label').hidden,true);
// Empty *selectable* choices must be explained even when the raw AP cache is nonempty.
r=await run({password:'correct horse',catalogue:{source:'networkmanager_cache',networks:[{ssid:'Open',selectable:false}]}});
assert.equal(r.$('setup-networks').children.length,1);
assert.match(r.$('setup-networks-note').textContent,/没有可选的 WPA2/);
r=await run({password:'correct horse',catalogue:{source:'before_setup_ap',snapshot_age_seconds:720,networks:[{ssid:'Home',strength:70,selectable:true}]}});
assert.match(r.$('setup-networks-note').textContent,/12分钟前的缓存/);
r=await run({password:'correct horse',catalogue:{source:'before_setup_ap_scan',snapshot_age_seconds:4,networks:[{ssid:'Fresh',strength:81,selectable:true}]}});
assert.match(r.$('setup-networks-note').textContent,/已完成一次主动扫描，约4秒前完成/);
assert.match(r.$('setup-networks-note').textContent,/不会重新扫描/);
assert.equal(r.$('setup-networks').children[1].value,'Fresh');
assert.deepEqual(r.log.filter(op=>op.startsWith('setup_ap_')),[]);
r=await run({password:'correct horse',catalogue:{source:'before_setup_ap_scan',snapshot_age_seconds:4,networks:[]}});
assert.equal(r.$('setup-networks').children.length,1);
assert.match(r.$('setup-networks-note').textContent,/没有可选的 WPA2/);
console.log('setup page account/adoption/trial/save and empty/aged catalogue checks passed');
