// SPDX-License-Identifier: AGPL-3.0-only
// Owner pairing approval. Core decides; the page only shows the exact request and echoes the
// same key, kind and scopes back. No retry: an unconfirmed approve/revoke is re-read, not resent.
export const SCOPES=['status.read','sms.read','sms.send','calls.read','calls.control','media.use','network.manage','clients.manage','update.manage','system.manage'];
const LABELS={'status.read':'读取状态','sms.read':'读取短信','sms.send':'发送短信','calls.read':'读取通话','calls.control':'控制通话','media.use':'通话声音','network.manage':'管理网络（含改 Wi-Fi）','clients.manage':'管理配对','update.manage':'管理更新','system.manage':'管理系统'};
const KINDS={client:'客户端',nas:'NAS',owner:'所有者'};
export function scopeNames(mask){if(!Number.isInteger(mask)||mask<1||mask>=1<<SCOPES.length)throw Error('权限格式无效');return SCOPES.filter((_,i)=>mask&(1<<i));}
export function scopeText(names){return names.map(n=>`${n}（${LABELS[n]}）`).join('、');}
export function keyHex(key){if(!Array.isArray(key)||key.length!==32||key.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('公钥格式无效');return key.map(b=>b.toString(16).padStart(2,'0')).join('');}
const same=(a,b)=>keyHex(a)===keyHex(b);
// Validate the whole finite reply before anything is rendered or becomes clickable.
export function pendingInput(rows){
 if(!Array.isArray(rows)||rows.length>16)throw Error('待批准列表格式无效');
 return rows.map(r=>{if(typeof r?.id!=='string'||!/^[0-9a-f]{64}$/.test(r.id)||!['client','nas'].includes(r.kind)||!Number.isInteger(r.expires_in_seconds)||r.expires_in_seconds<0||r.expires_in_seconds>300)throw Error('待批准申请格式无效');return {id:r.id,public_key:[...r.public_key],hex:keyHex(r.public_key),kind:r.kind,scopes:scopeNames(r.scopes),expires_in_seconds:r.expires_in_seconds};});
}
export function clientsInput(rows){
 if(!Array.isArray(rows)||rows.length>256)throw Error('已授权列表格式无效');
 return rows.map(r=>{if(typeof r?.id!=='string'||!r.id||r.id.length>128||!['owner','client','nas'].includes(r.kind)||typeof r.active!=='boolean'||!Number.isSafeInteger(r.generation)||r.generation<0)throw Error('已授权条目格式无效');return {id:r.id,public_key:[...r.public_key],hex:keyHex(r.public_key),kind:r.kind,scopes:scopeNames(r.scopes),generation:r.generation,active:r.active};});
}
export class ClientsPanel {
 constructor({document,request,confirm}){this.doc=document;this.request=request;this.confirm=confirm;this.$=id=>document.getElementById(id);this.generation=0;
  this.$('clients-open-pairing').onclick=()=>void this.openPairing();
  this.$('clients-refresh').onclick=()=>void this.refresh();
  this.reset();
 }
 reset(){this.generation++;this.busy=false;this.uncertain=null;for(const id of ['clients-pending','clients-list'])this.$(id).replaceChildren();this.$('clients-state').textContent='所有者在最近5分钟内登录后，可在这里允许新设备配对、批准或撤销。';this.buttons(false);}
 buttons(disabled){for(const id of ['clients-open-pairing','clients-refresh'])this.$(id).disabled=disabled;}
 current(g){if(g!==this.generation)throw Error('配对管理会话已结束');}
 state(text){this.$('clients-state').textContent=text;}
 async run(fn){if(this.busy)return false;const g=this.generation;this.busy=true;this.buttons(true);
  try{await fn(g);return true;}
  catch(error){if(g===this.generation)this.state(`${error.message||'操作失败'}；如果提示被拒绝，请用所有者重新登录（需最近5分钟内）后再试。`);return false;}
  finally{if(g===this.generation){this.busy=false;this.buttons(false);}}
 }
 openPairing(){return this.run(async g=>{
  if(!this.confirm('允许新设备配对5分钟？重新打开会作废尚未批准的申请。'))return;
  const r=await this.request({op:'open_pairing'});this.current(g);
  if(r?.ok!==true||r.expires_in_seconds!==300)throw Error('设备未确认开放配对');
  await this.load(g);this.state('已允许新设备配对5分钟。请在工具或浏览器上提交申请，然后点“刷新列表”核对。');
 });}
 refresh(){return this.run(g=>this.load(g));}
 async load(g){
  const pending=pendingInput(await this.request({op:'pending_pairings'}));this.current(g);
  const clients=clientsInput(await this.request({op:'clients'}));this.current(g);
  this.render(g,pending,clients);
  if(this.uncertain){const u=this.uncertain;const found=clients.find(c=>c.hex===u.hex);
   if(u.op==='approve'&&found?.active)this.state(`上次批准已生效：${found.hex}`);
   else if(u.op==='revoke'&&found&&!found.active)this.state(`上次撤销已生效：${found.hex}`);
   else this.state(u.op==='approve'?'上次批准未确认，且列表中没有该设备；不会自动重试，请让对方重新申请。':'上次撤销未确认，该设备仍显示为有效；请再次点撤销。');
   this.uncertain=null;}
  else this.state(`待批准 ${pending.length} 项 · 已授权 ${clients.filter(c=>c.active&&c.kind!=='owner').length} 项`);
 }
 row(parent,lines,label,onClick){const item=this.doc.createElement('div'),text=this.doc.createElement('span');item.className='row';text.className='mono';text.textContent=lines.join('\n');item.append(text);if(onClick){const button=this.doc.createElement('button');button.textContent=label;button.onclick=()=>void onClick();item.append(button);}parent.append(item);}
 render(g,pending,clients){
  this.$('clients-pending').replaceChildren();this.$('clients-list').replaceChildren();
  for(const p of pending)this.row(this.$('clients-pending'),[`申请编号：${p.id}`,`类型：${p.kind}（${KINDS[p.kind]}）`,`公钥：${p.hex}`,`权限：${scopeText(p.scopes)}`,`剩余 ${p.expires_in_seconds} 秒`],'核对后批准',()=>this.approve(g,p));
  for(const c of clients.filter(c=>c.active&&c.kind!=='owner'))this.row(this.$('clients-list'),[`类型：${c.kind}（${KINDS[c.kind]}） · 第 ${c.generation} 代`,`公钥：${c.hex}`,`权限：${scopeText(c.scopes)}`],'撤销',()=>this.revoke(g,c));
  if(!pending.length)this.row(this.$('clients-pending'),['没有待批准的申请']);
  if(!this.$('clients-list').children.length)this.row(this.$('clients-list'),['没有已授权的客户端']);
 }
 approve(view,p){return this.run(async g=>{
  if(view!==g)throw Error('列表已过期，请刷新');
  if(!this.confirm(`批准这台${KINDS[p.kind]}？\n申请编号：${p.id}\n公钥：${p.hex}\n权限：${scopeText(p.scopes)}\n请逐字核对公钥与对方显示的一致。`))return;
  let r;try{r=await this.request({op:'approve',pairing_id:p.id,public_key:p.public_key,kind:p.kind,scopes:p.scopes});}catch(error){this.current(g);this.uncertain={op:'approve',hex:p.hex};throw Error('批准结果未确认，请点“刷新列表”核对；不会自动重试');}
  this.current(g);
  if(!r||!same(r.public_key,p.public_key)||r.kind!==p.kind||r.active!==true||scopeNames(r.scopes).join()!==p.scopes.join()){this.uncertain={op:'approve',hex:p.hex};throw Error('批准回复与申请不一致，请点“刷新列表”核对');}
  await this.load(g);this.state(`已批准：${p.hex}`);
 });}
 revoke(view,c){return this.run(async g=>{
  if(view!==g)throw Error('列表已过期，请刷新');
  if(!this.confirm(`撤销这台${KINDS[c.kind]}？撤销后它立即退出，需要重新申请并批准。\n公钥：${c.hex}`))return;
  let r;try{r=await this.request({op:'revoke',public_key:c.public_key});}catch(error){this.current(g);this.uncertain={op:'revoke',hex:c.hex};throw Error('撤销结果未确认，请点“刷新列表”核对');}
  this.current(g);if(r?.ok!==true){this.uncertain={op:'revoke',hex:c.hex};throw Error('设备未确认撤销，请点“刷新列表”核对');}
  await this.load(g);this.state(`已撤销：${c.hex}`);
 });}
}
