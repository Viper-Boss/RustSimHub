// SPDX-License-Identifier: AGPL-3.0-only
import { prove, unhex, hex } from './proof.js';
const $=id=>document.getElementById(id);
const scopes=['status.read','sms.read','calls.read'];
let key, publicKey, token=null, busy=false, pending=null, lease=null;
let inboxCursor=null, callsCursor=null, inboxEvents=null, callEvents=null;
let db;
const request=idb=>new Promise((resolve,reject)=>{idb.onsuccess=()=>resolve(idb.result);idb.onerror=()=>reject(Error('浏览器存储不可用'));});
async function stored(name){return request(db.transaction('state','readonly').objectStore('state').get(name));}
async function save(name,value){const tx=db.transaction('state','readwrite');tx.objectStore('state').put(value,name);await new Promise((resolve,reject)=>{tx.oncomplete=resolve;tx.onerror=tx.onabort=()=>reject(Error('浏览器存储不可用'));});}
async function api(command){
 const id=crypto.randomUUID();const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),18000);
 try{const r=await fetch('/api/v2',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:2,id,command}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error'});
  if(!r.ok)throw Error('请求被拒绝或服务暂不可用');
  const value=await r.json();if(value.protocol!==2||value.id!==id)throw Error('响应不匹配');
  if(value.result?.error)throw Error('核心拒绝操作；请检查登录、权限和设备状态');
  return value.result;
 }finally{clearTimeout(timer);}
}
function auth(command){if(!token)throw Error('请先登录');return api({...command,token});}
const notice=text=>{$('notice').textContent=text;};
function showPending(){
 $('pending').textContent=pending?JSON.stringify({type:pending.type,action:pending.action,operation_id:pending.id,state:pending.state,submit_started:pending.submit_started},null,2):'没有待处理操作';
 $('submit').disabled=!pending||pending.submit_started||pending.state!=='prepared';$('query').disabled=!pending;$('clear').disabled=!pending||!['submitted','cancelled'].includes(pending.state);
}
async function initialize(){
 db=await new Promise((resolve,reject)=>{const opening=indexedDB.open('rustsimhub-client-v1',1);opening.onupgradeneeded=()=>opening.result.createObjectStore('state');opening.onsuccess=()=>resolve(opening.result);opening.onerror=()=>reject(Error('浏览器存储不可用'));});
 key=await stored('key');if(!key){key=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);await save('key',key);}
 publicKey=new Uint8Array(await crypto.subtle.exportKey('raw',key.publicKey));
 $('client-key').textContent='浏览器公钥：'+hex(publicKey);$('pin').value=await stored('pin')||'';pending=await stored('pending')||null;showPending();
}
async function proof(challenge,purpose,pairScopes){
 const pin=unhex($('pin').value.trim());const signature=await prove(challenge,pin,publicKey,key.privateKey,purpose,pairScopes);
 await save('pin',hex(pin));return signature;
}
async function pair(){
 const desired=$('write-scope').checked?[...scopes,'sms.send','calls.control']:scopes;
 const c=await api({op:'pair_challenge',public_key:Array.from(publicKey),kind:'client',scopes:desired});
 const signature=await proof(c,'pair',desired);
 const result=await api({op:'pair_proof',challenge_id:c.id,signature});
 notice('配对申请已提交。请由设备所有者核对浏览器公钥、权限和申请 ID 后批准：'+result.pairing_id);
}
async function login(){
 token=null;lease=null;const c=await api({op:'challenge',public_key:Array.from(publicKey)});const signature=await proof(c,'login');
 const r=await api({op:'authenticate',challenge_id:c.id,signature});if(!/^[a-f0-9]{64}$/.test(r.token))throw Error('登录响应无效');
 token=r.token;notice('已登录；会话最长 15 分钟');await refresh();
}
function row(text,parent,onClick){const item=document.createElement('div');item.className='row';const label=document.createElement('span');label.textContent=text;item.append(label);if(onClick){const button=document.createElement('button');button.textContent='查看';button.onclick=()=>act(onClick);item.append(button);}parent.append(item);}
async function inboxPage(reset=false){if(reset){inboxCursor=null;$('inbox').replaceChildren();}const page=await auth({op:'inbox',after:inboxCursor,limit:16});for(const s of page.messages)row(`${s.id} · SIM 代次 ${s.line}${s.binary?' · 二进制短信':''}`,$('inbox'),async()=>{$('sms-detail').textContent=JSON.stringify(await auth({op:'sms_detail',sms_id:s.id}),null,2);});inboxCursor=page.cursor;$('more-inbox').disabled=page.messages.length<16;}
async function callsPage(reset=false){if(reset){callsCursor=null;$('calls').replaceChildren();$('call-id').replaceChildren();}const page=await auth({op:'calls',after:callsCursor,limit:16});for(const s of page.calls){row(`${s.id} · ${s.state} · ${s.availability}`,$('calls'));if(s.state!=='ended'){const option=document.createElement('option');option.value=s.id;option.textContent=`${s.state} · ${s.id}`;$('call-id').append(option);}}callsCursor=page.cursor;$('more-calls').disabled=page.calls.length<16;}
async function refresh(){const line=await auth({op:'line_status'});const call=await auth({op:'call_status'});const network=await auth({op:'network_status'});$('status').textContent=JSON.stringify({line,call,network},null,2);await inboxPage(true);await callsPage(true);}
async function sync(){
 let messages=await auth({op:'inbox_events',after:inboxEvents,limit:32});let calls=await auth({op:'call_events',after:callEvents,limit:32});
 inboxEvents=messages.cursor;callEvents=calls.cursor;$('events').textContent=`本次补同步 ${messages.events.length} 条短信事件、${calls.events.length} 条通话事件。`;
 if(messages.events.length)await inboxPage(true);if(calls.events.length)await callsPage(true);
}
async function prepare(type,action){
 if(pending)throw Error('请先查询并关闭当前操作；不自动覆盖结果不明的提交');
 const line=await auth({op:'line_status'});if(line.status!=='ready')throw Error('SIM 尚未就绪');
 const idempotency_key=crypto.randomUUID();let operation;
 if(type==='sms'){
  const recipient=$('recipient').value.trim(),text=$('text').value;if(!recipient||!text)throw Error('请输入号码和内容');
  operation=await auth({op:'prepare_sms',line_epoch:line.line,idempotency_key,recipient,text});
 }else{
  const call_id=action==='dial'?null:$('call-id').value;const payload=action==='dial'?$('number').value.trim():action==='dtmf'?$('dtmf').value:'';
  if(action!=='dial'&&!call_id)throw Error('请选择呼叫');
  const result=await auth({op:'prepare_call',line_epoch:line.line,idempotency_key,action,call_id,payload});operation=result.operation;
 }
 pending={type,action,id:operation.id,state:operation.state,submit_started:false};await save('pending',pending);showPending();notice('操作已准备；核对后确认提交一次');
}
async function submit(){
 if(!pending||pending.state!=='prepared'||pending.submit_started)throw Error('当前操作只能查询');
 // Commit the client marker BEFORE the request. EOF, reload or timeout never auto-resubmits.
 pending.submit_started=true;await save('pending',pending);showPending();
 let r;if(pending.type==='sms')r=await auth({op:'submit_sms',operation_id:pending.id});
 else r=await auth({op:'submit_call',operation_id:pending.id,lease,urgent:pending.action==='hangup'});
 if(r.lease)lease=r.lease;await query();
}
async function query(){if(!pending)return;const r=await auth({op:pending.type==='sms'?'sms_operation':'call_operation',operation_id:pending.id});if(r.lease)lease=r.lease;const operation=pending.type==='call'?r.operation:r;pending.state=operation.state;await save('pending',pending);showPending();notice('操作状态：'+operation.state+'；没有重复提交');}
async function act(fn){if(busy)return;busy=true;try{await fn();}catch(error){notice(error.message||'操作失败');}finally{busy=false;showPending();}}
function bind(id,fn){$(id).onclick=()=>act(fn);}
bind('pair',pair);bind('login',login);bind('logout',async()=>{token=null;lease=null;$('sms-detail').textContent='';$('inbox').replaceChildren();$('calls').replaceChildren();$('status').textContent='未登录';notice('已退出；设备租约到期后清理呼叫');});bind('refresh',refresh);bind('sync',sync);
bind('more-inbox',()=>inboxPage());bind('more-calls',()=>callsPage());bind('prepare-sms',()=>prepare('sms','send'));for(const action of ['dial','accept','hangup','dtmf'])bind('prepare-'+action,()=>prepare('call',action));bind('submit',submit);bind('query',query);bind('clear',async()=>{if(!pending||!['submitted','cancelled'].includes(pending.state))throw Error('当前结果必须保留查询');pending=null;await save('pending',null);showPending();});
setInterval(()=>{if(token&&lease&&!busy)act(async()=>{try{const r=await auth({op:'renew_call',lease});lease=r.lease;$('lease').textContent='控制租约已续期';}catch(e){lease=null;$('lease').textContent='控制租约已失效；核心清理正在处理';throw e;}});},10000);
initialize().catch(error=>notice('初始化失败：'+error.message));
