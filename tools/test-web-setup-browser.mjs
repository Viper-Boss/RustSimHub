// SPDX-License-Identifier: AGPL-3.0-only
// Loopback-only external device fixture. Serves the actual first-setup assets;
// verifies real browser Ed25519 proofs, without copying the AP controller.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {spawn} from 'node:child_process';
const {subtle}=webcrypto,hex=b=>Buffer.from(b).toString('hex');
const key=await subtle.generateKey({name:'Ed25519'},true,['sign','verify']);
const device=Array.from(new Uint8Array(await subtle.exportKey('raw',key.publicKey)));
const OP='11111111-1111-4111-8111-111111111111',token='b'.repeat(64);
let mode='ready',owner=null,challenges=new Map(),parentHash=null,trial=null,commit=null,scans=new Map(),calls=[],heldReply=null;
const assets=new Map(['/setup.html','/setup.js','/ap-wifi.js','/password.js','/style.css'].map(p=>[p,new URL('../crates/edge/src/web'+p,import.meta.url)]));
const cap=()=>mode==='unsupported'?{supported:false,available:false}:mode==='cooldown'?{supported:true,available:false,remaining:7,max_per_parent:8,refusal:'cooldown',cooldown_remaining_ms:2000}:{supported:true,available:true,remaining:8-scans.size,max_per_parent:8,refusal:null};
const parent=()=>({known:true,available:true,state:'ready',operation_id:OP,request_key_sha256:parentHash,station_trial_api:true,station_save_api:true,credentials_returned:false,cellular_activation:false,expires_in_seconds:120,scan:cap()});
const trialView=()=>({known:true,operation_id:OP,trial_id:trial.id,state:trial.ok?'ready':'failed',authoritative_result:true,ap_return_verified:true,requires_confirmation:trial.ok,settings_saved:false,credentials_returned:false,cellular_activation:false});
const commitView=()=>({known:true,operation_id:OP,trial_id:trial.id,commit_id:commit,state:'saved',authoritative_result:true,settings_saved:true,credentials_returned:false,cellular_activation:false,not_resubmitted:true});
function scanView(id){
 const cancelled=scans.get(id)?.cancelled;
 const unknown=mode==='restore-unknown'&&!cancelled;
 return {known:true,operation_id:OP,scan_id:id,state:cancelled?'cancelled':unknown?'unknown':mode==='refused'?'refused':'ready',authoritative_result:!unknown,ap_return_verified:!unknown&&mode!=='refused',historical:false,execution_started:mode!=='refused',refusal:mode==='refused'?'conflict':null,cancel_requested:!!cancelled,settings_saved:false,credentials_returned:false,cellular_activation:false,scan:cap(),parent_expires_in_seconds:120,snapshot:cancelled||unknown||mode==='refused'?null:{requested_uptime_ms:10,last_scan_uptime_ms:11,completed_uptime_ms:12,networks:mode==='empty'?[]:[{ssid:'Fresh <synthetic>',strength:90,frequency_mhz:2412,security:'wpa2_personal'}],omitted:0,truncated:false}};
}
function challenge(client,purpose){
 const id=hex(webcrypto.getRandomValues(new Uint8Array(32)));
 const transcript=new TextEncoder().encode(JSON.stringify({domain:'RustSimHub.Auth.v1',purpose,instance:'a'.repeat(64),challenge_id:id,device_public_key:device,client_public_key:client,kind:'owner',scopes:1023,generation:purpose==='bootstrap'?0:1}));
 challenges.set(id,{client,transcript,purpose});return {id,expires_in_seconds:60,transcript:Array.from(transcript),device_signature:Array(64).fill(0)};
}
async function proof(c,purpose){
 const p=challenges.get(c.challenge_id);challenges.delete(c.challenge_id);assert(p&&p.purpose===purpose);
 const k=await subtle.importKey('raw',new Uint8Array(p.client),'Ed25519',false,['verify']);
 assert(await subtle.verify('Ed25519',k,new Uint8Array(c.signature),p.transcript));return hex(p.client);
}
async function command(c){
 calls.push({op:c.op,...(c.scan_id?{scan_id:c.scan_id}:{}),...(c.trial_id?{trial_id:c.trial_id}:{}),...(c.commit_id?{commit_id:c.commit_id}:{})});
 if(!['identity','setup_open_challenge','setup_open_finish','challenge','authenticate'].includes(c.op))assert.equal(c.token,token);
 switch(c.op){
 case 'identity':return {public_key:device,owner_setup_available:!owner};
 case 'setup_open_challenge':assert(!owner);return challenge(c.public_key,'bootstrap');
 case 'setup_open_finish':owner=await proof(c,'bootstrap');return {ok:true,owner_initialized:true};
 case 'challenge':return challenge(c.public_key,'login');
 case 'authenticate':assert.equal(await proof(c,'login'),owner);return {token,expires_in_seconds:900};
 case 'wifi_networks':return {source:'before_setup_ap',snapshot_age_seconds:60,networks:[{ssid:'Home',strength:70,selectable:true}]};
 case 'setup_ap_status':return {state:'ready',available:true,first_setup:true,owner_adoption_required:true,operation_id:OP,station_trial_api:true,station_save_api:true,credentials_returned:false,cellular_activation:false};
 case 'setup_ap_start':assert(!parentHash);parentHash=hex(await subtle.digest('SHA-256',new TextEncoder().encode(c.idempotency_key)));return parent();
 case 'setup_ap_operation_status':assert.equal(c.request_key_sha256,parentHash);return parent();
 case 'setup_ap_wifi_begin':assert.equal(c.operation_id,OP);trial={id:c.trial_id,ok:c.passphrase==='wifi-password'};return trialView();
 case 'setup_ap_wifi_status':assert.equal(c.trial_id,trial.id);return trialView();
 case 'setup_ap_wifi_confirm':assert(trial.ok&&c.trial_id===trial.id&&c.passphrase==='wifi-password');commit=c.commit_id;return commitView();
 case 'setup_ap_wifi_commit_status':assert.equal(c.commit_id,commit);return commitView();
 case 'setup_ap_scan_begin':assert.equal(c.operation_id,OP);if(!scans.has(c.scan_id))scans.set(c.scan_id,{cancelled:false});return scanView(c.scan_id);
 case 'setup_ap_scan_status':assert(scans.has(c.scan_id));return scanView(c.scan_id);
 case 'setup_ap_scan_cancel':assert(scans.has(c.scan_id));scans.get(c.scan_id).cancelled=true;return scanView(c.scan_id);
 default:throw Error('Unexpected fixture command');
 }
}
const json=(res,value)=>{res.writeHead(200,{'Content-Type':'application/json','Cache-Control':'no-store'});res.end(JSON.stringify(value));};
const server=createServer(async(req,res)=>{
 try{
  const path=new URL(req.url,'http://127.0.0.1').pathname;
  if(req.method==='GET'&&assets.has(path)){res.writeHead(200,{'Content-Type':path.endsWith('.js')?'text/javascript':path.endsWith('.css')?'text/css':'text/html','Cache-Control':'no-store'});res.end(await readFile(assets.get(path)));return;}
  if(req.method==='GET'&&!assets.has(path)&&path!=='/fixture'){res.writeHead(404);res.end();return;}
  if(path==='/fixture'&&req.method==='GET'){json(res,{mode,calls,scanCount:scans.size});return;}
  let raw='';for await(const part of req){raw+=part;assert(raw.length<=16384);}
  if(path==='/fixture'&&req.method==='POST'){
   const update=JSON.parse(raw);mode=update.mode||'ready';
   if(update.reset){owner=null;parentHash=null;trial=null;commit=null;scans=new Map();calls=[];challenges=new Map();}
   if(heldReply){const release=heldReply;heldReply=null;release();}
   json(res,{ok:true});return;
  }
  assert.equal(path,'/api/v2');const body=JSON.parse(raw);assert.equal(body.protocol,2);
  const result=await command(body.command);
  // Consume Begin but lose its usable receipt. Complete HTTP to avoid Chromium's
  // transport-level retry of a socket closed before response headers; the Core
  // fixture also keeps the real API's same-ID idempotency.
  if(body.command.op==='setup_ap_scan_begin'&&['lost','offline','restore-unknown'].includes(mode)){res.writeHead(200,{'Content-Type':'application/json'});res.end('{');return;}
  if(body.command.op==='setup_ap_scan_status'&&mode==='offline'){res.writeHead(503);res.end();return;}
  if(body.command.op==='setup_ap_scan_begin'&&mode==='held')await new Promise(resolve=>{heldReply=resolve;});
  json(res,{protocol:2,id:body.id,result});
 }catch(error){console.error('Fixture assertion:',error.message);res.writeHead(500);res.end('fixture rejected');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const python=process.env.WEB3_AP_BROWSER_PYTHON||'python3';
try{
 const child=spawn(python,[new URL('./test-web-setup-browser.py',import.meta.url).pathname,`http://127.0.0.1:${server.address().port}`],{stdio:'inherit',env:process.env});
 const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);});assert.equal(code,0,'real-browser regression failed');
}finally{await new Promise(r=>server.close(r));}
