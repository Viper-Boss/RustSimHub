// SPDX-License-Identifier: AGPL-3.0-only
import { prove, unhex, hex } from './proof.js';
import { AudioLink } from './media.js';
import { CallTest, archiveEndedOperation } from './call-test.js';
import { ConfigurationIndicator } from './indicator.js';
import { NetworkPanel } from './network.js';
import { EventSync } from './event-sync.js';
import { SmsManager } from './sms-management.js';
const $=id=>document.getElementById(id);
const scopes=['status.read','sms.read','calls.read'];
let key, publicKey, token=null, busy=false, pending=null, lease=null;
let inboxCursor=null, callsCursor=null,smsView=0,smsPreview=0,callsView=0,callsPreview=0;
let historyCursor=null, historySources=[],historyView=0,historyPreview=0,historySourcesView=0;
const pageLoads={sms:{id:0,pending:false},calls:{id:0,pending:false},history:{id:0,pending:false}};
function pageLoad(kind,reset){
 const slot=pageLoads[kind];if(slot.pending&&!reset)throw Error('列表正在加载，请稍候');
 const id=++slot.id;slot.pending=true;
 return {current:()=>slot.id===id,finish:()=>{if(slot.id===id)slot.pending=false;}};
}
function invalidatePage(kind){pageLoads[kind].id++;pageLoads[kind].pending=false;}
let wifiTrial=null;
let audioReady=false;
let configurationOpen=true;
const configurationIndicator=new ConfigurationIndicator(command=>{
 const authToken=token;
 return async()=>{
  if(!authToken)return;
  const r=await fetch('/api/v2',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:2,id:crypto.randomUUID(),command:{...command,token:authToken}}),credentials:'omit',cache:'no-store',redirect:'error',keepalive:true,signal:AbortSignal.timeout(4000)});
  if(!r.ok)throw Error('indicator unavailable');
 };
});
const audioLink=new AudioLink($('call-audio'),(text,state)=>{$('audio-state').textContent=text;$('audio-stop').disabled=state==='closed';showAudio();});
const callTest=new CallTest({audio:audioLink,login:()=>token?Promise.resolve():login(),request:command=>auth(command),context:lease=>({token,lease}),getPending:()=>pending,setPending:async value=>{pending=value;await save('pending',value);showPending();},archivePending:archivePending,onLease:value=>{lease=value;showAudio();},notify:(text,running)=>{$('test-state').textContent=text;$('test-start').disabled=running||!audioReady;$('test-incoming-start').disabled=running||!audioReady;$('test-stop').disabled=!running;}});
function showAudio(){$('audio-start').disabled=!token||!lease||!audioReady||audioLink.state!=='closed';$('audio-stop').disabled=audioLink.state==='closed';}
let db;
let viewEpoch=0, actionGeneration=0;const requests=new Set();
function current(epoch){if(epoch!==viewEpoch)throw Error('会话已结束');}
const request=idb=>new Promise((resolve,reject)=>{idb.onsuccess=()=>resolve(idb.result);idb.onerror=()=>reject(Error('浏览器存储不可用'));});
async function stored(name){return request(db.transaction('state','readonly').objectStore('state').get(name));}
async function save(name,value){const tx=db.transaction('state','readwrite');tx.objectStore('state').put(value,name);await new Promise((resolve,reject)=>{tx.oncomplete=resolve;tx.onerror=tx.onabort=()=>reject(Error('浏览器存储不可用'));});}
async function archivePending(value){
 const receipts=await stored('ended-hangup-receipts')||[];
 if(!Array.isArray(receipts))throw Error('旧操作记录不可用，已停止测试');
 if(receipts.some(receipt=>receipt.id===value.id))return;
 if(receipts.length>=64)throw Error('旧操作记录已满，已停止测试');
 await save('ended-hangup-receipts',[...receipts,value]);
}
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
const eventSync=new EventSync({request:command=>auth(command),reload:{sms:()=>inboxPage(true),calls:()=>callsPage(true)}});
const smsManager=new SmsManager({document,request:command=>auth(command),reloadInbox:()=>inboxPage(true),confirm:message=>confirm(message)});
const notice=text=>{$('notice').textContent=text;};
const networkPanel=new NetworkPanel({document,request:command=>auth(command),getToken:()=>token,runTest:async command=>{
 const epoch=viewEpoch,authToken=token;if(!authToken)throw Error('请先登录');
 const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),35000);requests.add(controller);
 try{const r=await fetch('/api/network/v1/test',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({...command,token:authToken}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error'});current(epoch);if(!r.ok)throw Error(r.status===429?'测试正在运行或需间隔15秒':'请检查网络权限和设备状态');const result=await r.json();current(epoch);return result;}
 finally{clearTimeout(timer);requests.delete(controller);}
}});
function showPending(){
 $('pending').textContent=pending?JSON.stringify({type:pending.type,action:pending.action,operation_id:pending.id,state:pending.state,submit_started:pending.submit_started,prior_operation:pending.prior_operation},null,2):'没有待处理操作';
 $('pending-hint').textContent=!pending?'先在上方准备一笔操作，再在这里核对和提交。':pending.state==='submitted'?'这笔操作已提交，因此不能再次确认。要准备下一笔，请先点击“关闭已完成操作”。':pending.state==='cancelled'?'这笔操作已取消；请关闭它后再准备下一笔。':pending.submit_started?'这笔请求可能已经发出，只能查询结果；请勿重新提交。':!token?'请先在页面上方登录，再查询和核对当前操作。':'操作已准备。核对后点击“确认提交一次”；接通后在上方点击“连接声音”。';
 $('submit').disabled=!token||!pending||pending.submit_started||pending.state!=='prepared';$('query').disabled=!token||!pending;$('clear').disabled=!pending||!['submitted','cancelled'].includes(pending.state);
 $('archive-ended-call').disabled=!token||pending?.type!=='call'||!['dial','hangup'].includes(pending?.action)||!pending?.submit_started||!['unknown','submitting'].includes(pending?.state);
}
async function initialize(){
 db=await new Promise((resolve,reject)=>{const opening=indexedDB.open('rustsimhub-client-v1',1);opening.onupgradeneeded=()=>opening.result.createObjectStore('state');opening.onsuccess=()=>resolve(opening.result);opening.onerror=()=>reject(Error('浏览器存储不可用'));});
 key=await stored('key');if(!key){key=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);await save('key',key);}
 publicKey=new Uint8Array(await crypto.subtle.exportKey('raw',key.publicKey));
 $('client-key').textContent='浏览器公钥：'+hex(publicKey);$('pin').value=await stored('pin')||'';pending=await stored('pending')||null;showPending();
 const identity=await api({op:'identity'});$('first-setup').hidden=identity.owner_setup_available!==true;
 try{const response=await fetch('/api/media/capabilities',{credentials:'omit',cache:'no-store',redirect:'error'});const capability=await response.json();audioReady=response.ok&&capability.protocol===1&&capability.enabled===true;}catch{audioReady=false;}
 $('audio-section').hidden=!audioReady;showAudio();
 $('test-start').disabled=!audioReady;$('test-incoming-start').disabled=!audioReady;$('test-state').textContent=audioReady?'点击一次即可；首次使用请允许麦克风。':'此设备暂未开启浏览器音频。';
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
 const epoch=viewEpoch;const desired=$('write-scope').checked?[...scopes,'sms.send','calls.control','media.use']:scopes;
 const c=await api({op:'pair_challenge',public_key:Array.from(publicKey),kind:'client',scopes:desired},epoch);
 const signature=await proof(c,'pair',desired,epoch);current(epoch);
 const result=await api({op:'pair_proof',challenge_id:c.id,signature},epoch);
 notice('配对申请已提交。请由设备所有者核对浏览器公钥、权限和申请 ID 后批准：'+result.pairing_id);
}
async function login(){
 void audioLink.stop();const epoch=viewEpoch;token=null;lease=null;showAudio();const c=await api({op:'challenge',public_key:Array.from(publicKey)},epoch);const signature=await proof(c,'login',undefined,epoch);current(epoch);
 const r=await api({op:'authenticate',challenge_id:c.id,signature},epoch);current(epoch);if(!/^[a-f0-9]{64}$/.test(r.token))throw Error('登录响应无效');
 token=r.token;configurationOpen=true;void configurationIndicator.set(!document.hidden);networkPanel.start();notice('已登录；会话最长 15 分钟');await refresh();
}
function row(text,parent,onClick){const item=document.createElement('div');item.className='row';const label=document.createElement('span');label.textContent=text;item.append(label);if(onClick){const button=document.createElement('button');button.textContent='查看';button.onclick=()=>act(onClick);item.append(button);}parent.append(item);}
async function inboxPage(reset=false){
 const load=pageLoad('sms',reset);
 if(reset){smsView++;smsPreview++;inboxCursor=null;smsManager.select(null);$('sms-detail').textContent='';$('inbox').replaceChildren();$('more-inbox').disabled=true;}
 const view=smsView;
 try{const page=await auth({op:'inbox',after:inboxCursor,limit:16});if(!load.current()||view!==smsView)throw Error('短信列表已变化，请重新刷新');
  for(const s of page.messages)row(`${s.id} · SIM 代次 ${s.line}${s.binary?' · 二进制短信':''}${s.duplicate_candidate?' · 可能重复，待核对':''}`,$('inbox'),async()=>{const preview=++smsPreview;smsManager.select(null);$('sms-detail').textContent='';const detail=await auth({op:'sms_detail',sms_id:s.id});if(view!==smsView||preview!==smsPreview)return;$('sms-detail').textContent=JSON.stringify(detail,null,2);smsManager.select(s.id,detail);await auth({op:'notification_read',entity:'sms',record_id:s.id});});
  inboxCursor=page.cursor;$('more-inbox').disabled=page.messages.length<16;
 }finally{load.finish();}
}
async function callsPage(reset=false){
 const load=pageLoad('calls',reset);
 if(reset){callsView++;callsPreview++;callsCursor=null;$('call-detail').textContent='';$('calls').replaceChildren();$('call-id').replaceChildren();$('more-calls').disabled=true;}
 const view=callsView;
 try{const page=await auth({op:'calls',after:callsCursor,limit:16});if(!load.current()||view!==callsView)throw Error('通话列表已变化，请重新刷新');
  for(const s of page.calls){row(`${s.id} · ${s.state} · ${s.availability}`,$('calls'),async()=>{
   const preview=++callsPreview;$('call-detail').textContent='';
   const detail=await auth({op:'call_detail',call_id:s.id});if(view!==callsView||preview!==callsPreview)return;
   $('call-detail').textContent=JSON.stringify(detail,null,2);await auth({op:'notification_read',entity:'calls',record_id:s.id});
  });if(s.availability==='observed'&&['outgoing_pending','dialing','ringing_out','ringing_in','active','held','waiting'].includes(s.state)){const option=document.createElement('option');option.value=s.id;option.textContent=`${s.state} · ${s.id}`;$('call-id').append(option);}}
  callsCursor=page.cursor;$('more-calls').disabled=page.calls.length<16;
 }finally{load.finish();}
}
function clearWifi(){wifiTrial=null;$('wifi-ssid').value='';$('wifi-passphrase').value='';$('wifi-state').textContent='没有试连接';for(const id of ['wifi-status','wifi-confirm','wifi-cancel'])$(id).disabled=true;}
function showWifi(result){wifiTrial=result;const terminal=['saved','rolled_back','unknown'].includes(result.state);const names={starting:'正在开始',connecting:'正在连接',ready:'已连接，等待确认',saving:'正在保存',saved:'已保存',rolled_back:'已恢复原连接',unknown:'结果不明，请检查设备；不会自动重试'};$('wifi-state').textContent=`${names[result.state]||'未知状态'} · 剩余 ${result.expires_in_seconds} 秒`;$('wifi-status').disabled=!token;$('wifi-confirm').disabled=!token||result.state!=='ready';$('wifi-cancel').disabled=!token||terminal;}
async function beginWifi(){if(wifiTrial&&!['saved','rolled_back','unknown'].includes(wifiTrial.state))throw Error('请先查询或结束当前试连接');const ssid=$('wifi-ssid').value,passphrase=$('wifi-passphrase').value;if(!ssid||new TextEncoder().encode(ssid).length>32)throw Error('网络名称须为1至32字节');$('wifi-passphrase').value='';showWifi(await auth({op:'wifi_begin',idempotency_key:crypto.randomUUID(),ssid,passphrase}));}
async function wifiAction(op){if(!wifiTrial)throw Error('没有当前试连接');try{showWifi(await auth({op,trial_id:wifiTrial.trial_id}));if(op==='wifi_confirm'&&wifiTrial.state==='saved'){configurationOpen=false;void configurationIndicator.set(false);}}catch(error){clearWifi();throw error;}}
function clearHistory(){historyView++;historyPreview++;invalidatePage('history');historyCursor=null;$('history-list').replaceChildren();$('history-detail').textContent='';$('more-history').disabled=true;}
function historyTypes(){clearHistory();$('history-entity').replaceChildren();const source=historySources.find(s=>s.source===$('history-source').value);for(const entity of source?.entities||[]){const option=document.createElement('option');option.value=entity;option.textContent={sms:'短信',calls:'通话',contacts:'联系人'}[entity]||'未知类型';$('history-entity').append(option);}}
async function historyPage(reset=false){
 if(reset)clearHistory();const source=$('history-source').value,entity=$('history-entity').value;if(!source||!entity)return;
 const view=historyView,load=pageLoad('history',reset),selected=()=>view===historyView&&source===$('history-source').value&&entity===$('history-entity').value;
 try{const page=await auth({op:'history_page',source,entity,after:historyCursor,limit:16});if(!load.current()||!selected())throw Error('历史记录选择已变化，请重新刷新');
  for(const record of page.records)row(`${record.id} · ${record.deleted?'已删除的历史':'归档记录'}${record.user_state?.favorite?' · 收藏':''}${record.user_state?.marked_unread?' · 标记未读':''}`,$('history-list'),async()=>{
   const preview=++historyPreview;$('history-detail').textContent='';
   try{const detail=await auth({op:'history_detail',source,entity,record_id:record.id});if(selected()&&preview===historyPreview)$('history-detail').textContent=JSON.stringify(detail,null,2);}
   catch(error){if(selected()&&preview===historyPreview)clearHistory();throw error;}
  });historyCursor=page.cursor;$('more-history').disabled=page.records.length<16;
 }catch(error){if(load.current()&&selected())clearHistory();throw error;}finally{load.finish();}
}
async function loadHistory(){
 const generation=++historySourcesView;clearHistory();historySources=[];$('history-source').replaceChildren();$('history-entity').replaceChildren();
 const result=await auth({op:'history_sources'});if(generation!==historySourcesView)throw Error('历史来源已变化，请重新刷新');
 historySources=result.sources;for(const [index,source] of historySources.entries()){const option=document.createElement('option');option.value=source.source;option.textContent=`历史库 ${index+1} · ${source.source.slice(0,8)}`;$('history-source').append(option);}historyTypes();await historyPage(true);
}
async function refresh(){
 smsManager.reset();await smsManager.permissions();
 eventSync.reset();$('events').textContent='已重置补同步位置；可点击“补同步事件”追赶历史变化';
 const epoch=viewEpoch;const health=await auth({op:'health'});current(epoch);
 const line=await auth({op:'line_status'});current(epoch);
 async function optional(op){try{return await auth({op});}catch(error){current(epoch);return {available:false,reason:'暂不可用或未授予此项权限'};}}
 const call=await optional('call_status');const network=await optional('network_status');current(epoch);
 $('status').textContent=JSON.stringify({line,call,network},null,2);await inboxPage(true);await callsPage(true);
 $('history-section').hidden=health.history_read_api!==true;if(health.history_read_api===true)await loadHistory();else clearHistory();
 $('wifi-section').hidden=health.wifi_provisioning_api!==true;if(health.wifi_provisioning_api!==true)clearWifi();
 showAudio();
}
async function sync(){
 const epoch=viewEpoch;
 try{const result=await eventSync.run();current(epoch);$('events').textContent=`本次补同步 ${result.sms.count} 条短信事件、${result.calls.count} 条通话事件。${result.sms.more||result.calls.more?'可能仍有更多，请再次点击继续补同步。':'已追赶到本次查询末尾。'}`;}
 catch(error){if(epoch===viewEpoch)$('events').textContent='补同步未完成；失败部分保留原位置，可再次点击重试。若提示游标或事件库变化，请先刷新状态。';throw error;}
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
 if(pending.type==='call'&&pending.action==='hangup')void audioLink.stop();
 let r;if(pending.type==='sms')r=await auth({op:'submit_sms',operation_id:pending.id});
 else r=await auth({op:'submit_call',operation_id:pending.id,lease,urgent:pending.action==='hangup'});
 if(r.lease)lease=r.lease;showAudio();await query();
}
async function query(){const epoch=viewEpoch;if(!pending)return;const r=await auth({op:pending.type==='sms'?'sms_operation':'call_operation',operation_id:pending.id});if(r.lease)lease=r.lease;const operation=pending.type==='call'?r.operation:r;pending.state=operation.state;await save('pending',pending);current(epoch);showPending();showAudio();notice('操作状态：'+operation.state+'；没有重复提交');}
async function act(fn){if(busy)return;const generation=++actionGeneration,epoch=viewEpoch;busy=true;try{await fn();}catch(error){if(epoch===viewEpoch&&generation===actionGeneration)notice(error.message||'操作失败');}finally{if(generation===actionGeneration){busy=false;showPending();}}}
function bind(id,fn){$(id).onclick=()=>act(fn);}
bind('initialize-owner',initializeOwner);bind('pair',pair);bind('login',login);$('logout').onclick=()=>{viewEpoch++;actionGeneration++;busy=false;for(const controller of requests)controller.abort();requests.clear();token=null;lease=null;clearHistory();historySources=[];$('history-source').replaceChildren();$('history-entity').replaceChildren();$('history-section').hidden=true;for(const id of ['sms-detail','recipient','text','number','dtmf','events','setup-code']){const element=$(id);if('value' in element)element.value='';else element.textContent='';}$('inbox').replaceChildren();$('calls').replaceChildren();$('call-id').replaceChildren();$('status').textContent='未登录';$('lease').textContent='没有控制租约';notice('已退出；设备租约到期后清理呼叫');showPending();};bind('refresh',refresh);bind('sync',sync);
bind('sms-login',async()=>{await login();$('sms-management-state').textContent=smsManager.available?'已重新登录，请重新选择短信后管理。':'当前浏览器没有所有者管理权限，请使用已认领的所有者浏览器。';});
bind('network-login',login);const networkPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{networkPanel.stop();networkPreviousLogout();};
const syncPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{eventSync.reset();smsView++;smsPreview++;callsView++;callsPreview++;historySourcesView++;invalidatePage('sms');invalidatePage('calls');inboxCursor=null;callsCursor=null;$('call-detail').textContent='';$('more-inbox').disabled=true;$('more-calls').disabled=true;syncPreviousLogout();};
const smsPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{smsManager.reset();smsPreviousLogout();};
bind('refresh-history',loadHistory);bind('more-history',()=>historyPage());$('history-source').onchange=()=>{historyTypes();return act(()=>historyPage(true));};$('history-entity').onchange=()=>{clearHistory();return act(()=>historyPage(true));};
bind('wifi-begin',beginWifi);bind('wifi-status',()=>wifiAction('wifi_status'));bind('wifi-confirm',()=>wifiAction('wifi_confirm'));bind('wifi-cancel',()=>wifiAction('wifi_cancel'));const previousLogout=$('logout').onclick;$('logout').onclick=()=>{clearWifi();$('wifi-section').hidden=true;previousLogout();};
bind('more-inbox',()=>inboxPage());bind('more-calls',()=>callsPage());bind('prepare-sms',()=>prepare('sms','send'));for(const action of ['dial','accept','hangup','dtmf'])bind('prepare-'+action,()=>prepare('call',action));bind('submit',submit);bind('query',query);bind('clear',async()=>{if(!pending||!['submitted','cancelled'].includes(pending.state))throw Error('当前结果必须保留查询');pending=null;await save('pending',null);showPending();});
bind('archive-ended-call',async()=>{const epoch=viewEpoch,old=pending;await archiveEndedOperation({pending:old,request:command=>auth(command),archivePending,setPending:async value=>{current(epoch);await save('pending',value);current(epoch);pending=value;},current:()=>{current(epoch);if(pending?.id!==old?.id&&pending!==null)throw Error('当前操作已变化');}});showPending();notice('已核对呼叫结束并保存不确定操作记录；未重拨，设备原记录保持');});
setInterval(()=>{if(token&&lease&&!busy&&!callTest.running)act(async()=>{try{const r=await auth({op:'renew_call',lease});lease=r.lease;$('lease').textContent='控制租约已续期';showAudio();}catch(e){lease=null;void audioLink.stop();showAudio();$('lease').textContent='控制租约已失效；核心清理正在处理';throw e;}});},10000);
initialize().catch(error=>notice('初始化失败：'+error.message));

