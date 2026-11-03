// SPDX-License-Identifier: AGPL-3.0-only
// Focused, reproducible negative proof: optionally load the unpatched module
// extracted from the recorded base SHA, never a copied controller algorithm.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import {resolve} from 'node:path';
const source=await readFile(process.argv[2]?resolve(process.argv[2]):new URL('../crates/edge/src/web/ap-wifi.js',import.meta.url));
const {ApConfigurator}=await import('data:text/javascript;base64,'+source.toString('base64'));
const OP='11111111-1111-4111-8111-111111111111',HASH='a'.repeat(64);
const elements=new Map(),snapshots=[];
const document={getElementById:id=>{
 if(!elements.has(id))elements.set(id,{value:'',checked:false,dataset:{},setAttribute(){}});
 return elements.get(id);
}};
let record=null,scanRecord=null;
const parent=()=>({known:true,state:'ready',available:true,operation_id:OP,request_key_sha256:HASH,station_trial_api:true,station_save_api:true,credentials_returned:false,cellular_activation:false,scan:{supported:true,available:true}});
const scan={domain:()=>HASH,button:{},note:{},load:async()=>structuredClone(scanRecord),save:async(next,expected)=>{assert.deepEqual(scanRecord,expected);scanRecord=structuredClone(next);},snapshot:s=>snapshots.push(s)};
const panel=new ApConfigurator({document,crypto:webcrypto,env:{confirm:()=>true},scan,
 getToken:()=> 'synthetic-session',login:async()=>{},loadPending:async()=>structuredClone(record),
 savePending:async next=>{record=structuredClone(next);},archivePending:async()=>{},
 request:async c=>{
  if(['setup_ap_status','setup_ap_operation_status'].includes(c.op))return parent();
  assert.equal(c.op,'setup_ap_scan_begin');assert.equal(scanRecord.scan_id,c.scan_id);
  // Another real page can finish and CAS-clear this exact scan while the
  // original HTTP Begin response remains delayed. This is the storage boundary.
  scanRecord=null;
  return {known:true,state:'ready',operation_id:OP,scan_id:c.scan_id,authoritative_result:true,ap_return_verified:true,historical:false,settings_saved:false,credentials_returned:false,cellular_activation:false,snapshot:{requested_uptime_ms:10,last_scan_uptime_ms:11,completed_uptime_ms:12,networks:[],omitted:0,truncated:false}};
 }});
await panel.open();await scan.button.onclick();
assert.equal(snapshots.length,0,'a stale tab must not publish before checking durable scan ownership');
console.log('WEB3-AP: competing-tab receipt fence passed using actual ApConfigurator.');
