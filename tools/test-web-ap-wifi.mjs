// SPDX-License-Identifier: AGPL-3.0-only
// Actual controller, metadata CAS and strict Core-shaped responses. No device/network.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const bytes=await readFile(new URL('../crates/edge/src/web/ap-wifi.js',import.meta.url));
const {ApConfigurator,pendingInput,credentialsInput,refusals}=await import('data:text/javascript;base64,'+bytes.toString('base64'));
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const routes=await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url),'utf8');
const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]);assert.equal(new Set(ids).size,ids.length);
for(const m of bytes.toString().matchAll(/\$\('(ap-wifi-[^']+)'\)/g))assert(ids.includes(m[1]),m[1]);
assert(app.includes("import { ApConfigurator, pendingInput as apPendingInput } from './ap-wifi.js'"));
assert(routes.includes('"/ap-wifi.js"')&&routes.includes('include_str!("web/ap-wifi.js")'));
assert(app.includes("atomicStored('ap-wifi-pending'")&&app.includes('old?.revision')&&app.includes('apConfigurator?.suspend()'));
const OP='11111111-1111-4111-8111-111111111111',HASH='a'.repeat(64),SSID='合成 <wifi>',PASS='synthetic-password';
const clone=v=>structuredClone(v),turn=()=>new Promise(r=>setImmediate(r));
class Element{
 constructor(){this.value='';this.textContent='';this.hidden=false;this.disabled=false;this.readOnly=false;this.dataset={};this.attributes={};this.checked=false;this.type='password';}
 setAttribute(k,v){this.attributes[k]=v;}
 set innerHTML(_){throw Error('Never parse untrusted content as HTML');}
}
async function fixture({record=null,hook=null,loginHook=null,storageError=false,blocked=false,created=false,bootstrap=false,archiveError=false,scanning=false,retainedScan=null,scanCapability=null,scanWriteError=false,approveScan=true}={}){
 const elements=new Map(ids.map(id=>[id,new Element()])),calls=[],archives=[];let storage=clone(record),authenticated=false,now=0,phase='ready',failWrite=false;
 let scanStore=clone(retainedScan);const snapshots=[],scanButton=new Element(),scanNote=new Element();
 const scanReply=(c,extra={})=>({known:true,operation_id:OP,scan_id:c.scan_id,state:'ready',authoritative_result:true,ap_return_verified:true,historical:false,credentials_returned:false,cellular_activation:false,settings_saved:false,snapshot:{requested_uptime_ms:100,last_scan_uptime_ms:101,completed_uptime_ms:102,networks:[],omitted:0,truncated:false},...extra});
 const parent=()=>({known:true,available:true,request_key_sha256:storage?.request_key_sha256||HASH,operation_id:OP,state:phase,credentials_returned:false,cellular_activation:false,completion_saved:false,station_trial_api:phase==='ready',station_save_api:phase==='ready',...(scanning?{scan:scanCapability||{supported:true,available:true,remaining:8}}:{})});
 const trial=(c,state='ready',extra={})=>({known:true,operation_id:OP,trial_id:c.trial_id,state,authoritative_result:state!=='testing'&&state!=='unknown',ap_return_verified:['ready','failed','cancelled'].includes(state),requires_confirmation:state==='ready',settings_saved:false,credentials_returned:false,cellular_activation:false,...extra});
 const commit=(c,state='saved',extra={})=>({known:true,operation_id:OP,trial_id:storage.commits.find(v=>v.id===c.commit_id)?.trial_id,commit_id:c.commit_id,state,settings_saved:state==='saved',authoritative_result:['saved','restored_ap','unknown'].includes(state),credentials_returned:false,cellular_activation:false,not_resubmitted:true,...extra});
 const defaults=async c=>{
  switch(c.op){
   case 'setup_ap_status':if(bootstrap)return {state:'ready',available:true,first_setup:true,owner_adoption_required:true,operation_id:OP,credentials_returned:false,cellular_activation:false,station_trial_api:true,station_save_api:true};return created?{state:'idle',available:true,station_trial_api:true,station_save_api:true}:parent();
   case 'setup_ap_start':assert(storage?.start_key===c.idempotency_key);return parent();
   case 'setup_ap_operation_status':return parent();
   case 'setup_ap_wifi_begin':assert.equal(storage.trials.at(-1),c.trial_id);return trial(c,'testing');
   case 'setup_ap_wifi_status':return trial(c);
   case 'setup_ap_wifi_confirm':assert.equal(storage.commits.at(-1).id,c.commit_id);return commit(c,'submitted');
   case 'setup_ap_wifi_commit_status':return commit(c);
   case 'setup_ap_cancel':assert(storage.cancel_sent);phase='closed';return parent();
   case 'setup_ap_scan_begin':assert.equal(scanStore.scan_id,c.scan_id,'must be durable before mutation');return scanReply(c);
   case 'setup_ap_scan_status':return scanReply(c);
   default:throw Error('unexpected command '+c.op);
  }
 };
 const panel=new ApConfigurator({document:{getElementById:id=>{assert(elements.has(id),id);return elements.get(id);}},crypto:webcrypto,
  env:{confirm:()=>approveScan,performance:{now:()=>now},setTimeout:(f,ms)=>{now+=ms;queueMicrotask(f);}},
  request:async c=>{calls.push(clone(c));return hook?hook(c,{defaults,parent,trial,commit,storage:clone(storage),panel}):defaults(c);},
  login:async()=>{if(loginHook)await loginHook(panel);authenticated=true;},getToken:()=>authenticated?'synthetic-session':null,
  loadPending:async()=>{if(storageError)throw Error('storage unavailable');return clone(storage);},
  savePending:async(next,expected)=>{if(failWrite)throw Error('storage failed');assert.equal(storage?.id??null,expected?.id??null);assert.equal(storage?.revision??null,expected?.revision??null);pendingInput(next);assert(!JSON.stringify(next).includes(PASS)&&!JSON.stringify(next).includes(SSID));storage=clone(next);},
  archivePending:async value=>{if(archiveError)throw Error("archive unavailable");archives.push(clone(value));},blocked:()=>blocked,
  scan:scanning?{domain:()=>HASH,button:scanButton,note:scanNote,load:async()=>clone(scanStore),save:async(v,expected)=>{if(scanWriteError)throw Error('scan storage unavailable');assert.deepEqual(scanStore,expected);scanStore=clone(v);},snapshot:v=>snapshots.push(clone(v))}:null});
 await panel.initialize();await panel.open();
 return {panel,calls,archives,snapshots,scanButton,scanNote,scanStore:()=>clone(scanStore),scanReply,$:id=>elements.get(id),storage:()=>clone(storage),replace:v=>{storage=clone(v);},failWrites:()=>{failWrite=true;},defaults,parent,trial,commit,
  fill:()=>{elements.get('ap-wifi-ssid').value=SSID;elements.get('ap-wifi-passphrase').value=PASS;},click:id=>elements.get('ap-wifi-'+id).onclick(),reopen:async()=>{panel.suspend();await panel.open();}};
}
{
 const f=await fixture();f.fill();await f.click('begin');assert.equal(f.panel.phase,'trial_ready');
 assert.deepEqual(f.calls.map(c=>c.op),['setup_ap_status','setup_ap_operation_status','setup_ap_wifi_begin','setup_ap_wifi_status']);assert(f.$('ap-wifi-ssid').readOnly);
 assert(!JSON.stringify(f.storage()).includes(PASS));await f.click('confirm');assert.equal(f.panel.phase,'saved');assert.equal(f.$('ap-wifi-passphrase').value,'');assert.equal(f.$('ap-wifi-ssid').value,'');
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);await f.click('finish');assert.equal(f.storage(),null);assert.equal(f.archives.length,1);assert(!JSON.stringify(f.archives).includes(PASS));
}
{
 const f=await fixture({created:true,hook:async(c,x)=>{if(c.op==='setup_ap_start')throw Error('lost start reply');return x.defaults(c);}});f.fill();await f.click('begin');
 assert(f.storage().start_key&&f.storage().operation_id===null);assert.equal(f.panel.phase,'unknown');
 const copy=f.storage(),next=await fixture({record:copy});await next.click('query');assert.equal(next.panel.phase,'parent_ready');assert.equal(next.storage().operation_id,OP);
 assert.deepEqual(next.calls.map(c=>c.op),['setup_ap_operation_status']);assert.equal(next.calls.filter(c=>c.op==='setup_ap_start').length,0);
}
{
 const f=await fixture({hook:async(c,x)=>{if(c.op==='setup_ap_wifi_confirm')throw Error('lost save reply');return x.defaults(c);}});f.fill();await f.click('begin');await f.click('confirm');assert.equal(f.panel.phase,'unknown');assert(f.storage().commits.length===1);assert.equal(f.$('ap-wifi-passphrase').value,'');
 const next=await fixture({record:f.storage()});await next.click('query');assert.equal(next.panel.phase,'saved');assert.deepEqual(next.calls.map(c=>c.op),['setup_ap_wifi_commit_status']);
}
{
 let ready=false;const f=await fixture({hook:async(c,x)=>{if(c.op==='setup_ap_wifi_begin')throw Error('lost trial reply');if(c.op==='setup_ap_wifi_status')return x.trial(c,ready?'ready':'unknown');return x.defaults(c);}});
 f.fill();await f.click('begin');assert.equal(f.panel.phase,'unknown');const n=f.calls.length;await f.click('query');assert(f.calls.slice(n).every(c=>['setup_ap_wifi_status','setup_ap_operation_status'].includes(c.op)));
 ready=true;await f.reopen();assert.equal(f.panel.phase,'trial_ready');assert.equal(f.$('ap-wifi-passphrase').value,'');assert(!f.$('ap-wifi-reenter').hidden);assert(!f.$('ap-wifi-ssid').readOnly);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin').length,1);
}
{
 const f=await fixture({loginHook:async panel=>panel.suspend()});f.fill();await f.click('begin');assert.equal(f.calls.length,0);assert.equal(f.storage(),null);assert.equal(f.$('ap-wifi-passphrase').value,'');
}
{
 const f=await fixture();f.fill();f.failWrites();await f.click('begin');assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin'||c.op==='setup_ap_start').length,0);
 const bad=await fixture({storageError:true});bad.fill();await bad.click('begin');assert.equal(bad.calls.length,0);assert(bad.$('ap-wifi-begin').disabled);
}
{
 const f=await fixture({blocked:true});f.fill();await f.click('begin');assert.equal(f.calls.length,0);assert.equal(f.storage(),null);
}
for(const extra of [{settings_saved:false},{authoritative_result:false},{commit_id:'99999999-9999-4999-8999-999999999999'}]){
 const f=await fixture({hook:async(c,x)=>c.op==='setup_ap_wifi_commit_status'?x.commit(c,'saved',extra):x.defaults(c)});f.fill();await f.click('begin');await f.click('confirm');assert.equal(f.panel.phase,'unknown');assert.equal(f.panel.terminal,false);assert(f.$('ap-wifi-finish').hidden);assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);
}
{
 let failedId=null;const f=await fixture({hook:async(c,x)=>{if(c.op==='setup_ap_wifi_status'){failedId??=c.trial_id;return x.trial(c,c.trial_id===failedId?'failed':'ready');}return x.defaults(c);}});f.fill();await f.click('begin');assert.equal(f.panel.phase,'failed');const old=f.storage().trials[0];f.fill();await f.click('begin');assert.equal(f.panel.phase,'trial_ready');assert.equal(f.storage().trials[0],old);assert.equal(f.storage().trials.length,2);assert.notEqual(f.storage().trials[1],old);
}
{
 const f=await fixture();f.fill();await f.click('begin');const record=f.storage();record.revision++;record.commits=[{id:webcrypto.randomUUID(),trial_id:record.trials[0]}];f.replace(record);
 await f.click('confirm');assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,0);assert.equal(f.panel.terminal,false);assert.equal(f.$('ap-wifi-passphrase').value,'');
}
{
 const f=await fixture();f.fill();await f.click('begin');await f.click('cancel');assert.equal(f.panel.phase,'closed');assert(f.panel.terminal);assert.equal(f.calls.filter(c=>c.op==='setup_ap_cancel').length,1);assert.equal(f.$('ap-wifi-passphrase').value,'');
}
{
 const f=await fixture({hook:async(c,x)=>c.op==='setup_ap_wifi_commit_status'?{known:false,not_found_is_not_proof:true}:x.defaults(c)});f.fill();await f.click('begin');await f.click('confirm');assert.equal(f.panel.phase,'unknown');assert(!f.panel.terminal);assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);await f.click('query');assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);
}
{
 const f=await fixture();f.fill();await f.click('begin');const good=f.storage();assert.deepEqual(pendingInput(good),good);
 for(const changed of [{...good,passphrase:PASS},{...good,ssid:SSID},{...good,revision:-1},{...good,operation_id:'../bad'},{...good,trials:[...good.trials,good.trials[0]]}])assert.throws(()=>pendingInput(changed));
 assert.throws(()=>credentialsInput('中'.repeat(11),PASS));assert.throws(()=>credentialsInput('ok','short'));assert.throws(()=>credentialsInput('bad\0name',PASS));assert.deepEqual(credentialsInput(SSID,PASS),{ssid:SSID,passphrase:PASS});
}
console.log('AP Wi-Fi: automatic finite trial, explicit confirmation, metadata CAS, read-only recovery, lost replies, permissions boundary and credential cleanup passed.');

