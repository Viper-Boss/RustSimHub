// SPDX-License-Identifier: AGPL-3.0-only
// Explicit synthetic fixture. No fetch, persistence, secrets, engine or production switch.
import { NasBridgePanel, pendingInput, receiptInput, clearExported } from '/nas-bridge.js';
const key=Array.from({length:32},(_,i)=>i+1),pin='ab'.repeat(32),rows=[{id:'synthetic-nas',public_key:key,kind:'nas',scopes:11,generation:2,active:true}];
let pending=null,sequence=0;const bindings=new Map(),receipts=[];
const log=text=>{document.getElementById('fixture-log').textContent=text;};
new NasBridgePanel({document,crypto,getDevicePin:()=>pin,
 confirm:()=>{log('合成确认已记录；没有操作真实设备。');return true;},
 request:async command=>{
  log('仅内存模拟：'+command.op);
  if(command.op==='clients')return structuredClone(rows);
  if(command.op==='legacy_binding_status'){const binding=bindings.get(command.core_id);return binding?{known:true,binding:structuredClone(binding),principal_current:true}:{known:false};}
  if(command.op==='bind_legacy_checked'){
   const expected=command.expected_binding,binding=bindings.get(command.core_id);if(expected.state==='absent'?!!binding:binding?.binding_epoch!==expected.binding_epoch)throw Error('synthetic stale binding');
   const saved={core_id:command.core_id,public_key:key,generation:2,scopes:11,binding_epoch:String(++sequence).padStart(64,'0')};bindings.set(command.core_id,saved);
   if(document.getElementById('fixture-lost').checked){document.getElementById('fixture-lost').checked=false;throw Error('synthetic lost reply');}
   return structuredClone(saved);
  }
  if(command.op==='revoke_legacy_checked'){
   if(bindings.get(command.core_id)?.binding_epoch!==command.expected_binding_epoch)throw Error('synthetic stale binding');bindings.delete(command.core_id);return {ok:true};
  }
  throw Error('synthetic unsupported request');
 },loadPending:async()=>structuredClone(pending),savePending:async(value,expected)=>{
  if(JSON.stringify(pendingInput(pending))!==JSON.stringify(pendingInput(expected)))throw Error('synthetic CAS');pending=structuredClone(value);
 },archivePending:async value=>{receipts.push(receiptInput(value));},
 loadJournal:async guard=>{guard();return structuredClone({pending,receipts});},
 clearJournal:async(exported,guard)=>{guard();const result=clearExported({pending,receipts},exported,pin);receipts.splice(0,receipts.length,...result.receipts);return result;}});
log('尚未读取；本页没有自动请求。');
