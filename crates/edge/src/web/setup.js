// SPDX-License-Identifier: AGPL-3.0-only
// First-setup page on the open hotspot (plain http, no WebCrypto): create the Owner from a
// username + password derived key, then reuse the AP Wi-Fi controller to try and save Wi-Fi.
// The password and derived key stay in memory on this page only.
import {ApConfigurator,pendingInput} from './ap-wifi.js';
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
function storage(){try{return globalThis.localStorage??null;}catch{return null;}}
const KEY='rustsimhub-setup-ap-wifi-pending';
const pending={
 load:async()=>{const s=storage();const v=s?.getItem(KEY);return v?JSON.parse(v):null;},
 save:async(value,expected)=>{const s=storage();const old=pendingInput(s?.getItem(KEY)?JSON.parse(s.getItem(KEY)):null);
  if((old?.id??null)!==(expected?.id??null)||(old?.revision??null)!==(expected?.revision??null))throw Error('配网记录已在另一页面更新，请重新查询');
  if(s){if(value===null)s.removeItem(KEY);else s.setItem(KEY,JSON.stringify(value));}},
 archive:async()=>{},
};
const wifi=new ApConfigurator({document,env:globalThis,crypto:pageCrypto,request:command=>{if(!token)throw Error('请先设置管理员账号');return api({...command,token});},
 login,getToken:()=>token,blocked:()=>busy,loadPending:pending.load,savePending:pending.save,archivePending:pending.archive});
let listing=false,refresher=null;
// Rebuilds the list (keeping the current choice); also run every 20 s while this step is shown.
async function networks(){
 const list=$('setup-networks'),note=$('setup-networks-note');
 if(listing||!token)return;listing=true;$('setup-networks-refresh').disabled=true;
 try{
  const v=await api({op:'wifi_networks',token}),chosen=list.value;
  const placeholder=list.children[0];list.replaceChildren(placeholder);
  for(const n of (v.networks||[]).filter(n=>n.selectable).slice(0,64)){const o=document.createElement('option');o.value=n.ssid;o.textContent=`${n.ssid}（信号 ${n.strength}%）`;list.append(o);}
  if(chosen)list.value=chosen;
  const when=new Date().toLocaleTimeString();
  note.textContent=(v.source==='before_setup_ap'?'列表是开启热点前扫描到的（热点开着时设备无法重新扫描）':'列表来自设备最近一次扫描')+`，${when} 更新。`+((v.networks||[]).length?'':'没有找到时请手动输入名称。');
 }catch{note.textContent='暂时无法获取附近的 Wi-Fi，请手动输入名称。';}
 finally{listing=false;$('setup-networks-refresh').disabled=false;}
}
function watchNetworks(){
 $('setup-networks').onchange=()=>{const v=$('setup-networks').value;if(v){$('ap-wifi-ssid').value=v;$('ap-wifi-passphrase').focus();}};
 $('setup-networks-refresh').onclick=()=>void networks();
 if(!refresher)refresher=setInterval(()=>{if(!$('setup-wifi').hidden)void networks();},20000);
 refresher?.unref?.();
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
  $('setup-account').hidden=true;$('setup-wifi').hidden=false;
  $('setup-notice').textContent=create?'管理员已创建。请选择要连接的 Wi-Fi。':'已登录。请选择要连接的 Wi-Fi。';
  watchNetworks();await networks();await wifi.open();
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
