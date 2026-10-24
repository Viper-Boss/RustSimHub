// SPDX-License-Identifier: AGPL-3.0-only
import { prove, unhex, hex } from './proof.js';
import { AudioLink } from './media.js';
import { CallTest, archiveEndedOperation } from './call-test.js';
import { ConfigurationIndicator } from './indicator.js';
import { NetworkPanel } from './network.js';
import { EventSync } from './event-sync.js';
import { SmsManager, smsListText, smsDetailText } from './sms-management.js';
import { DailyPhone } from './phone.js';
import { SessionKeeper } from './session.js';
const $=id=>document.getElementById(id);
const scopes=['status.read','sms.read','calls.read'];
let key, publicKey, token=null, busy=false, pending=null, lease=null;
let dailyPhone=null;
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
let wifiCatalogueGeneration=0;
let audioReady=false;
let testWorking=false,testGeneration=0,testTask=null,testResult=null;
let configurationOpen=false;
function configurationVisible(){return Array.from(document.querySelectorAll('details[data-configuration]')).some(panel=>panel.open);}
const configurationIndicator=new ConfigurationIndicator(command=>{
 const authToken=token;
 return async()=>{
  if(!authToken)return;
  const r=await fetch('/api/v2',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({protocol:2,id:crypto.randomUUID(),command:{...command,token:authToken}}),credentials:'omit',cache:'no-store',redirect:'error',keepalive:true,signal:AbortSignal.timeout(4000)});
  if(!r.ok)throw Error('indicator unavailable');
 };
});
const audioLink=new AudioLink($('call-audio'),(text,state)=>{$('audio-state').textContent=text;$('audio-stop').disabled=state==='closed';showAudio();});
const callTest=new CallTest({audio:audioLink,login:()=>token?Promise.resolve():login(),request:command=>auth(command),context:lease=>({token,lease}),getPending:()=>pending,setPending:async value=>{pending=value;await save('pending',value);showPending();},archivePending:archivePending,onLease:value=>{lease=value;showAudio();},notify:(text,running)=>{$('test-state').textContent=text;testResult=text;showTestControls();}});
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
const sessionKeeper=new SessionKeeper({env:globalThis,token:()=>token,apply:(old,next)=>{if(token!==old)throw Error('登录会话已变化');audioLink.updateAuthentication(old,next);token=next;},renew:async(old,active)=>{
 const epoch=viewEpoch;active();const c=await api({op:'session_challenge',token:old},epoch);active();
 const signature=await proof(c,'session_renew',undefined,epoch,old);active();
 const result=await api({op:'renew_session',token:old,challenge_id:c.id,signature},epoch);current(epoch);active();return result;
}});
dailyPhone=new DailyPhone({document,env:globalThis,ensureSession:()=>sessionKeeper.ensure(),audio:audioLink,audioReady:()=>audioReady,context:value=>({token,lease:value}),request:command=>auth(command),getPending:()=>pending,setPending:async value=>{const epoch=viewEpoch;pending=value;await save('pending',value);current(epoch);showPending();},archivePending,archiveKnown:async value=>{const epoch=viewEpoch,receipts=await stored('phone-operation-receipts')||[];current(epoch);if(!Array.isArray(receipts))throw Error('电话回执不可用');await save('phone-operation-receipts',[...receipts.filter(x=>x.id!==value.id),value].slice(-64));current(epoch);},setLease:value=>{lease=value;showAudio();},blocked:()=>busy||callTest.running||!!lease&&!dailyPhone?.managing});
const networkPanel=new NetworkPanel({document,request:command=>auth(command),getToken:()=>token,runTest:async command=>{
 const epoch=viewEpoch,authToken=token;if(!authToken)throw Error('请先登录');
 const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),35000);requests.add(controller);
 try{const r=await fetch('/api/network/v1/test',{method:'POST',headers:{'Content-Type':'application/json','X-RustSimHub-Request':'2'},body:JSON.stringify({...command,token:authToken}),signal:controller.signal,credentials:'omit',cache:'no-store',redirect:'error'});current(epoch);if(!r.ok)throw Error(r.status===429?'测试正在运行或需间隔15秒':'请检查网络权限和设备状态');const result=await r.json();current(epoch);return result;}
 finally{clearTimeout(timer);requests.delete(controller);}
}});
function showPending(){
 showTestControls();
 $('pending').textContent=pending?JSON.stringify({type:pending.type,action:pending.action,operation_id:pending.id,state:pending.state,submit_started:pending.submit_started,prior_operation:pending.prior_operation},null,2):'没有待处理操作';
 $('pending-hint').textContent=!pending?'先在上方准备一笔操作，再在这里核对和提交。':pending.state==='submitted'?'这笔操作已提交，因此不能再次确认。要准备下一笔，请先点击“关闭已完成操作”。':pending.state==='cancelled'?'这笔操作已取消；请关闭它后再准备下一笔。':pending.submit_started?'这笔请求可能已经发出，只能查询结果；请勿重新提交。':!token?'请先在页面上方登录，再查询和核对当前操作。':'操作已准备。核对后点击“确认提交一次”；接通后在上方点击“连接声音”。';
 $('submit').disabled=!token||!pending||pending.submit_started||pending.state!=='prepared';$('query').disabled=!token||!pending;$('clear').disabled=!pending||!['submitted','cancelled'].includes(pending.state);
 $('archive-ended-call').disabled=!token||pending?.type!=='call'||!['dial','hangup'].includes(pending?.action)||!pending?.submit_started||!['unknown','submitting'].includes(pending?.state);
 $('send-sms').disabled=!token||busy||!!pending||dailyPhone?.managing||dailyPhone?.working;
 $('sms-query').disabled=!token||busy||pending?.type!=='sms';
 $('sms-close').disabled=busy||pending?.type!=='sms'||!( ['submitted','cancelled'].includes(pending.state)||pending.state==='prepared'&&!pending.submit_started);
 $('sms-close').textContent=pending?.state==='prepared'?'放弃未发送操作':'完成并清空草稿';
 $('daily-login').disabled=busy||!!lease;
 const names={prepared:'尚未发送。可以放弃此操作；手动确认入口仍在调试区。',submitted:'已提交发送；对端是否收到请另行确认。',cancelled:'本次发送已取消。',submitting:'发送结果待确认，请查询，暂勿再次发送。',unknown:'发送结果未确认，请查询，暂勿再次发送。'};
 $('sms-send-state').textContent=!pending?'填写号码和正文，确认后发送。':pending.type!=='sms'?'有待处理的通话操作，请先在电话或调试区核对。':pending.submit_started&&!['submitted','cancelled'].includes(pending.state)?'发送结果待确认，请查询，暂勿再次发送。':names[pending.state]||'请查询本次发送结果。';
}
async function initialize(){
 db=await new Promise((resolve,reject)=>{const opening=indexedDB.open('rustsimhub-client-v1',1);opening.onupgradeneeded=()=>opening.result.createObjectStore('state');opening.onsuccess=()=>resolve(opening.result);opening.onerror=()=>reject(Error('浏览器存储不可用'));});
 key=await stored('key');if(!key){key=await crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify']);await save('key',key);}
 publicKey=new Uint8Array(await crypto.subtle.exportKey('raw',key.publicKey));
 $('client-key').textContent='浏览器公钥：'+hex(publicKey);$('pin').value=await stored('pin')||'';pending=await stored('pending')||null;showPending();
 $('test-restore-draft').hidden=!await stored('test-center-saved-draft');
 const identity=await api({op:'identity'});$('first-setup').hidden=identity.owner_setup_available!==true;
 try{const response=await fetch('/api/media/capabilities',{credentials:'omit',cache:'no-store',redirect:'error'});const capability=await response.json();audioReady=response.ok&&capability.protocol===1&&capability.enabled===true;}catch{audioReady=false;}
 $('audio-section').hidden=!audioReady;showAudio();
 showTestControls();$('test-state').textContent=audioReady?'选一个测试即可。来电测试开始后，再用另一部手机拨入。':'可以检查状态和测试短信；此设备暂未开启浏览器音频。';
}
async function proof(challenge,purpose,pairScopes,epoch=viewEpoch,sessionToken=null){
 let expectedKind=purpose==='bootstrap'?'owner':'client';
 if(['login','session_renew'].includes(purpose)){
  if(!Array.isArray(challenge.transcript)||challenge.transcript.length>4096)throw Error('挑战格式无效');
  // This selects the expected signed shape; prove still verifies device pin,
  // exact transcript, client key and role before producing any signature.
  const claimed=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array(challenge.transcript))).kind;
  if(!['client','owner'].includes(claimed))throw Error('浏览器登录身份不支持');expectedKind=claimed;
 }
 const pin=unhex($('pin').value.trim());const signature=await prove(challenge,pin,publicKey,key.privateKey,purpose,pairScopes,expectedKind,sessionToken);current(epoch);
 if(purpose!=='session_renew')await save('pin',hex(pin));current(epoch);return signature;
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
 clearWifiNetworks();
 sessionKeeper.reset();void audioLink.stop();const epoch=viewEpoch;token=null;lease=null;showAudio();const c=await api({op:'challenge',public_key:Array.from(publicKey)},epoch);const signature=await proof(c,'login',undefined,epoch);current(epoch);
 const r=await api({op:'authenticate',challenge_id:c.id,signature},epoch);current(epoch);if(!/^[a-f0-9]{64}$/.test(r.token))throw Error('登录响应无效');
 sessionKeeper.start(r.token,r.expires_in_seconds);token=r.token;configurationOpen=configurationVisible();void configurationIndicator.set(configurationOpen&&!document.hidden);networkPanel.start();dailyPhone.start();notice('已登录；会话15分钟，通话期间验证身份后续期');await refresh();
}
function row(text,parent,onClick){const item=document.createElement('div');item.className='row';const label=document.createElement('span');label.textContent=text;item.append(label);if(onClick){const button=document.createElement('button');button.textContent='查看';button.onclick=()=>act(onClick);item.append(button);}parent.append(item);}
async function inboxPage(reset=false){
 const load=pageLoad('sms',reset);
 if(reset){smsView++;smsPreview++;inboxCursor=null;smsManager.select(null);$('sms-detail').textContent='';$('inbox').replaceChildren();$('more-inbox').disabled=true;}
 const view=smsView;
 try{const page=await auth({op:'inbox',after:inboxCursor,limit:16});if(!load.current()||view!==smsView)throw Error('短信列表已变化，请重新刷新');
  for(const s of page.messages)row(smsListText(s),$('inbox'),async()=>{const preview=++smsPreview;smsManager.select(null);$('sms-detail').textContent='';const detail=await auth({op:'sms_detail',sms_id:s.id});if(view!==smsView||preview!==smsPreview)return;$('sms-detail').textContent=smsDetailText(detail);smsManager.select(s.id,detail);await auth({op:'notification_read',entity:'sms',record_id:s.id});});
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
function clearWifiNetworks(){wifiCatalogueGeneration++;$('wifi-networks').replaceChildren();$('wifi-networks-state').textContent='点击读取设备已发现的网络，也可手输网络名。';$('wifi-networks-refresh').disabled=false;}
function clearWifi(){clearWifiNetworks();wifiTrial=null;$('wifi-ssid').value='';$('wifi-passphrase').value='';$('wifi-state').textContent='没有试连接';for(const id of ['wifi-status','wifi-confirm','wifi-cancel'])$(id).disabled=true;}
async function loadWifiNetworks(){
 const generation=++wifiCatalogueGeneration;
 $('wifi-networks').replaceChildren();$('wifi-networks-refresh').disabled=true;$('wifi-networks-state').textContent='正在读取设备已发现的网络…';
 try{
  const result=await auth({op:'wifi_networks'});if(generation!==wifiCatalogueGeneration)return;
  if(result?.schema!==1||result.source!=='networkmanager_cache'||result.scan_requested!==false||result.hardware_mutations!==false||typeof result.wifi_enabled!=='boolean'||typeof result.scan_cache_known!=='boolean'||typeof result.truncated!=='boolean'||!Number.isInteger(result.omitted)||result.omitted<0||result.omitted>128||!Array.isArray(result.networks)||result.networks.length>32)throw Error('网络列表格式无效');
  // Validate the whole finite catalogue before creating any selection callbacks.
  for(const n of result.networks){
   if(typeof n.ssid!=='string'||!n.ssid||new TextEncoder().encode(n.ssid).length>32||/[\u0000-\u001f\u007f-\u009f]/u.test(n.ssid)||!Number.isInteger(n.strength)||n.strength<0||n.strength>100||!Number.isInteger(n.frequency_mhz)||n.frequency_mhz<2000||n.frequency_mhz>8000||!['wpa2_personal','open','unsupported'].includes(n.security)||typeof n.connected!=='boolean'||n.selectable!==(n.security==='wpa2_personal'))throw Error('网络条目格式无效');
  }
  for(const n of result.networks){
   const item=document.createElement('div'),label=document.createElement('span'),select=document.createElement('button');item.className='wifi-network';
   label.textContent=`${n.ssid}${n.connected?' · 当前连接':''}\n信号 ${n.strength}% · ${n.frequency_mhz} MHz · ${{wpa2_personal:'WPA2 个人网络',open:'开放网络，暂不支持',unsupported:'暂不支持此加密方式'}[n.security]}`;
   select.textContent='选择';select.disabled=!n.selectable;select.onclick=()=>act(()=>{
    if(generation!==wifiCatalogueGeneration||!token)throw Error('网络列表已变化，请重新读取');
    if(!n.selectable)throw Error('暂不支持此网络的加密方式，请选择 WPA2 个人网络');
    if(wifiTrial&&!['saved','rolled_back'].includes(wifiTrial.state))throw Error('请先查询或结束当前试连接');
    $('wifi-ssid').value=n.ssid;$('wifi-passphrase').value='';$('wifi-passphrase').focus();$('wifi-networks-state').textContent=`已选择 ${n.ssid}，填写密码后点“试连接”；尚未切换网络。`;
   });item.append(label,select);$('wifi-networks').append(item);
  }
  $('wifi-networks-state').textContent=!result.wifi_enabled?'设备 Wi-Fi 当前关闭；此操作不会开启无线。':result.networks.length?`已读取设备缓存，共 ${result.networks.length} 项${result.truncated?'（仅显示前32项）':''}。没有发起新扫描；选择后仍需试连接并确认。`:result.scan_cache_known?'缓存中没有可显示的网络，请手输网络名。':'设备暂无扫描缓存，请手输网络名。';
 }catch(error){if(generation===wifiCatalogueGeneration){$('wifi-networks').replaceChildren();$('wifi-networks-state').textContent='未能读取列表，请在本区重新登录后重试，或手输网络名。';}throw error;}
 finally{if(generation===wifiCatalogueGeneration)$('wifi-networks-refresh').disabled=false;}
}
function showWifi(result){wifiTrial=result;const terminal=['saved','rolled_back','unknown'].includes(result.state);const names={starting:'正在开始',connecting:'正在连接',ready:'已连接，等待确认',saving:'正在保存',saved:'已保存',rolled_back:'已恢复原连接',unknown:'结果不明，请检查设备；不会自动重试'};$('wifi-state').textContent=`${names[result.state]||'未知状态'} · 剩余 ${result.expires_in_seconds} 秒`;$('wifi-status').disabled=!token;$('wifi-confirm').disabled=!token||result.state!=='ready';$('wifi-cancel').disabled=!token||terminal;}
async function beginWifi(){if(wifiTrial&&!['saved','rolled_back','unknown'].includes(wifiTrial.state))throw Error('请先查询或结束当前试连接');const ssid=$('wifi-ssid').value,passphrase=$('wifi-passphrase').value;if(!ssid||new TextEncoder().encode(ssid).length>32)throw Error('网络名称须为1至32字节');$('wifi-passphrase').value='';showWifi(await auth({op:'wifi_begin',idempotency_key:crypto.randomUUID(),ssid,passphrase}));}
async function wifiAction(op){if(!wifiTrial)throw Error('没有当前试连接');try{showWifi(await auth({op,trial_id:wifiTrial.trial_id}));if(op==='wifi_confirm'&&wifiTrial.state==='saved'){configurationOpen=false;void configurationIndicator.set(false);}}catch(error){if(token){$('wifi-confirm').disabled=true;$('wifi-state').textContent='本次查询未确认；保留试连接编号，请重新查询，设备计时回退仍有效。';}throw error;}}
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
async function submit(check=()=>{}){
 check();
 if(!pending||pending.state!=='prepared'||pending.submit_started)throw Error('当前操作只能查询');
 const epoch=viewEpoch,old=pending,authToken=token;
 // Commit the client marker BEFORE the request. EOF, reload or timeout never auto-resubmits.
 pending.submit_started=true;await save('pending',pending);current(epoch);if(pending!==old||token!==authToken)throw Error('操作已变化，未继续提交');showPending();check();
 if(pending.type==='call'&&pending.action==='hangup')void audioLink.stop();
 let r;if(pending.type==='sms')r=await auth({op:'submit_sms',operation_id:pending.id});
 else r=await auth({op:'submit_call',operation_id:pending.id,lease,urgent:pending.action==='hangup'});
 current(epoch);if(pending!==old||token!==authToken)throw Error('操作已变化，请重新查询');if(r.lease)lease=r.lease;showAudio();await query();
}
async function query(){const epoch=viewEpoch,old=pending;if(!old)return;const r=await auth({op:old.type==='sms'?'sms_operation':'call_operation',operation_id:old.id});current(epoch);if(pending!==old)throw Error('当前操作已变化');const operation=old.type==='call'?r.operation:r;if(operation?.id!==old.id||!['prepared','submitting','submitted','cancelled','unknown'].includes(operation.state))throw Error('操作响应不匹配，请稍后查询');if(r.lease)lease=r.lease;old.state=operation.state;await save('pending',old);current(epoch);showPending();showAudio();notice('操作状态：'+operation.state+'；没有重复提交');}
async function dailySendSms(check=()=>{}){
 check();
 const epoch=viewEpoch;if(!token)throw Error('请先登录');if(pending)throw Error('请先查询并完成当前操作');
 const recipient=$('recipient').value.trim(),text=$('text').value;
 if(recipient.length<3||recipient.length>32||!/^\+?[0-9]+$/.test(recipient))throw Error('请输入有效的接收号码');
 if(!text||new TextEncoder().encode(text).length>4096||text.includes('\0'))throw Error('短信内容须为1至4096字节且不含空字符');
 if(!confirm(`确认发送给 ${recipient}？\n\n${text}`))return false;
 check();
 const line=await auth({op:'line_status'});current(epoch);check();if(line.status!=='ready')throw Error('SIM 尚未就绪');
 if(pending||$('recipient').value.trim()!==recipient||$('text').value!==text)throw Error('草稿或当前操作已变化，未发送');
 const operation=await auth({op:'prepare_sms',line_epoch:line.line,idempotency_key:crypto.randomUUID(),recipient,text});current(epoch);
 if(!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(operation?.id||'')||operation.state!=='prepared')throw Error('准备结果未确认，未提交发送');
 if(pending)throw Error('已有其他操作，本次未提交');
 const old={type:'sms',action:'send',id:operation.id,state:operation.state,submit_started:false};pending=old;await save('pending',old);current(epoch);check();
 if(pending!==old||$('recipient').value.trim()!==recipient||$('text').value!==text)throw Error('草稿已变化，尚未发送；可放弃未发送操作');
 await submit(check);return true;
}
async function closeSms(){
 const epoch=viewEpoch,old=pending;if(old?.type!=='sms'||!(['submitted','cancelled'].includes(old.state)||old.state==='prepared'&&!old.submit_started))throw Error('发送结果尚未确认，请保留并查询');
 if(old.state==='prepared'&&!confirm('放弃这笔尚未提交的短信操作？正文会保留，设备不会自动发送它。'))return;
 const receipts=await stored('sms-operation-receipts')||[];current(epoch);if(pending!==old||!Array.isArray(receipts))throw Error('操作记录已变化');
 await save('sms-operation-receipts',[...receipts.filter(x=>x.id!==old.id),{...old,local_discarded:old.state==='prepared'}].slice(-64));current(epoch);if(pending!==old)throw Error('当前操作已变化');
 await save('pending',null);current(epoch);if(pending!==old)throw Error('当前操作已变化');pending=null;
 if(old.state!=='prepared'){$('recipient').value='';$('text').value='';}smsLength();showPending();notice(old.state==='prepared'?'未发送操作已关闭，正文保留。':'本次操作已完成，记录已保存。');
}
// One visible task owns the same durable operation slot as daily/manual controls.
// Recovery only queries/archives exact receipts. It never retries an old action.
function showTestControls(){
 const running=testWorking||callTest.running,locked=busy||running;
 $('test-check').disabled=locked;$('test-sms').disabled=locked;
 $('test-start').disabled=locked||!audioReady;$('test-incoming-start').disabled=locked||!audioReady;
 $('test-stop').disabled=!running;$('test-recover').disabled=locked||!pending;
 $('test-recovery').hidden=!pending;
 $('test-pending-state').textContent=pending?'有一笔上次的操作需要核对。测试会先核对它；结果不明时会保留，不会重拨或重发。':'';
 $('test-copy').disabled=!testResult&&!pending;
}
function testMessage(text){testResult=text;$('test-state').textContent=text;showTestControls();}
async function testLogin(check){
 testMessage('正在登录此浏览器…');
 if(token)await sessionKeeper.ensure();else await login();check();
}
async function archiveTestCall(check=()=>{}){
 const epoch=viewEpoch,old=pending;
 await archiveEndedOperation({pending:old,request:command=>auth(command),archivePending,
  setPending:async value=>{current(epoch);check();if(pending!==old)throw Error('当前操作已变化');await save('pending',value);current(epoch);check();if(pending!==old)throw Error('当前操作已变化');pending=value;},
  current:()=>{current(epoch);check();if(pending!==old&&pending!==null)throw Error('当前操作已变化');}});
 showPending();
}
async function reconcileTestPending(check){
 const old=pending;if(!old)return;
 testMessage('正在核对上次操作，不会重新提交…');await query();check();
 if(pending!==old)throw Error('当前操作已变化');
 if(old.type==='call'&&['unknown','submitting'].includes(old.state)){
  await archiveTestCall(check);check();return;
 }
 const known=['submitted','cancelled'].includes(old.state);
 const unsent=old.state==='prepared'&&old.submit_started===false;
 if(!known&&!unsent)throw Error('上次操作结果尚未确认。请点击“复制测试结果”发给开发者；不会重拨或重发');
 if(unsent&&!confirm('上次有尚未提交的操作。保存回执并关闭它，再继续本次测试？旧操作不会被发送。'))throw Error('已保留上次尚未提交的操作');
 check();
 if(old.type==='call'){
  const status=await auth({op:'call_status'});check();
  if(status.available!==true||status.active_calls!==0)throw Error('仍有未结束或未确认的电话，请先结束通话');
  if(old.state==='submitted'){
   const response=await auth({op:'call_operation',operation_id:old.id});check();
   if(response.operation?.id!==old.id||response.operation.state!=='submitted'||!response.operation.call_id)throw Error('上次电话结果尚未确认');
   const call=await auth({op:'call_detail',call_id:response.operation.call_id});check();
   if(call.id!==response.operation.call_id||!(call.state==='ended'&&call.availability==='observed'||call.state==='reconciling'&&call.availability==='reconciling'&&call.modem_state===7))throw Error('尚未核实上次电话已结束');
  }
 }
 const receipt={...old,test_center_archive:true,local_discarded:unsent};
 await archivePending(receipt);check();if(pending!==old)throw Error('当前操作已变化');
 await save('pending',null);check();if(pending!==old)throw Error('当前操作已变化');pending=null;showPending();
}
async function runTestCenter(task){
 if(testWorking||callTest.running)return;
 const generation=++testGeneration,epoch=viewEpoch;testWorking=true;testTask=task;
 const check=()=>{current(epoch);if(generation!==testGeneration)throw Error('测试已停止；不会继续提交，已有操作保留核对');};
 showTestControls();
 try{
  await testLogin(check);
  if(task==='check'){
   testMessage('正在检查设备与电话状态…');
   const line=await auth({op:'line_status'});check();const call=await auth({op:'call_status'});check();
   let network=null;try{network=await auth({op:'network_status'});}catch{check();}check();
   const connected=kind=>network?.devices?.some(device=>device.kind===kind&&device.state===100);
   testMessage(`检查完成：SIM ${line.status==='ready'?'已就绪':'尚未就绪'}；电话${call.available===true?'服务可用':'服务暂不可用'}；${Number.isSafeInteger(call.active_calls)?call.active_calls:'未知'}个活动呼叫。`+(network?`\nWi-Fi ${connected('wifi')?'已连接':'未连接'}；USB ${connected('ethernet')?'已连接':'未连接'}；蜂窝数据${connected('cellular')?'已连接':'未连接'}。`:'\n网络状态暂不可读取。')+(pending?'\n有上次操作，请点击下方“核对上次操作”。':'\n没有待处理操作。'));return;
  }
  await reconcileTestPending(check);check();
  if(task==='recover'){testMessage('上次操作已核对，回执已保存。现在可以选择一项测试。');return;}
  if(task==='sms'){
   const draft=$('text').value,recipient=$('recipient').value;
   if((draft&&draft!=='CXYE')||(recipient&&recipient!=='10010')){
    if(!confirm('当前有其他短信草稿。将它保存在本机，再准备 10010 / CXYE 测试？')){testMessage('已保留草稿，未发送测试短信。');return;}
    const savedDraft=await stored('test-center-saved-draft');check();
    if(savedDraft&&(savedDraft.recipient!==recipient||savedDraft.text!==draft)&&!confirm('本机已有另一份测试前草稿备份。用当前草稿替换该备份？取消可保留两者并停止测试。'))throw Error('原备份与当前草稿均已保留，未发送');
    check();
    await save('test-center-saved-draft',{recipient,text:draft});check();$('test-restore-draft').hidden=false;
   }
   $('recipient').value='10010';$('text').value='CXYE';smsLength();
   testMessage('即将查询余额；请确认弹窗中的 10010 / CXYE。只发送一次。');
   const sent=await dailySendSms(check);check();
   testMessage(!sent?'已取消，未发送测试短信。':pending?.state==='submitted'?'测试短信已提交一次。正在读取收件箱；运营商回复可能稍后到达。':'发送结果未确认，记录已保留；不会再次发送。');
   if(sent&&pending?.state==='submitted'){await inboxPage(true);check();$('sms-panel').open=true;testMessage('测试短信已提交一次，收件箱已展开。请查看最新运营商回复；需要时点击“刷新收件箱”，不要重复发送。');}return;
  }
  if(!['outgoing','incoming'].includes(task))throw Error('测试类型无效');
  await callTest.start(task);check();
 }catch(error){if(generation===testGeneration&&epoch===viewEpoch)testMessage('测试停止：'+(error.message||'请稍后核对')+'；已有记录保留，不会自动重试');}
 finally{testWorking=false;showTestControls();}
}
async function copyTestResult(){
 const states=['prepared','submitting','submitted','cancelled','unknown'];
 const report={format:1,test:['check','recover','sms','outgoing','incoming'].includes(testTask)?testTask:null,result:testResult||'尚未测试',operation:pending?{type:['call','sms'].includes(pending.type)?pending.type:'other',action:['dial','accept','hangup','dtmf','send'].includes(pending.action)?pending.action:'other',operation_id:/^[0-9a-f-]{36}$/i.test(pending.id||'')?pending.id:null,state:states.includes(pending.state)?pending.state:'other',submit_started:pending.submit_started===true}:null};
 const text=JSON.stringify(report,null,2);
 try{await navigator.clipboard.writeText(text);notice('测试结果已复制，直接粘贴发来即可。');}
 catch{$('test-report').value=text;$('test-report').hidden=false;notice('请选取首页结果框中的文字，复制发来即可。');}
}
async function restoreTestDraft(){
 const epoch=viewEpoch;if(pending)throw Error('请先核对上次操作，再恢复草稿');
 const draft=await stored('test-center-saved-draft');current(epoch);
 if(!draft||typeof draft.recipient!=='string'||typeof draft.text!=='string'||draft.recipient.length>32||new TextEncoder().encode(draft.text).length>4096)throw Error('没有可恢复的草稿');
 if(!confirm('恢复测试前的草稿并替换当前输入？只恢复文字，不发送。'))return;
 if(pending)throw Error('当前操作已变化');
 // Retain the backup: logout can clear the visible fields at any time.
 $('recipient').value=draft.recipient;$('text').value=draft.text;smsLength();
 notice('已恢复原草稿，尚未发送；本机备份仍保留。');
}
function smsLength(){$('sms-length').textContent=`正文 ${new TextEncoder().encode($('text').value).length} / 4096 字节`;} $('text').oninput=smsLength;
async function act(fn){if(busy)return;if(dailyPhone?.working||dailyPhone?.managing){notice('请先在电话区结束当前操作或通话');return;}const generation=++actionGeneration,epoch=viewEpoch;busy=true;showPending();try{await fn();}catch(error){if(epoch===viewEpoch&&generation===actionGeneration)notice(error.message||'操作失败');}finally{if(generation===actionGeneration){busy=false;showPending();}}}
function bind(id,fn){$(id).onclick=()=>act(fn);}
bind('initialize-owner',initializeOwner);bind('pair',pair);bind('login',login);$('logout').onclick=()=>{viewEpoch++;actionGeneration++;busy=false;for(const controller of requests)controller.abort();requests.clear();token=null;lease=null;clearHistory();historySources=[];$('history-source').replaceChildren();$('history-entity').replaceChildren();$('history-section').hidden=true;for(const id of ['sms-detail','recipient','text','number','dtmf','events','setup-code']){const element=$(id);if('value' in element)element.value='';else element.textContent='';}$('inbox').replaceChildren();$('calls').replaceChildren();$('call-id').replaceChildren();$('status').textContent='未登录';$('lease').textContent='没有控制租约';notice('已退出；设备租约到期后清理呼叫');showPending();};bind('refresh',refresh);bind('sync',sync);
bind('sms-login',async()=>{await login();$('sms-management-state').textContent=smsManager.available?'已重新登录，请重新选择短信后管理。':'当前浏览器没有所有者管理权限，请使用已认领的所有者浏览器。';});
bind('daily-login',login);$('daily-logout').onclick=()=>$('logout').onclick();bind('sms-refresh',()=>inboxPage(true));bind('send-sms',dailySendSms);bind('sms-query',query);bind('sms-close',closeSms);
bind('test-restore-draft',restoreTestDraft);
bind('network-login',login);const networkPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{networkPanel.stop();networkPreviousLogout();};
const syncPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{eventSync.reset();smsView++;smsPreview++;callsView++;callsPreview++;historySourcesView++;invalidatePage('sms');invalidatePage('calls');inboxCursor=null;callsCursor=null;$('call-detail').textContent='';$('more-inbox').disabled=true;$('more-calls').disabled=true;syncPreviousLogout();};
const smsPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{smsManager.reset();smsPreviousLogout();};
const phonePreviousLogout=$('logout').onclick;$('logout').onclick=()=>{dailyPhone.reset();phonePreviousLogout();};
const sessionPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{sessionKeeper.reset();sessionPreviousLogout();};
bind('refresh-history',loadHistory);bind('more-history',()=>historyPage());$('history-source').onchange=()=>{historyTypes();return act(()=>historyPage(true));};$('history-entity').onchange=()=>{clearHistory();return act(()=>historyPage(true));};
bind('wifi-login',login);bind('wifi-networks-refresh',loadWifiNetworks);bind('wifi-begin',beginWifi);bind('wifi-status',()=>wifiAction('wifi_status'));bind('wifi-confirm',()=>wifiAction('wifi_confirm'));bind('wifi-cancel',()=>wifiAction('wifi_cancel'));const previousLogout=$('logout').onclick;$('logout').onclick=()=>{clearWifi();$('wifi-section').hidden=true;previousLogout();};
bind('more-inbox',()=>inboxPage());bind('more-calls',()=>callsPage());bind('prepare-sms',()=>prepare('sms','send'));for(const action of ['dial','accept','hangup','dtmf'])bind('prepare-'+action,()=>prepare('call',action));bind('submit',submit);bind('query',query);bind('clear',async()=>{if(!pending||!['submitted','cancelled'].includes(pending.state))throw Error('当前结果必须保留查询');pending=null;await save('pending',null);showPending();});
bind('archive-ended-call',async()=>{const epoch=viewEpoch,old=pending;await archiveEndedOperation({pending:old,request:command=>auth(command),archivePending,setPending:async value=>{current(epoch);await save('pending',value);current(epoch);pending=value;},current:()=>{current(epoch);if(pending?.id!==old?.id&&pending!==null)throw Error('当前操作已变化');}});showPending();notice('已核对呼叫结束并保存不确定操作记录；未重拨，设备原记录保持');});
setInterval(()=>{if(token&&lease&&!busy&&!callTest.running&&!dailyPhone.managing)act(async()=>{try{const r=await auth({op:'renew_call',lease});lease=r.lease;$('lease').textContent='控制租约已续期';showAudio();}catch(e){lease=null;void audioLink.stop();showAudio();$('lease').textContent='控制租约已失效；核心清理正在处理';throw e;}});},10000);
initialize().catch(error=>notice('初始化失败：'+error.message));

// Microphone permission may stay pending. It must not block hangup/logout.
$('audio-start').onclick=()=>{if(!token||!lease){notice('请先取得当前通话的控制租约');return;}const epoch=viewEpoch,authToken=token;void audioLink.start({token,lease}).catch(error=>{if(epoch===viewEpoch&&token===authToken)notice(error.message||'声音连接失败');}).finally(showAudio);};
$('audio-stop').onclick=()=>{void audioLink.stop();$('audio-state').textContent='声音已关闭；电话可在上方挂断';showAudio();};
$('audio-muted').onchange=()=>audioLink.mute($('audio-muted').checked);
bind('audio-play',()=>audioLink.play());
for(const [id,task] of [['test-check','check'],['test-start','outgoing'],['test-incoming-start','incoming'],['test-sms','sms'],['test-recover','recover']])bind(id,()=>runTestCenter(task));
$('test-copy').onclick=()=>{void copyTestResult();};
$('test-stop').onclick=()=>{testGeneration++;void callTest.stop();testMessage('已请求停止；尚未提交的动作不会继续。已提交的短信不能撤回，结果不明记录会保留。');};
const audioPreviousLogout=$('logout').onclick;$('logout').onclick=()=>{void callTest.stop(false);void audioLink.stop();$('audio-state').textContent='未连接声音';audioPreviousLogout();showAudio();};
addEventListener('pagehide',()=>{void callTest.stop(false);void audioLink.stop();});
const indicatorPreviousLogout=$('logout').onclick;
$('logout').onclick=()=>{configurationOpen=false;void configurationIndicator.set(false);indicatorPreviousLogout();};
bind('configuration-done',async()=>{configurationOpen=false;await configurationIndicator.set(false);notice('已结束本页配置');});
for(const panel of document.querySelectorAll('details[data-configuration]'))panel.addEventListener('toggle',()=>{if(token){configurationOpen=configurationVisible();void configurationIndicator.set(configurationOpen&&!document.hidden);}});
setInterval(()=>{if(token&&configurationOpen&&!document.hidden)void configurationIndicator.set(true);},10000);
document.addEventListener('visibilitychange',()=>{if(token)void configurationIndicator.set(configurationOpen&&!document.hidden);});
addEventListener('pagehide',()=>{configurationOpen=false;void configurationIndicator.set(false);});
addEventListener('pagehide',()=>networkPanel.stop());
addEventListener('pagehide',()=>eventSync.reset());
addEventListener('pagehide',()=>smsManager.reset());
addEventListener('pagehide',()=>dailyPhone.reset());
addEventListener('pagehide',()=>sessionKeeper.reset());
