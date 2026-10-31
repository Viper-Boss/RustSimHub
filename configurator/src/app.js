// SPDX-License-Identifier: AGPL-3.0-only
import {RecoveryController} from './recovery-controller.js';import {PendingStore} from './pending-store.js';import {BASE} from './static-policy.js';import {errorCode} from './contracts.js';
const messages={unavailable:'生产恢复后端尚未安装；没有提供蓝牙认证或设备访问',connecting:'正在选择设备并由主线端口核验预绑定身份和完整公钥',connected:'已连接；操作仍由设备重新授权',not_preapproved:'未提前登记恢复身份，请使用原可信管理渠道',pin_mismatch:'设备公钥不匹配，请停止并通过可信渠道核对',revoked:'恢复身份已撤销',permission_denied:'缺少有效授权',auth_failed:'认证未通过',unknown:'结果未确认；保留原请求，只查询',disconnected_not_rollback:'本地已断开；不能据此认定设备已回滚',query_original_request:'请核对原请求；没有自动重试',query_complete_no_resubmit:'只查询原请求，没有再次提交',archived_confirmed_terminal:'已明确归档有终态证明的记录'};
export function mount({client=null,store=null,document=globalThis.document}={}){
 const $=id=>document.getElementById(id);let controller,updating=false,updateCommitted=false;store=store||(client?new PendingStore(globalThis.indexedDB):null);
 const render=v=>{$('state').textContent=messages[v.message]||({ready:'试连就绪，尚未保存；需要明确确认',saved:'设备返回已保存证明',rolled_back:'设备返回已回滚证明'})[v.message]||'等待核对';$('connect').disabled=!v.available||v.busy||updating||updateCommitted;$('disconnect').disabled=!v.connected;$('begin').disabled=!v.connected||!v.recoveryAvailable||v.deviceBusy||v.busy||v.requests.some(r=>r.state==='unknown');const ready=v.requests.find(r=>r.state==='ready'&&r.trial_id&&controller?.ready(r.trial_id));$('confirm').disabled=!ready||!v.recoveryAvailable||v.busy||v.requests.some(r=>r.state==='unknown');const cancellable=v.requests.find(r=>r.trial_id&&controller?.cancellable(r.trial_id));$('cancel').disabled=!cancellable||!v.recoveryAvailable||v.busy||v.requests.some(r=>r.state==='unknown');$('requests').replaceChildren();for(const r of v.requests){const row=document.createElement('li'),text=document.createElement('p'),q=document.createElement('button'),a=document.createElement('button');text.textContent=`原请求 ${r.request_id} · ${r.state} · 提交开始标记 ${r.submit_started?'已保存':'未知'}`;q.textContent='只查询此原请求';q.type=a.type='button';q.disabled=!v.connected||v.busy;q.onclick=()=>perform(()=>controller.query(r.request_id));a.textContent='明确归档此已证实终态';a.disabled=!r.archive_allowed||v.busy||!v.connected;a.onclick=()=>perform(()=>controller.archive(r.request_id));row.append(text,q,a);$('requests').append(row);}};
 const perform=async fn=>{try{await fn();}catch(error){$('state').textContent=messages[errorCode(error)]||'操作未确认或被拒绝；原记录保留，没有自动重试';}};
 controller=new RecoveryController({client,store,notify:render,hidden:()=>document.hidden});
 $('connect').onclick=()=>{if(updating||updateCommitted)return;$('password').value='';return perform(()=>controller.connect($('pin').value));};
 $('begin').onclick=()=>{const ssid=$('ssid').value,passphrase=$('password').value;$('password').value='';return perform(()=>controller.write('begin_wifi',{ssid,passphrase}));};
 for(const [button,action] of [['confirm','confirm_wifi'],['cancel','cancel_wifi']])$(button).onclick=()=>{const r=controller.record?.requests.find(r=>action==='confirm_wifi'?controller.ready(r.trial_id):controller.cancellable(r.trial_id));return perform(()=>controller.write(action,{trialId:r?.trial_id}));};
 $('disconnect').onclick=()=>{$('password').value='';$('ssid').value='';return perform(()=>controller.disconnect());};
 document.addEventListener('visibilitychange',()=>{if(document.hidden){$('password').value='';$('ssid').value='';}});
 $('update').onclick=async()=>{
  if(updating||updateCommitted)return;const epoch=controller.epoch,started=performance.now();let timer;
  const safe=()=>{const elapsed=performance.now()-started;return !document.hidden&&!controller.connected&&!controller.busy&&controller.epoch===epoch&&Number.isFinite(elapsed)&&elapsed>=0&&elapsed<3000;};
  if(!safe()){$('offline').textContent='存在会话、原请求或未知存储状态，保留现有页面与查询记录';return;}
  updating=true;$('update').disabled=true;$('connect').disabled=true;
  try{
   const deadline=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('deadline')),3000);});
   const check=(async()=>{if(await new PendingStore(globalThis.indexedDB).anyPending()||!safe())throw Error();const registration=await navigator.serviceWorker.getRegistration(BASE);if(!safe())throw Error();return registration;})();
   const registration=await Promise.race([check,deadline]);if(!safe())throw Error();
   if(registration?.waiting){registration.waiting.postMessage({type:'activate_static_update'});updateCommitted=true;$('offline').textContent='已请求更新静态资源；本页暂停新连接，请结束其他窗口并手动重新打开页面';}
   else $('offline').textContent='没有等待中的静态更新；保留当前页面';
  }catch{$('offline').textContent='存在会话、原请求或未知存储状态，保留现有页面与查询记录';}
  finally{clearTimeout(timer);updating=false;$('update').disabled=updateCommitted;$('connect').disabled=updateCommitted||!client||!store||controller.busy;}
 };
 return controller;
}
if(typeof document!=='undefined'&&document.getElementById('state')){
 mount();
 if(location.pathname!==BASE&&location.pathname!==BASE+'index.html')document.getElementById('offline').textContent='当前路径与已冻结配置器路径不符，不注册离线缓存';
 else if('serviceWorker' in navigator)navigator.serviceWorker.register('./sw.js',{type:'module',scope:'./'}).then(()=>{document.getElementById('offline').textContent='离线资源安装由浏览器核验；首次离线不可用。更新需要明确操作且不能有未确认请求。';}).catch(()=>{document.getElementById('offline').textContent='离线资源未准备好；没有缓存过的首次离线不可用';});
}
