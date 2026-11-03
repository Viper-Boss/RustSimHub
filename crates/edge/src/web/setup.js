// SPDX-License-Identifier: AGPL-3.0-only
// First-setup page on the open hotspot (plain http, no WebCrypto): create the Owner from a
// username + password derived key, then reuse the AP Wi-Fi controller to try and save Wi-Fi.
// The password and derived key stay in memory on this page only.
import {ApConfigurator,pendingInput,scanPendingInput} from './ap-wifi.js';
import {deriveSeed,publicKey,sign,sha256,normalizeUsername,normalizePassword} from './password.js';
const $=id=>document.getElementById(id);
const hex=b=>Array.from(b,v=>v.toString(16).padStart(2,'0')).join('');
let device=null,seed=null,token=null,busy=false;
function uuid(){const b=crypto.getRandomValues(new Uint8Array(16));b[6]=b[6]&15|64;b[8]=b[8]&63|128;const h=hex(b);return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;}
// crypto.randomUUID/subtle are secure-context only; the controller needs just these two.
const pageCrypto={randomUUID:uuid,subtle:{digest:async(_algorithm,data)=>sha256(new Uint8Array(data)).buffer}};
async function api(command){
 const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),18000);
 try{
  const id=uuid();
  const r=await fetch('/api/v2',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:2,id,command}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error'});
  if(!r.ok)throw Error('请求被拒绝或服务暂不可用');
  // Same envelope as the https app: {protocol, id, result}; Core errors arrive inside result.
  const v=await r.json();if(v?.protocol!==2||v.id!==id)throw Error('设备应答不匹配');
  if(!v.result||typeof v.result!=='object'||'error' in v.result)throw Error('设备拒绝了请求');
  return v.result;
 }finally{clearTimeout(timer);}
}
const bytes=(v,n)=>{if(!Array.isArray(v)||(n&&v.length!==n)||v.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('设备应答格式无效');return new Uint8Array(v);};
/// Same transcript binding the https app checks, minus the device signature (no WebCrypto here).
function transcript(c,purpose){
 if(!c||!/^[a-f0-9]{64}$/.test(c.id)||!Array.isArray(c.transcript)||c.transcript.length<1||c.transcript.length>4096)throw Error('挑战格式无效');
 const raw=bytes(c.transcript),t=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(raw));
 if(t.domain!=='RustSimHub.Auth.v1'||t.purpose!==purpose||t.kind!=='owner'||t.scopes!==1023||t.challenge_id!==c.id
  ||hex(bytes(t.device_public_key,32))!==hex(device)||hex(bytes(t.client_public_key,32))!==hex(publicKey(seed))
  ||(purpose==='bootstrap'?t.generation!==0:t.generation<1))throw Error('设备挑战与本机身份不符');
 return raw;
}
async function login(){
 const c=await api({op:'challenge',public_key:Array.from(publicKey(seed))});
 const r=await api({op:'authenticate',challenge_id:c.id,signature:Array.from(sign(seed,transcript(c,'login')))});
 if(!/^[a-f0-9]{64}$/.test(r.token))throw Error('登录应答无效');token=r.token;
}
function storage(){
 try{const s=globalThis.localStorage;if(s)return s;}catch{}
 throw Error('浏览器存储不可用，无法保存配网进度，请改用完整浏览器');
}
function stored(s,key){
 try{const raw=s.getItem(key);return raw===null?null:JSON.parse(raw);}
 catch{throw Error('本机配网记录无法读取，请保留原进度后查询');}
}
function writeStored(s,key,raw){
 if(raw===null)s.removeItem(key);else s.setItem(key,raw);
 if(s.getItem(key)!==raw)throw Error('配网记录未能保存，请先查询原进度，不会提交网络变更');
}
const KEY='rustsimhub-setup-ap-wifi-pending';
const receiptsKey=()=>KEY+':receipts:'+hex(device);
let pendingKey=KEY;
function selectPendingNamespace(created){
 const s=storage(),scoped=KEY+':'+hex(device);
 // A newly proved first Owner belongs to this device identity. Keep a legacy
 // record intact rather than attaching an earlier device's operation to it.
 // Existing Owners without a scoped slot still recover their legacy record.
 pendingKey=!created&&s.getItem(scoped)===null&&s.getItem(KEY)!==null?KEY:scoped;
 // Retain a null scoped slot after completion so later logins never fall back
 // to an unrelated legacy record. Never replace an existing scoped operation.
 if(pendingKey===scoped&&s.getItem(scoped)===null)writeStored(s,scoped,'null');
}
const pending={
 load:async()=>stored(storage(),pendingKey),
 save:async(value,expected)=>{const s=storage();const old=pendingInput(stored(s,pendingKey));
  if((old?.id??null)!==(expected?.id??null)||(old?.revision??null)!==(expected?.revision??null))throw Error('配网记录已在另一页面更新，请重新查询');
  writeStored(s,pendingKey,value===null&&pendingKey===KEY?null:JSON.stringify(value));},
 archive:async(value)=>{
  const s=storage(),raw=stored(s,receiptsKey()),receipts=raw===null?[]:raw;
  const valid=r=>r&&Object.keys(r).length===2&&Object.hasOwn(r,'record')&&Object.hasOwn(r,'state')&&['saved','closed'].includes(r.state)&&pendingInput(r.record)!==null;
  if(!Array.isArray(receipts)||receipts.length>8||!receipts.every(valid)||new Set(receipts.map(r=>r.record.id)).size!==receipts.length||!valid(value))throw Error('配网回执无法核对，当前操作仍保留');
  const previous=receipts.find(r=>r.record.id===value.record.id);
  if(previous&&JSON.stringify(previous)!==JSON.stringify(value))throw Error('配网回执冲突，当前操作仍保留');
  if(!previous)writeStored(s,receiptsKey(),JSON.stringify([...receipts,value].slice(-8)));
 },
};
const scanKey=()=>KEY+':scan:'+hex(device);
const scan={
 domain:()=>hex(device),button:$('setup-networks-rescan'),note:$('setup-networks-note'),
 load:async()=>stored(storage(),scanKey()),
 save:async(value,expected)=>{const s=storage();
  const old=scanPendingInput(stored(s,scanKey()),hex(device));
  if(JSON.stringify(old)!==JSON.stringify(expected))throw Error('扫描记录已在另一页面更新');
  writeStored(s,scanKey(),JSON.stringify(value));},
 snapshot:value=>showNetworks(value.networks.map(n=>({...n,selectable:n.security==='wpa2_personal'}))),
};
const wifi=new ApConfigurator({document,env:globalThis,crypto:pageCrypto,request:command=>{if(!token)throw Error('请先设置管理员账号');return api({...command,token});},
 login,getToken:()=>token,blocked:()=>busy||listing,loadPending:pending.load,savePending:pending.save,archivePending:pending.archive,scan});
