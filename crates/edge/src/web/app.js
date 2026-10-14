// SPDX-License-Identifier: AGPL-3.0-only
import { prove, unhex, hex } from './proof.js';
const $=id=>document.getElementById(id);
const scopes=['status.read','sms.read','calls.read'];
let key, publicKey, token=null, busy=false, pending=null, lease=null;
let inboxCursor=null, callsCursor=null, inboxEvents=null, callEvents=null;
let historyCursor=null, historySources=[];
let db;
let viewEpoch=0, actionGeneration=0;const requests=new Set();
function current(epoch){if(epoch!==viewEpoch)throw Error('会话已结束');}
const request=idb=>new Promise((resolve,reject)=>{idb.onsuccess=()=>resolve(idb.result);idb.onerror=()=>reject(Error('浏览器存储不可用'));});
async function stored(name){return request(db.transaction('state','readonly').objectStore('state').get(name));}
async function save(name,value){const tx=db.transaction('state','readwrite');tx.objectStore('state').put(value,name);await new Promise((resolve,reject)=>{tx.oncomplete=resolve;tx.onerror=tx.onabort=()=>reject(Error('浏览器存储不可用'));});}
async function api(command,epoch=viewEpoch){
 current(epoch);
 const id=crypto.randomUUID();const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),18000);requests.add(controller);
 try{const r=await fetch('/api/v2',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:2,id,command}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error'});
  if(!r.ok)throw Error('请求被拒绝或服务暂不可用');
  const value=await r.json();current(epoch);if(value.protocol!==2||value.id!==id)throw Error('响应不匹配');
  if(value.result?.error)throw Error('核心拒绝操作；请检查登录、权限和设备状态');
  return value.result;
 }finally{clearTimeout(timer);requests.delete(controller);}
}
function auth(command){if(!token)throw Error('请先登录');return api({...command,token});}
const notice=text=>{$('notice').textContent=text;};
function showPending(){
 $('pending').textContent=pending?JSON.stringify({type:pending.type,action:pending.action,operation_id:pending.id,state:pending.state,submit_started:pending.submit_started},null,2):'没有待处理操作';
 $('submit').disabled=!token||!pending||pending.submit_started||pending.state!=='prepared';$('query').disabled=!token||!pending;$('clear').disabled=!pending||!['submitted','cancelled'].includes(pending.state);
}
async function initialize(){
 db=await new Promise((resolve,reject)=>{const opening=indexedDB.open('rustsimhub-client-v1',1);opening.onupgradeneeded=()=>opening.result.createObjectStore('state');opening.onsuccess=()=>resolve(opening.result);opening.onerror=()=>reject(Error('浏览器存储不可用'));});
 key=await stored('key');if(!key){key=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);await save('key',key);}
 publicKey=new Uint8Array(await crypto.subtle.exportKey('raw',key.publicKey));
 $('client-key').textContent='浏览器公钥：'+hex(publicKey);$('pin').value=await stored('pin')||'';pending=await stored('pending')||null;showPending();
 const identity=await api({op:'identity'});$('first-setup').hidden=identity.owner_setup_available!==true;
}
async function proof(challenge,purpose,pairScopes,epoch=viewEpoch){
 let expectedKind=purpose==='bootstrap'?'owner':'client';
 if(purpose==='login'){
  if(!Array.isArray(challenge.transcript)||challenge.transcript.length>4096)throw Error('挑战格式无效');
  // This selects the expected signed shape; prove still verifies device pin,
  // exact transcript, client key and role before producing any signature.
  const claimed=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array(challenge.transcript))).kind;
  if(!['client','owner'].includes(claimed))throw Error('浏览器登录身份不支持');expectedKind=claimed;
 }
 const pin=unhex($('pin').value.trim());const signature=await prove(challenge,pin,publicKey,key.privateKey,purpose,pairScopes,expectedKind);current(epoch);
 await save('pin',hex(pin));current(epoch);return signature;
}
async function initializeOwner(){
 const epoch=viewEpoch;const setup_code=$('setup-code').value.trim().toLowerCase();
 if(!/^[a-f0-9]{64}$/.test(setup_code))throw Error('请填写可信维护入口提供的临时设置码');
 const c=await api({op:'setup_challenge',setup_code,public_key:Array.from(publicKey)},epoch);
 const signature=await proof(c,'bootstrap',undefined,epoch);current(epoch);
 $('setup-code').value='';
 const result=await api({op:'setup_finish',setup_code,challenge_id:c.id,signature},epoch);
 if(result.owner_initialized!==true)throw Error('首次设置结果尚未确认，请尝试正常登录；不要重复初始化');
 $('first-setup').hidden=true;
 notice('第一个所有者已初始化；此浏览器钥匙请妥善保留');await login();
}
async function pair(){
 const epoch=viewEpoch;const desired=$('write-scope').checked?[...scopes,'sms.send','calls.control']:scopes;
 const c=await api({op:'pair_challenge',public_key:Array.from(publicKey),kind:'client',scopes:desired},epoch);
 const signature=await proof(c,'pair',desired,epoch);current(epoch);
 const result=await api({op:'pair_proof',challenge_id:c.id,signature},epoch);
 notice('配对申请已提交。请由设备所有者核对浏览器公钥、权限和申请 ID 后批准：'+result.pairing_id);
}
async function login(){
 const epoch=viewEpoch;token=null;lease=null;const c=await api({op:'challenge',public_key:Array.from(publicKey)},epoch);const signature=await proof(c,'login',undefined,epoch);current(epoch);
 const r=await api({op:'authenticate',challenge_id:c.id,signature},epoch);current(epoch);if(!/^[a-f0-9]{64}$/.test(r.token))throw Error('登录响应无效');
 token=r.token;notice('已登录；会话最长 15 分钟');await refresh();
}
function row(text,parent,onClick){const item=document.createElement('div');item.className='row';const label=document.createElement('span');label.textContent=text;item.append(label);if(onClick){const button=document.createElement('button');button.textContent='查看';button.onclick=()=>act(onClick);item.append(button);}parent.append(item);}
async function inboxPage(reset=false){if(reset){inboxCursor=null;$('inbox').replaceChildren();}const page=await auth({op:'inbox',after:inboxCursor,limit:16});for(const s of page.messages)row(`${s.id} · SIM 代次 ${s.line}${s.binary?' · 二进制短信':''}`,$('inbox'),async()=>{$('sms-detail').textContent=JSON.stringify(await auth({op:'sms_detail',sms_id:s.id}),null,2);});inboxCursor=page.cursor;$('more-inbox').disabled=page.messages.length<16;}
async function callsPage(reset=false){if(reset){callsCursor=null;$('calls').replaceChildren();$('call-id').replaceChildren();}const page=await auth({op:'calls',after:callsCursor,limit:16});for(const s of page.calls){row(`${s.id} · ${s.state} · ${s.availability}`,$('calls'));if(s.state!=='ended'){const option=document.createElement('option');option.value=s.id;option.textContent=`${s.state} · ${s.id}`;$('call-id').append(option);}}callsCursor=page.cursor;$('more-calls').disabled=page.calls.length<16;}
function clearHistory(){historyCursor=null;$('history-list').replaceChildren();$('history-detail').textContent='';$('more-history').disabled=true;}
function historyTypes(){clearHistory();$('history-entity').replaceChildren();const source=historySources.find(s=>s.source===$('history-source').value);for(const entity of source?.entities||[]){const option=document.createElement('option');option.value=entity;option.textContent={sms:'短信',calls:'通话',contacts:'联系人'}[entity]||'未知类型';$('history-entity').append(option);}}
async function historyPage(reset=false){if(reset)clearHistory();const source=$('history-source').value,entity=$('history-entity').value;if(!source||!entity)return;try{const page=await auth({op:'history_page',source,entity,after:historyCursor,limit:16});for(const record of page.records)row(`${record.id} · ${record.deleted?'已删除的历史':'归档记录'}${record.user_state?.favorite?' · 收藏':''}${record.user_state?.marked_unread?' · 标记未读':''}`,$('history-list'),async()=>{try{$('history-detail').textContent=JSON.stringify(await auth({op:'history_detail',source,entity,record_id:record.id}),null,2);}catch(error){clearHistory();throw error;}});historyCursor=page.cursor;$('more-history').disabled=page.records.length<16;}catch(error){clearHistory();throw error;}}
async function loadHistory(){clearHistory();$('history-source').replaceChildren();const result=await auth({op:'history_sources'});historySources=result.sources;for(const [index,source] of historySources.entries()){const option=document.createElement('option');option.value=source.source;option.textContent=`历史库 ${index+1} · ${source.source.slice(0,8)}`;$('history-source').append(option);}historyTypes();await historyPage(true);}
async function refresh(){
 const epoch=viewEpoch;const health=await auth({op:'health'});current(epoch);
 const line=await auth({op:'line_status'});current(epoch);
 async function optional(op){try{return await auth({op});}catch(error){current(epoch);return {available:false,reason:'暂不可用或未授予此项权限'};}}
 const call=await optional('call_status');const network=await optional('network_status');current(epoch);
 $('status').textContent=JSON.stringify({line,call,network},null,2);await inboxPage(true);await callsPage(true);
 $('history-section').hidden=health.history_read_api!==true;if(health.history_read_api===true)await loadHistory();else clearHistory();
}
async function sync(){
 let messages=await auth({op:'inbox_events',after:inboxEvents,limit:32});let calls=await auth({op:'call_events',after:callEvents,limit:32});
 inboxEvents=messages.cursor;callEvents=calls.cursor;$('events').textContent=`本次补同步 ${messages.events.length} 条短信事件、${calls.events.length} 条通话事件。`;
 if(messages.events.length)await inboxPage(true);if(calls.events.length)await callsPage(true);
}
async function prepare(type,action){
 const epoch=viewEpoch;if(pending)throw Error('请先查询并关闭当前操作；不自动覆盖结果不明的提交');
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
 pending={type,action,id:operation.id,state:operation.state,submit_started:false};await save('pending',pending);current(epoch);showPending();notice('操作已准备；核对后确认提交一次');
}
async function submit(){
 if(!pending||pending.state!=='prepared'||pending.submit_started)throw Error('当前操作只能查询');
 // Commit the client marker BEFORE the request. EOF, reload or timeout never auto-resubmits.
 pending.submit_started=true;await save('pending',pending);showPending();
 let r;if(pending.type==='sms')r=await auth({op:'submit_sms',operation_id:pending.id});
 else r=await auth({op:'submit_call',operation_id:pending.id,lease,urgent:pending.action==='hangup'});
 if(r.lease)lease=r.lease;await query();
}
async function query(){const epoch=viewEpoch;if(!pending)return;const r=await auth({op:pending.type==='sms'?'sms_operation':'call_operation',operation_id:pending.id});if(r.lease)lease=r.lease;const operation=pending.type==='call'?r.operation:r;pending.state=operation.state;await save('pending',pending);current(epoch);showPending();notice('操作状态：'+operation.state+'；没有重复提交');}
async function act(fn){if(busy)return;const generation=++actionGeneration,epoch=viewEpoch;busy=true;try{await fn();}catch(error){if(epoch===viewEpoch&&generation===actionGeneration)notice(error.message||'操作失败');}finally{if(generation===actionGeneration){busy=false;showPending();}}}
function bind(id,fn){$(id).onclick=()=>act(fn);}
bind('initialize-owner',initializeOwner);bind('pair',pair);bind('login',login);$('logout').onclick=()=>{viewEpoch++;actionGeneration++;busy=false;for(const controller of requests)controller.abort();requests.clear();token=null;lease=null;clearHistory();historySources=[];$('history-source').replaceChildren();$('history-entity').replaceChildren();$('history-section').hidden=true;for(const id of ['sms-detail','recipient','text','number','dtmf','events','setup-code']){const element=$(id);if('value' in element)element.value='';else element.textContent='';}$('inbox').replaceChildren();$('calls').replaceChildren();$('call-id').replaceChildren();$('status').textContent='未登录';$('lease').textContent='没有控制租约';notice('已退出；设备租约到期后清理呼叫');showPending();};bind('refresh',refresh);bind('sync',sync);
bind('refresh-history',loadHistory);bind('more-history',()=>historyPage());$('history-source').onchange=()=>act(async()=>{historyTypes();await historyPage(true);});$('history-entity').onchange=()=>act(()=>historyPage(true));
bind('more-inbox',()=>inboxPage());bind('more-calls',()=>callsPage());bind('prepare-sms',()=>prepare('sms','send'));for(const action of ['dial','accept','hangup','dtmf'])bind('prepare-'+action,()=>prepare('call',action));bind('submit',submit);bind('query',query);bind('clear',async()=>{if(!pending||!['submitted','cancelled'].includes(pending.state))throw Error('当前结果必须保留查询');pending=null;await save('pending',null);showPending();});
setInterval(()=>{if(token&&lease&&!busy)act(async()=>{try{const r=await auth({op:'renew_call',lease});lease=r.lease;$('lease').textContent='控制租约已续期';}catch(e){lease=null;$('lease').textContent='控制租约已失效；核心清理正在处理';throw e;}});},10000);
initialize().catch(error=>notice('初始化失败：'+error.message));