// Microphone permission may stay pending. It must not block hangup/logout.
$('audio-start').onclick=()=>{if(!token||!lease){notice('请先取得当前通话的控制租约');return;}const epoch=viewEpoch,authToken=token;void audioLink.start({token,lease}).catch(error=>{if(epoch===viewEpoch&&token===authToken)notice(error.message||'声音连接失败');}).finally(showAudio);};
$('audio-stop').onclick=()=>{void audioLink.stop();$('audio-state').textContent='声音已关闭；电话可在上方挂断';showAudio();};
$('audio-muted').onchange=()=>audioLink.mute($('audio-muted').checked);
bind('audio-play',()=>audioLink.play());
bind('test-start',()=>callTest.start());bind('test-incoming-start',()=>callTest.start('incoming'));$('test-stop').onclick=()=>{void callTest.stop();};
const audioPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{void callTest.stop(false);void audioLink.stop();$('audio-state').textContent='未连接声音';audioPreviousLogout();showAudio();};
addEventListener('pagehide',()=>{void callTest.stop(false);void audioLink.stop();});
const indicatorPreviousLogout=$('logout').onclick;
$('logout').onclick=()=>{configurationOpen=false;void configurationIndicator.set(false);indicatorPreviousLogout();};
bind('configuration-done',async()=>{configurationOpen=false;await configurationIndicator.set(false);notice('已结束本页配置');});
for(const panel of document.querySelectorAll('details'))panel.addEventListener('toggle',()=>{if(panel.open&&token){configurationOpen=true;void configurationIndicator.set(!document.hidden);}});
setInterval(()=>{if(token&&configurationOpen&&!document.hidden)void configurationIndicator.set(true);},10000);
document.addEventListener('visibilitychange',()=>{if(token)void configurationIndicator.set(configurationOpen&&!document.hidden);});
addEventListener('pagehide',()=>{configurationOpen=false;void configurationIndicator.set(false);});
addEventListener('pagehide',()=>networkPanel.stop());
addEventListener('pagehide',()=>eventSync.reset());
addEventListener('pagehide',()=>smsManager.reset());