// A reopened page has no Wi-Fi secret. Input validation cannot make a known
// successful trial "unknown": no save was submitted and the same trial remains usable.
for(const bad of [{ssid:'',passphrase:PASS},{ssid:SSID,passphrase:''},{ssid:SSID,passphrase:'short'}]){
 const f=await fixture();f.fill();await f.click('begin');await f.reopen();
 assert.equal(f.panel.phase,'trial_ready');const before=f.storage(),calls=f.calls.length;
 f.$('ap-wifi-ssid').value=bad.ssid;f.$('ap-wifi-passphrase').value=bad.passphrase;
 await f.click('confirm');assert.equal(f.panel.phase,'trial_ready');assert.deepEqual(f.storage(),before);
 assert.equal(f.calls.length,calls);assert(!f.$('ap-wifi-confirm').hidden);assert(!f.$('ap-wifi-passphrase').readOnly);
 assert.match(f.$('ap-wifi-state').textContent,/尚未提交/);assert.doesNotMatch(f.$('ap-wifi-state').textContent,/进度已保留|结果未确认/);
 f.fill();await f.click('confirm');assert.equal(f.panel.phase,'saved');
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin').length,1);assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);
}

// A successful final bounded poll must not be overwritten with Unknown.
{
 let polls=0;const f=await fixture({hook:async(c,x)=>c.op==='setup_ap_wifi_status'?x.trial(c,++polls===60?'ready':'testing'):x.defaults(c)});
 f.fill();await f.click('begin');assert.equal(polls,60);assert.equal(f.panel.phase,'trial_ready');
}
// Archive failure keeps the record queryable, never a fake terminal receipt.
{
 const f=await fixture({archiveError:true});f.fill();await f.click('begin');await f.click('confirm');await f.click('finish');
 assert(f.storage());assert.equal(f.panel.phase,'unknown');assert(!f.panel.terminal);assert(!f.$('ap-wifi-query').hidden);
 await f.click('query');assert.equal(f.panel.phase,'saved');assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_confirm').length,1);
}

