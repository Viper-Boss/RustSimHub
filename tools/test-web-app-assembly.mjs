// SPDX-License-Identifier: AGPL-3.0-only
// Actual HTML/ESM/static routes/CSP, with a finite loopback external API only.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const web=new URL('../crates/edge/src/web/',import.meta.url);
const linux=await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url),'utf8');
const csp=linux.match(/\("content-security-policy","([^"]+)"\)/)?.[1];assert(csp,'Actual default CSP required');
const files=new Map([...linux.matchAll(/include_str!\("web\/([^"/]+)"\)/g)].map(m=>['/'+m[1],new URL(m[1],web)]));files.set('/',new URL('index.html',web));
const {subtle}=webcrypto,pair=await subtle.generateKey('Ed25519',true,['sign','verify']);
const device=Array.from(new Uint8Array(await subtle.exportKey('raw',pair.publicKey))),token='c'.repeat(64),instance='a'.repeat(64);
let calls=[],assets=[],failures=[],challenges=new Map(),sequence=0,mode='ready',held=null;
let permissionGeneration=0,permissionEnabled=false,permissionDisabled=0,permissionReceipts=new Map();
function permissionStatus(){return {format:1,available:true,core_instance:instance,generation:String(permissionGeneration),enabled:permissionEnabled,disclosure_version:permissionEnabled?1:null,disabled_generation:String(permissionDisabled),owner_current:permissionEnabled,record_exists:permissionGeneration>0,reporting_available:false,runtime:permissionEnabled?'authorized_not_connected':'disabled'};}
function report(){return {available:true,configuration_api:true,remote_control_supported:false,revision:'0',provider:{contract:1,core_instance:instance,owners:{manual:false,remote:false,statistics:false},supervisor_state:'ready',root_generation:1,original_process_exit_verified:false,status:{contract:1,source:'engine',generation:1,sequence:++sequence,age_ms:0,enabled:'on',service:'running',path:'unknown',dht:'unknown',rx_bytes:null,tx_bytes:null,rtt_ms:null,peer_count:null,last_error:null}}};}
async function handle(c){
 calls.push({op:c.op,...(c.request_id?{request_id:c.request_id}:{}),...(c.owner?{owner:c.owner,enabled:c.enabled}:{})});
 if(!['identity','challenge','authenticate'].includes(c.op))assert.equal(c.token,token);
 switch(c.op){
 case 'identity':return {public_key:device,owner_setup_available:false};
 case 'challenge':{
  assert(Array.isArray(c.public_key)&&c.public_key.length===32);
  const id=Buffer.from(webcrypto.getRandomValues(new Uint8Array(32))).toString('hex');
  const transcript=new TextEncoder().encode(JSON.stringify({domain:'RustSimHub.Auth.v1',purpose:'login',instance,challenge_id:id,device_public_key:device,client_public_key:c.public_key,kind:'owner',scopes:1023,generation:1}));
  challenges.set(id,{client:c.public_key,transcript});return {id,expires_in_seconds:60,transcript:Array.from(transcript),device_signature:Array.from(new Uint8Array(await subtle.sign('Ed25519',pair.privateKey,transcript)))};
 }
 case 'authenticate':{
  const proof=challenges.get(c.challenge_id);assert(proof);challenges.delete(c.challenge_id);
  const key=await subtle.importKey('raw',new Uint8Array(proof.client),'Ed25519',false,['verify']);assert(await subtle.verify('Ed25519',key,new Uint8Array(c.signature),proof.transcript));return {token,expires_in_seconds:900};
 }
 case 'nkn_status':return report();
 case 'usage_participation_status':return permissionStatus();
 case 'usage_participation_set':{
  assert.equal(c.expected_core_instance,instance);assert.equal(c.expected_generation,String(permissionGeneration));assert.equal(c.disclosure_version,c.enabled?1:null);
  permissionGeneration++;permissionEnabled=c.enabled;if(!c.enabled)permissionDisabled++;
  const result={request_id:c.request_id,known:true,state:'saved',core_instance:instance,current_core_instance:instance,enabled:c.enabled,disclosure_version:c.disclosure_version,expected_generation:c.expected_generation,generation:String(permissionGeneration),not_found_is_not_proof:true,reporting_available:false};permissionReceipts.set(c.request_id,result);return result;
 }
 case 'usage_participation_request':return permissionReceipts.get(c.request_id)||{request_id:c.request_id,known:false,current_core_instance:instance,not_found_is_not_proof:true,reporting_available:false};
 case 'usage_stats_query':assert(['24h','30d','90d'].includes(c.window));{const now=Math.floor(Date.now()/1000);return {status:'success',entry:{window:c.window,count:42,source:'read_only_transport',observed_at_secs:now,succeeded_at_secs:now,expires_at_secs:now+600,freshness_revoked:false}};}
 case 'nkn_request':assert.equal(c.expected_core_instance,instance);return {request_id:c.request_id,known:false,core_instance:instance,not_found_is_not_proof:true};
 case 'configuration_indicator':assert.equal(c.active,false);return {ok:true};
 case 'line_status':return {status:'ready',line:7};
 case 'system_status':return {schema:1,firmware_version:'0.1.0',model:null,kernel:null,os:null,cpu_cores:null,cpu_busy_ticks:null,cpu_total_ticks:null,load_1m:null,memory_total_bytes:null,memory_available_bytes:null,storage_total_bytes:null,storage_free_bytes:null,temperature_c:null,uptime_seconds:null};
 case 'call_status':return {available:true,active_calls:0,live_calls:[]};
 case 'inbox':return {messages:[],cursor:null};
 case 'inbox_recent':return {messages:[],cursor:null,total:0};
 case 'calls_recent':return {calls:[],cursor:null,total:0};
 case 'calls':return {calls:[],cursor:null};
 case 'health':return {history_read_api:false,wifi_provisioning_api:false};
 case 'network_status':case 'network_diagnostics':case 'sms_management_status':return {available:false};
 default:throw Error('Unexpected assembly fixture operation: '+c.op);
 }
}
const json=(res,value)=>{res.writeHead(200,{'Content-Type':'application/json','Cache-Control':'no-store'});res.end(JSON.stringify(value));};
const server=createServer(async(req,res)=>{
 try{
  const path=new URL(req.url,'http://127.0.0.1').pathname;
  if(req.method==='GET'&&files.has(path)){
   assets.push(path);res.writeHead(200,{'Content-Type':path.endsWith('.js')?'text/javascript':path.endsWith('.css')?'text/css':'text/html','Content-Security-Policy':csp,'Cache-Control':'no-store','X-Content-Type-Options':'nosniff'});res.end(await readFile(files.get(path)));return;
  }
  if(req.method==='GET'&&path==='/api/media/capabilities'){json(res,{protocol:1,enabled:false});return;}
  if(req.method==='GET'&&path==='/fixture'){json(res,{calls,assets,failures,csp,device,held:!!held});return;}
  if(req.method!=='POST'){res.writeHead(404);res.end();return;}
  let raw='';for await(const part of req){raw+=part;assert(raw.length<=65536);}
  if(path==='/fixture'){
   const value=JSON.parse(raw);mode=value.mode||'ready';
   if(value.reset){calls=[];assets=[];failures=[];sequence=0;challenges=new Map();permissionGeneration=0;permissionEnabled=false;permissionDisabled=0;permissionReceipts=new Map();}
   if(held){const release=held;held=null;release();}json(res,{ok:true});return;
  }
  assert.equal(path,'/api/v2');assert.equal(req.headers['x-rustsimhub-request'],'2');
  const body=JSON.parse(raw);assert.equal(body.protocol,2);assert.equal(typeof body.id,'string');
  const result=await handle(body.command);
  if(body.command.op==='nkn_status'&&mode==='offline'){res.writeHead(503);res.end();return;}
  if(body.command.op==='nkn_status'&&mode==='held')await new Promise(resolve=>{held=resolve;});
  if(!res.destroyed)json(res,{protocol:2,id:body.id,result});
 }catch(error){failures.push(error.message);res.writeHead(500);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
try{
 const child=spawn(process.env.WEB3_WEB_BROWSER_PYTHON||'python3',[fileURLToPath(new URL('./fixtures/web3-web/assembly-browser.py',import.meta.url)),`http://127.0.0.1:${server.address().port}`],{stdio:'inherit',env:process.env});
 const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);});assert.equal(code,0,'Real homepage assembly check failed');assert.deepEqual(failures,[]);
}finally{if(held)held();await new Promise(resolve=>server.close(resolve));}
