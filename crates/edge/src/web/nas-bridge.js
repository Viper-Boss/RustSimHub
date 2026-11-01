// SPDX-License-Identifier: AGPL-3.0-only
// Owner-only assembly for the existing NAS bridge. No credential, engine or network ownership.
import { clientsInput, keyHex, scopeNames, scopeText } from './clients.js';
const READ_MASK=1|2|8;
const ID=/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const EPOCH=/^[0-9a-f]{64}$/;
const validEpoch=value=>typeof value==='string'&&EPOCH.test(value);
const validKey=value=>keyHex(value).length===64;
const alias=value=>typeof value==='string'&&/^[A-Za-z0-9_.-]{1,128}$/.test(value);
const fields=(value,expected)=>value!==null&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).sort().join()===expected.slice().sort().join();
const mask=value=>Number.isInteger(value)&&value>0&&value<=READ_MASK&&(value&~READ_MASK)===0;
const generation=value=>Number.isSafeInteger(value)&&value>0;
export function bindingInput(value){
 if(!fields(value,['core_id','public_key','generation','binding_epoch','scopes'])||!alias(value.core_id)||!generation(value.generation)||!validEpoch(value.binding_epoch)||!mask(value.scopes))throw Error('NAS绑定回复无效');
 if(!validKey(value.public_key))throw Error('NAS公钥格式无效');return {...value,public_key:[...value.public_key]};
}
export function statusInput(value,coreId){
 if(fields(value,['known'])&&value.known===false)return {known:false};
 if(!fields(value,['known','binding','principal_current'])||value.known!==true||typeof value.principal_current!=='boolean')throw Error('NAS绑定状态无效');
 const binding=bindingInput(value.binding);if(binding.core_id!==coreId)throw Error('NAS绑定别名不匹配');
 return {known:true,binding,principal_current:value.principal_current};
}
export function pendingInput(value){
 if(value===undefined||value===null)return null;
 if(!fields(value,['format','id','action','core_id','public_key','generation','scopes','expected_binding_epoch','device_pin'])||value.format!==1||!(typeof value.id==='string'&&ID.test(value.id))||!validEpoch(value.device_pin)||!['bind','revoke'].includes(value.action)||!alias(value.core_id)||!generation(value.generation)||!mask(value.scopes)||!(value.expected_binding_epoch===null||validEpoch(value.expected_binding_epoch))||(value.action==='revoke'&&value.expected_binding_epoch===null))throw Error('NAS待确认记录无效，请保留原记录');
 if(!validKey(value.public_key))throw Error('NAS公钥格式无效');return {...value,public_key:[...value.public_key]};
}
export function receiptInput(value){
 if(!fields(value,['intent','source','observed'])||!['reply','current_state'].includes(value.source))throw Error('NAS核对回执无效');
 const intent=pendingInput(value.intent);if(!intent)throw Error('NAS核对回执缺少原记录');
 return {intent,source:value.source,observed:statusInput(value.observed,intent.core_id)};
}
export function journalInput(value){
 if(!fields(value,['pending','receipts'])||!Array.isArray(value.receipts)||value.receipts.length>64)throw Error('NAS本机记录无效');
 const pending=pendingInput(value.pending),receipts=value.receipts.map(receiptInput);
 if(new Set(receipts.map(row=>row.intent.id)).size!==receipts.length)throw Error('NAS本机回执编号重复');
 return {pending,receipts};
}
export function exportInput(value){
 if(!fields(value,['format','kind','device_pin','pending','receipts'])||value.format!==1||value.kind!=='rustsimhub.nas-binding-journal'||!validEpoch(value.device_pin))throw Error('NAS导出记录无效');
 const journal=journalInput({pending:value.pending,receipts:value.receipts});
 if(journal.pending&&journal.pending.device_pin!==value.device_pin||journal.receipts.some(row=>row.intent.device_pin!==value.device_pin))throw Error('NAS导出记录属于另一台设备');
 return {format:1,kind:value.kind,device_pin:value.device_pin,...journal};
}
export function exportJournal(value,pin){
 if(!validEpoch(pin))throw Error('请先核对设备身份');const journal=journalInput(value);
 return exportInput({format:1,kind:'rustsimhub.nas-binding-journal',device_pin:pin,pending:journal.pending?.device_pin===pin?journal.pending:null,receipts:journal.receipts.filter(row=>row.intent.device_pin===pin)});
}
export function clearExported(value,exported,pin){
 const journal=journalInput(value),saved=exportInput(exported);if(saved.device_pin!==pin)throw Error('设备身份已变化');
 const current=new Map(journal.receipts.map(row=>[row.intent.id,row]));
 for(const row of saved.receipts)if(JSON.stringify(current.get(row.intent.id))!==JSON.stringify(row))throw Error('回执已由其他页面更改，请重新导出');
 const removable=new Set(saved.receipts.filter(row=>row.intent.id!==journal.pending?.id).map(row=>row.intent.id));
 return {receipts:journal.receipts.filter(row=>!removable.has(row.intent.id)),removed:removable.size};
}
// Read both keys in one transaction; cleanup changes only exact exported receipts.
// The pending key is never written, including when an archive exists before intent cleanup.
function journalTransaction(db,mode,change,guard){
 return new Promise((resolve,reject)=>{
  const tx=db.transaction('state',mode),table=tx.objectStore('state'),values={};let failure,result,ready=0;
  for(const [field,key] of [['pending','nas-binding-pending'],['receipts','nas-binding-receipts']]){
   const reading=table.get(key);reading.onsuccess=()=>{values[field]=reading.result??(field==='receipts'?[]:null);if(++ready!==2)return;
    try{guard();const journal=journalInput(values);result=change?change(journal):journal;if(change)table.put(result.receipts,'nas-binding-receipts');}catch(error){failure=error;tx.abort();}
   };
  }
  tx.oncomplete=()=>resolve(result);tx.onerror=tx.onabort=()=>reject(failure||Error('浏览器存储不可用'));
 });
}
export const readJournal=(db,guard)=>journalTransaction(db,'readonly',null,guard);
export const clearJournal=(db,exported,pin,guard)=>journalTransaction(db,'readwrite',value=>clearExported(value,exported,pin),guard);
export class NasBridgePanel {
 constructor({document,request,confirm,loadPending,savePending,archivePending,loadJournal,clearJournal,getDevicePin,crypto=globalThis.crypto,url=globalThis.URL,Blob=globalThis.Blob}){
  Object.assign(this,{doc:document,request,confirm,loadPending,savePending,archivePending,loadJournal,clearJournal,getDevicePin,crypto,url,Blob});this.$=id=>document.getElementById(id);this.epoch=0;
  for(const [id,method] of [['nas-refresh','refresh'],['nas-query','query'],['nas-bind','bind'],['nas-revoke','revoke'],['nas-acknowledge','acknowledge'],['nas-export','prepareExport'],['nas-clear-exported','clearExport']])this.$(id).onclick=()=>void this[method]();
  this.$('nas-download').onclick=event=>{try{if(this.busy||!this.exported||this.exported.device_pin!==this.pin())throw Error('导出已失效');this.downloadClicked=true;this.buttons();}catch{event.preventDefault();this.clearDownload();this.buttons();}};
  for(const id of ['nas-client','nas-alias'])this.$(id).oninput=()=>{this.snapshot=null;const row=this.rows.find(r=>r.hex===this.$('nas-client').value);this.$('nas-key').textContent=row?`所选NAS公钥：${row.hex}`:'';this.$('nas-binding').textContent='选择已变化，请先核对当前绑定。';this.state('选择或别名已变化；请先核对当前绑定。');this.buttons();};
  this.reset();
 }
 reset(){++this.epoch;this.clearDownload();this.busy=false;this.pending=null;this.snapshot=null;this.rows=[];this.$('nas-client').replaceChildren();this.$('nas-alias').value='';this.$('nas-key').textContent='';this.$('nas-binding').textContent='尚未读取';this.state('需要所有者最近5分钟内登录。先读取已批准的NAS，再核对和保存。');this.buttons();}
 clearDownload(){if(this.exportUrl)this.url.revokeObjectURL(this.exportUrl);this.exportUrl=null;this.exported=null;this.downloadClicked=false;this.$('nas-download').hidden=true;this.$('nas-download').href='';}
 pin(){const pin=this.getDevicePin();if(typeof pin!=='string'||!EPOCH.test(pin))throw Error('请先登录并核对设备身份');return pin;}
 current(epoch){if(epoch!==this.epoch||this.operationPin!==this.pin())throw Error('NAS管理会话已结束');}
 state(text){this.$('nas-state').textContent=text;}
 buttons(){
  for(const id of ['nas-refresh','nas-query'])this.$(id).disabled=this.busy;
  for(const id of ['nas-client','nas-alias'])this.$(id).disabled=this.busy||!!this.pending;
  this.$('nas-bind').disabled=this.busy||!!this.pending||!this.snapshot||!this.rows.length;
  this.$('nas-revoke').disabled=this.busy||!!this.pending||!this.snapshot?.known;
  this.$('nas-acknowledge').hidden=!this.pending;this.$('nas-acknowledge').disabled=this.busy||!this.pending||!this.snapshot;
  this.$('nas-export').disabled=this.busy;this.$('nas-clear-exported').disabled=this.busy||!this.downloadClicked||!this.exported?.receipts.length;
 }
 async run(fn){if(this.busy)return false;const g=this.epoch;this.busy=true;this.buttons();
  try{this.operationPin=this.pin();await fn(g);return true;}
  catch{if(g===this.epoch)this.state(this.pending?'结果未确认；请核对当前绑定，不会自动重复保存或撤销。':'未能完成NAS管理。请检查别名、最近5分钟所有者登录与设备状态；没有自动重试。');return false;}
  finally{if(g===this.epoch){this.busy=false;this.buttons();}}
 }
 async pendingAt(g){const pending=pendingInput(await this.loadPending());this.current(g);if(pending&&pending.device_pin!==this.pin())throw Error('待确认记录属于另一台设备，请保留原记录');if(JSON.stringify(this.pending)!==JSON.stringify(pending))this.snapshot=null;this.pending=pending;return pending;}
 selected(){const key=this.$('nas-client').value;const row=this.rows.find(r=>r.hex===key);if(!row)throw Error('请先读取并选择已批准的NAS');return row;}
 coreId(){const value=this.pending?.core_id??this.$('nas-alias').value.trim();if(!alias(value))throw Error('NAS别名只允许1–128位英文字母、数字、点、下划线或连字符');return value;}
 async inspect(g){const coreId=this.coreId();this.snapshot=null;const value=statusInput(await this.request({op:'legacy_binding_status',core_id:coreId}),coreId);this.current(g);this.snapshot={...value,coreId,pendingId:this.pending?.id??null};this.render();return value;}
 render(){
  const value=this.snapshot;if(!value)return;
  if(!value.known)this.$('nas-binding').textContent=`别名：${value.coreId}\n当前没有绑定。`;
  else{const b=value.binding;this.$('nas-binding').textContent=`别名：${b.core_id}\nNAS公钥：${keyHex(b.public_key)}\n授权代次：${b.generation}\n权限：${scopeText(scopeNames(b.scopes))}\n当前授权：${value.principal_current?'仍有效':'已撤销或代次已变化；不能沿用'}\n绑定版本：${b.binding_epoch}`;}
 }
 refresh(){return this.run(async g=>{
  const pending=await this.pendingAt(g);const all=clientsInput(await this.request({op:'clients'}));this.current(g);
  this.rows=all.filter(r=>r.active&&r.kind==='nas'&&r.scopes.some(s=>['status.read','sms.read','calls.read'].includes(s)));
  if(new Set(this.rows.map(r=>r.hex)).size!==this.rows.length)throw Error('NAS列表含重复公钥');
  const previous=this.$('nas-client').value;this.$('nas-client').replaceChildren();
  for(const row of this.rows){const option=this.doc.createElement('option');option.value=row.hex;option.textContent=`NAS ${row.hex.slice(0,12)}… · 第${row.generation}代`;this.$('nas-client').append(option);}
  this.$('nas-client').value=pending?keyHex(pending.public_key):(this.rows.some(r=>r.hex===previous)?previous:this.rows[0]?.hex??'');
  if(pending)this.$('nas-alias').value=pending.core_id;
  else if(!this.$('nas-alias').value&&this.rows.length)this.$('nas-alias').value='nas-'+this.rows[0].hex.slice(0,8);
  const row=this.rows.find(r=>r.hex===this.$('nas-client').value);this.$('nas-key').textContent=row?`所选NAS公钥：${row.hex}`:'';
  if(pending||this.$('nas-alias').value){await this.inspect(g);this.state(pending?'已读取当前状态；上次操作是否执行仍以原记录为准。核对后可确认本次查看结果。':'已读取。核对NAS公钥后，可保存只读绑定。');}
  else this.state('尚无已批准的NAS。请先在配对审批中核对并批准独立NAS公钥；不要使用所有者或设备私钥。');
 });}
 query(){return this.run(async g=>{await this.pendingAt(g);await this.inspect(g);this.state(this.pending?'已核对当前绑定；不会重发原操作。确认结果只保存本机核对记录。':'已核对当前绑定；没有保存或撤销。');});}
 async start(g,intent){await this.savePending(pendingInput(intent),null);this.current(g);this.pending=pendingInput(intent);this.snapshot=null;this.buttons();}
 async finish(g,intent,source,observed){const receipt=receiptInput({intent,source,observed});await this.archivePending(receipt);this.current(g);await this.savePending(null,intent);this.current(g);this.pending=null;this.snapshot={...receipt.observed,coreId:intent.core_id};this.render();}
 bind(){return this.run(async g=>{
  if(await this.pendingAt(g))throw Error('请先核对上次操作');const row=this.selected(),coreId=this.coreId();
  if(!this.snapshot||this.snapshot.coreId!==coreId)throw Error('请先核对当前绑定');
  const scopes=row.scopes.filter(s=>['status.read','sms.read','calls.read'].includes(s));const bits=scopes.reduce((m,s)=>m|({'status.read':1,'sms.read':2,'calls.read':8}[s]),0);
  if(!mask(bits))throw Error('NAS缺少可绑定的只读权限');
  if(!this.confirm(`保存NAS只读绑定？\n别名：${coreId}\n公钥：${row.hex}\n权限：${scopeText(scopes)}\n这会更新该别名的绑定版本并关闭旧NAS会话。`))return;
  const intent=pendingInput({format:1,id:this.crypto.randomUUID(),device_pin:this.pin(),action:'bind',core_id:coreId,public_key:row.public_key,generation:row.generation,scopes:bits,expected_binding_epoch:this.snapshot.known?this.snapshot.binding.binding_epoch:null});
  await this.start(g,intent);const binding=bindingInput(await this.request({op:'bind_legacy_checked',core_id:coreId,public_key:intent.public_key,expected_generation:intent.generation,scopes,expected_binding:intent.expected_binding_epoch===null?{state:'absent'}:{state:'present',binding_epoch:intent.expected_binding_epoch}}));this.current(g);
  if(binding.core_id!==coreId||keyHex(binding.public_key)!==row.hex||binding.generation!==intent.generation||binding.scopes!==bits||binding.binding_epoch===intent.expected_binding_epoch)throw Error('NAS绑定回复与原请求不一致');
  await this.finish(g,intent,'reply',{known:true,binding,principal_current:true});this.state('NAS只读绑定已保存。下方绑定版本可填入NAS客户端；没有启动后台进程。');
 });}
 revoke(){return this.run(async g=>{
  if(await this.pendingAt(g))throw Error('请先核对上次操作');const coreId=this.coreId();if(!this.snapshot?.known||this.snapshot.coreId!==coreId)throw Error('请先核对当前绑定');
  const b=this.snapshot.binding;if(!this.confirm(`撤销别名 ${coreId} 的NAS读取绑定？\n公钥：${keyHex(b.public_key)}\n只撤销刚核对的版本，不撤销其他客户端。`))return;
  const intent=pendingInput({format:1,id:this.crypto.randomUUID(),device_pin:this.pin(),action:'revoke',core_id:coreId,public_key:b.public_key,generation:b.generation,scopes:b.scopes,expected_binding_epoch:b.binding_epoch});
  await this.start(g,intent);const reply=await this.request({op:'revoke_legacy_checked',core_id:coreId,expected_binding_epoch:b.binding_epoch});this.current(g);if(!fields(reply,['ok'])||reply.ok!==true)throw Error('NAS撤销回复无效');
  await this.finish(g,intent,'reply',{known:false});this.state('NAS读取绑定已撤销；配对记录保留，其他客户端不受影响。');
 });}
 acknowledge(){return this.run(async g=>{
  const pending=await this.pendingAt(g);if(!pending||!this.snapshot||this.snapshot.coreId!==pending.core_id||this.snapshot.pendingId!==pending.id)throw Error('请先核对原别名的当前绑定');
  if(!this.confirm('确认已查看当前绑定？这里只保存本机核对记录，不重发原操作，也不把当前状态当作原写操作的历史回执。'))return;
  const observed=this.snapshot.known?{known:true,binding:this.snapshot.binding,principal_current:this.snapshot.principal_current}:{known:false};await this.finish(g,pending,'current_state',observed);this.state('当前绑定已核对，原操作和当前状态已留存；没有重发。');
 });}
 prepareExport(){return this.run(async g=>{
  this.clearDownload();const journal=await this.loadJournal(()=>this.current(g));this.current(g);const exported=exportJournal(journal,this.pin());
  if(!exported.receipts.length&&!exported.pending){this.state('当前设备没有可导出的本机NAS记录；其他设备的记录保持。');return;}
  const blob=new this.Blob([JSON.stringify(exported,null,2)+'\n'],{type:'application/json'});if(blob.size>128*1024)throw Error('导出过大');
  this.exportUrl=this.url.createObjectURL(blob);this.exported=exported;const link=this.$('nas-download');link.href=this.exportUrl;link.download='rustsimhub-nas-records.json';link.hidden=false;
  this.state(`已准备 ${exported.receipts.length} 条回执${exported.pending?'及1条待确认操作':''}。请下载并保存JSON；记录含设备与NAS公开标识，不含私钥或登录令牌。尚未清理任何记录。`);
 });}
 clearExport(){return this.run(async g=>{
  const exported=this.exported;if(!exported||!this.downloadClicked||!exported.receipts.length)throw Error('请先下载本次导出');
  if(!this.confirm('请确认刚下载的JSON文件已保存。仅清理文件中仍完全相同的本机回执；待确认操作、其他设备和新回执保留。不修改棒子或重新发送操作。浏览器不能验证文件是否已保存。'))return;
  const result=await this.clearJournal(exported,()=>this.current(g));this.current(g);this.clearDownload();this.state(`已清理 ${result.removed} 条已导出回执。待确认操作、关联回执和新增记录保留；没有向棒子发送请求。`);
 });}
}