// A just-claimed setup AP is adopted by the existing explicit Owner-start API.
{
 const f=await fixture({bootstrap:true});f.fill();await f.click('begin');assert.equal(f.panel.phase,'trial_ready');
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_start').length,1);assert.equal(f.storage().operation_id,OP);
 assert(f.storage().start_key);await f.click('confirm');assert.equal(f.panel.phase,'saved');
}
{
 const f=await fixture({bootstrap:true,hook:async(c,x)=>{if(c.op==='setup_ap_start')throw Error('lost adoption reply');return x.defaults(c);}});
 f.fill();await f.click('begin');assert.equal(f.panel.phase,'unknown');assert.equal(f.storage().operation_id,null);
 const next=await fixture({record:f.storage()});await next.click('query');assert.equal(next.panel.phase,'parent_ready');assert.equal(next.storage().operation_id,OP);
 assert.deepEqual(next.calls.map(c=>c.op),['setup_ap_operation_status']);
}

// A partial server-side handoff may still use the original bootstrap cancel.
// Its acknowledgement cannot itself claim completion; query proves closure.
{
 let closed=false;
 const f=await fixture({bootstrap:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')throw Error('adoption metadata reply lost');
  if(c.op==='setup_ap_cancel'){closed=true;return {first_setup:true,operation_id:OP,state:'closing',credentials_returned:false,cellular_activation:false};}
  if(c.op==='setup_ap_operation_status'&&closed)return {...x.parent(),state:'closed'};
  return x.defaults(c);
 }});f.fill();await f.click('begin');assert.equal(f.panel.phase,'unknown');await f.click('cancel');
 assert.equal(f.panel.phase,'closed');assert(f.panel.terminal);assert.equal(f.calls.filter(c=>c.op==='setup_ap_start').length,1);assert.equal(f.calls.filter(c=>c.op==='setup_ap_cancel').length,1);
}
// Root refused in preflight (before any network change): the page shows the reason, not "unknown".
{
 const f=await fixture({created:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')return {...x.parent(),state:'starting'};
  if(c.op==='setup_ap_operation_status')return {...x.parent(),state:'closed',refusal:'station_connected'};
  return x.defaults(c);
 }});f.fill();await f.click('begin');
 assert.equal(f.panel.phase,'closed');assert(f.panel.terminal);
 assert.equal(f.$('ap-wifi-state').textContent,refusals.station_connected);assert.match(refusals.station_connected,/其他连接/);assert.match(refusals.station_saved,/多个 Wi-Fi/);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin').length,0);
}
// A closed session without a refusal keeps the plain closed message.
{
 const f=await fixture({created:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')return {...x.parent(),state:'starting'};
  if(c.op==='setup_ap_operation_status')return {...x.parent(),state:'closed',refusal:null};
  return x.defaults(c);
 }});f.fill();await f.click('begin');
 assert.equal(f.panel.phase,'closed');assert.equal(f.$('ap-wifi-state').textContent,'设备已确认本次配置会话结束。');
}
// Unknown refusal codes, or a refusal on a non-closed state, are rejected as invalid input.
for(const bad of [{state:'closed',refusal:'org.freedesktop.NetworkManager.Error'},{state:'starting',refusal:'station_saved'},{state:'closed',refusal:7}]){
 const f=await fixture({created:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')return {...x.parent(),state:'starting'};
  if(c.op==='setup_ap_operation_status')return {...x.parent(),...bad};
  return x.defaults(c);
 }});f.fill();await f.click('begin');
 assert.notEqual(f.panel.phase,'closed');assert.doesNotMatch(f.$('ap-wifi-state').textContent,/NetworkManager/);
}
for(const [code,text] of Object.entries(refusals))assert(typeof code==='string'&&/^[a-z_]+$/.test(code)&&text.length>8);
// "开启热点": one durable start, waits for ready, never begins a trial or sends credentials.
{
 const f=await fixture({created:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')return {...x.parent(),state:'starting'};
  return x.defaults(c);
 }});
 assert.equal(f.$('ap-wifi-open').hidden,false);
 await f.click('open');
 assert.equal(f.panel.phase,'parent_ready');assert.match(f.$('ap-wifi-state').textContent,/配置热点已开启/);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_start').length,1);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin').length,0);
 assert(f.storage().start_key&&f.storage().trials.length===0);
 assert.equal(f.$('ap-wifi-open').hidden,true);
 // A later trial reuses the same parent: no second start.
 f.fill();await f.click('begin');
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_start').length,1);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_wifi_begin').length,1);
}
// A refused start shows the reason after "开启热点" too.
{
 const f=await fixture({created:true,hook:async(c,x)=>{
  if(c.op==='setup_ap_start')return {...x.parent(),state:'starting'};
  if(c.op==='setup_ap_operation_status')return {...x.parent(),state:'closed',refusal:'station_saved'};
  return x.defaults(c);
 }});
 await f.click('open');
 assert.equal(f.panel.phase,'closed');assert.equal(f.$('ap-wifi-state').textContent,refusals.station_saved);
}
console.log('refusal display and open-AP checks passed');
// In-place rescan shares the production controller's single-flight lock. Empty
// authoritative results replace old choices; reload/lost replies never replay Begin.
{
 const f=await fixture({scanning:true});await f.scanButton.onclick();
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_scan_begin').length,1);
 assert.deepEqual(f.snapshots[0].networks,[]);assert.equal(f.scanStore(),null);
 assert.match(f.scanNote.textContent,/扫描完成/);assert.equal(f.panel.phase,'parent_ready');
 assert(!JSON.stringify(f.storage()).includes(PASS));
}
for(const options of [{approveScan:false},{scanCapability:{supported:false,available:false}},{scanCapability:{supported:true,available:false,refusal:'cooldown',cooldown_remaining_ms:1000}},{scanCapability:{supported:true,available:false,refusal:'limit'}}]){
 const f=await fixture({scanning:true,...options});await f.scanButton.onclick();
 assert(!f.calls.some(c=>c.op==='setup_ap_scan_begin'));assert.equal(f.scanStore(),null);
}
{
 const f=await fixture({scanning:true,scanWriteError:true});await f.scanButton.onclick();
 assert(!f.calls.some(c=>c.op==='setup_ap_scan_begin'));
}
{
 const f=await fixture({scanning:true,hook:async(c,x)=>{if(c.op==='setup_ap_scan_begin')throw Error('lost');return x.defaults(c);}});
 f.fill();await f.scanButton.onclick();const saved=f.scanStore();assert(saved?.scan_id);
 assert(f.$('ap-wifi-begin').disabled&&f.$('ap-wifi-confirm').disabled&&f.$('ap-wifi-cancel').disabled);
 await f.click('begin');assert(!f.calls.some(c=>c.op==='setup_ap_wifi_begin'));
 await f.scanButton.onclick();assert.equal(f.calls.filter(c=>c.op==='setup_ap_scan_begin').length,1);
 assert.equal(f.scanStore(),null);assert.equal(f.snapshots.length,1);
 const recovered=await fixture({scanning:true,record:f.storage(),retainedScan:saved});
 assert.equal(recovered.snapshots.length,0); // login has not happened yet
 await recovered.scanButton.onclick();
 assert(!recovered.calls.some(c=>c.op==='setup_ap_scan_begin'||c.op==='setup_ap_start'));
 assert.equal(recovered.snapshots.length,1);assert.equal(recovered.scanStore(),null);
 assert(!JSON.stringify(saved).includes(PASS));
}
for(const extra of [{authoritative_result:false},{operation_id:'22222222-2222-4222-8222-222222222222'},{scan_id:'22222222-2222-4222-8222-222222222222'},{credentials_returned:true},{ap_return_verified:false},{snapshot:{networks:[]}}]){
 const f=await fixture({scanning:true,hook:async(c,x)=>c.op.startsWith('setup_ap_scan_')?{...await x.defaults(c),...extra}:x.defaults(c)});
 await f.scanButton.onclick();assert(f.scanStore());assert.equal(f.snapshots.length,0);
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_scan_begin').length,1);
}
for(const state of ['failed','refused','cancelled']){
 const f=await fixture({scanning:true,hook:async(c,x)=>c.op==='setup_ap_scan_begin'?{...await x.defaults(c),state,snapshot:null}:x.defaults(c)});
 await f.scanButton.onclick();assert.equal(f.scanStore(),null);assert.equal(f.snapshots.length,0);
}
{
 const f=await fixture({scanning:true,hook:async(c,x)=>c.op==='setup_ap_scan_begin'?{...await x.defaults(c),historical:true,ap_return_verified:false}:x.defaults(c)});
 await f.scanButton.onclick();assert.equal(f.scanStore(),null);assert.equal(f.snapshots.length,0);assert.match(f.scanNote.textContent,/历史/);
}
{
 let release;const gate=new Promise(r=>release=r);
 const f=await fixture({scanning:true,hook:async(c,x)=>{if(c.op==='setup_ap_scan_begin')await gate;return x.defaults(c);}});
 const running=f.scanButton.onclick();for(let i=0;i<30&&!f.calls.some(c=>c.op==='setup_ap_scan_begin');i++)await turn();
 await f.click('begin');release();await running;
 assert.equal(f.calls.filter(c=>c.op==='setup_ap_scan_begin').length,1);assert(!f.calls.some(c=>c.op==='setup_ap_wifi_begin'));
}
console.log('rescan single-flight, consent, durable-before-send, empty snapshot, reload/query-only, refusal and invalid-result checks passed');