let listing=false;
function showNetworks(rows){
 const list=$('setup-networks'),chosen=list.value,placeholder=list.children[0];
 list.replaceChildren(placeholder);
 const choices=rows.filter(n=>n.selectable).slice(0,64);
 for(const n of choices){const o=document.createElement('option');o.value=n.ssid;o.textContent=`${n.ssid}（信号 ${n.strength}%）`;list.append(o);}
 list.value=choices.some(n=>n.ssid===chosen)?chosen:'';
 return choices;
}
// Cache read on login only; never periodically overwrite a newly completed scan.
async function networks(){
 const list=$('setup-networks'),note=$('setup-networks-note');
 if(listing||!token||wifi.working||wifi.scanRecord)return;listing=true;$('setup-networks-refresh').disabled=true;scan.button.disabled=true;
 try{
  const v=await api({op:'wifi_networks',token}),choices=showNetworks(v.networks||[]);
  const when=new Date().toLocaleTimeString();
  const age=Number.isFinite(v.snapshot_age_seconds)&&v.snapshot_age_seconds>=0?(v.source==='before_setup_ap_scan'?`，约${Math.floor(v.snapshot_age_seconds)}秒前完成`:`，约${Math.floor(v.snapshot_age_seconds/60)}分钟前的缓存`):'';
  const origin=v.source==='before_setup_ap_scan'?'开启热点前已完成一次主动扫描'+age:v.source==='before_setup_ap'?'列表是开启热点前的缓存快照'+age:'列表来自设备扫描缓存';
  note.textContent=origin+`，${when} 读取。热点开启期间只读取列表，不会重新扫描。`+(choices.length?'':'没有可选的 WPA2 网络，请手动输入名称。');
 }catch{note.textContent='暂时无法获取附近的 Wi-Fi，请手动输入名称。';}
 finally{listing=false;$('setup-networks-refresh').disabled=false;wifi.render();}
}
function watchNetworks(){
 $('setup-networks').onchange=()=>{const v=$('setup-networks').value;if(v){$('ap-wifi-ssid').value=v;$('ap-wifi-passphrase').focus();}};
 $('setup-networks-refresh').onclick=()=>void networks();
}
async function account(create){
 if(busy)return;busy=true;$('setup-account-go').disabled=true;
 try{
  const name=normalizeUsername($('setup-username').value),password=normalizePassword($('setup-password').value);
  if(create&&password!==$('setup-confirm').value.normalize('NFC'))throw Error('两次输入的密码不一致');
  $('setup-notice').textContent='正在生成管理员身份（手机上可能需要几秒）…';
  await new Promise(r=>setTimeout(r,30));
  seed=deriveSeed(device,name,password);
  $('setup-password').value='';$('setup-confirm').value='';
  if(create){
   const c=await api({op:'setup_open_challenge',public_key:Array.from(publicKey(seed))});
   await api({op:'setup_open_finish',challenge_id:c.id,signature:Array.from(sign(seed,transcript(c,'bootstrap')))});
  }
  await login();
  selectPendingNamespace(create);
  $('setup-notice').textContent='正在读取 Wi-Fi 缓存和已有配网进度…';
  watchNetworks();await networks();
  // Account authentication/list loading is finished. The shared controller
  // must be allowed to read its pending operation before the Wi-Fi step opens.
  busy=false;await wifi.open();
  $('setup-account').hidden=true;$('setup-wifi').hidden=false;
  $('setup-notice').textContent=create?'管理员已创建。请选择要连接的 Wi-Fi。':'已登录。请选择要连接的 Wi-Fi。';
 }catch(e){seed=null;token=null;$('setup-notice').textContent=e.message||'操作失败，请重试';}
 finally{busy=false;$('setup-account-go').disabled=false;}
}
async function start(){
 try{
  const v=await api({op:'identity'});device=bytes(v.public_key,32);
  const create=v.owner_setup_available===true;
  $('setup-confirm-label').hidden=!create;
  $('setup-account-title').textContent=create?'第一步：设置管理员账号':'第一步：登录管理员账号';
  $('setup-account-go').textContent=create?'创建管理员并继续':'登录并继续';
  $('setup-account-go').onclick=()=>account(create);
  $('setup-account').hidden=false;
  $('setup-notice').textContent=create?'这台设备还没有管理员。':'这台设备已经有管理员，请用原来的用户名和密码登录。';
 }catch{$('setup-notice').textContent='无法连接设备，请确认已连上 RustSimHub 热点后刷新本页。';}
}
void start();
