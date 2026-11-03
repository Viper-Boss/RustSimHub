// SPDX-License-Identifier: AGPL-3.0-only
// One finite AP configuration. Only metadata reaches browser storage. Refresh
// and polling use read-only commands; an uncertain mutation is never replayed.
const uuid=s=>typeof s==='string'&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(s);
const digest=s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s);
const need=(ok,message='设备返回的配网状态无效，进度已保留，请查询')=>{if(!ok)throw Error(message);};
// These failures happen locally before any network submission. Keep an already
// proven trial usable instead of reporting its result as unknown.
class WifiInputError extends Error {}
const inputNeed=(ok,message)=>{if(!ok)throw new WifiInputError(message);};
const clone=v=>structuredClone(v);
export function pendingInput(value){
 if(value===null||value===undefined)return null;
 const keys=['format','id','revision','source','start_key','request_key_sha256','operation_id','trials','commits','cancel_sent'];
 need(value&&Object.keys(value).length===keys.length&&Object.keys(value).every(k=>keys.includes(k)),'配网记录无效，已停止自动操作');
 need(value.format===1&&uuid(value.id)&&Number.isSafeInteger(value.revision)&&value.revision>=0&&value.revision<=1000&&['created','existing'].includes(value.source));
 need((value.source==='created'?uuid(value.start_key):value.start_key===null)&&digest(value.request_key_sha256)&&(value.operation_id===null||uuid(value.operation_id))&&typeof value.cancel_sent==='boolean');
 need(Array.isArray(value.trials)&&value.trials.length<=16&&value.trials.every(uuid)&&new Set(value.trials).size===value.trials.length);
 need(Array.isArray(value.commits)&&value.commits.length<=16&&value.commits.every(c=>c&&Object.keys(c).length===2&&uuid(c.id)&&value.trials.includes(c.trial_id)));
 const ids=[value.id,...value.trials,...value.commits.map(c=>c.id)];need(new Set(ids).size===ids.length);
 need((value.operation_id!==null||value.source==='created'&&!value.trials.length&&!value.commits.length&&!value.cancel_sent));
 return clone(value);
}
export function credentialsInput(ssid,passphrase){
 inputNeed(typeof ssid==='string'&&ssid.length>0&&new TextEncoder().encode(ssid).length<=32&&!/[\u0000-\u001f\u007f-\u009f]/u.test(ssid),'请填写目标 Wi-Fi 名称（最多32字节）');
 inputNeed(typeof passphrase==='string'&&(/^[\x20-\x7e]{8,63}$/.test(passphrase)||/^[a-fA-F0-9]{64}$/.test(passphrase)),'请填写目标 Wi-Fi 的8–63位 WPA2 密码，或64位十六进制密钥');
 return {ssid,passphrase};
}
// Root's finite preflight refusals (before any network change) and what the owner can do.
export const refusals={wifi_disabled:'设备的 Wi-Fi 功能已关闭，未开启配置热点。',checkpoint_exists:'设备正在进行另一项网络操作，请稍后再试。',ap_unsupported:'设备的无线网卡当前不能开启热点。',subnet_conflict:'设备当前网络与配置热点的网段（192.168.100.x）冲突，未开启热点。',station_connected:'设备的 Wi-Fi 正被其他连接占用，未开启热点。',station_saved:'设备保存了多个 Wi-Fi，重新配网要求只保留一个，未开启热点。',gsm_autoconnect:'设备存在会自动连接的蜂窝数据配置，为避免产生流量，未开启热点。'};
function parentInput(v,record=null){
 need(v&&typeof v.state==='string');
 need(v.refusal===undefined||v.refusal===null||v.state==='closed'&&typeof v.refusal==='string'&&Object.hasOwn(refusals,v.refusal));
 if(['idle','disabled'].includes(v.state)){need(typeof v.available==='boolean');return v;}
 if(v.state==='not_found'){need(v.known===false&&v.not_found_is_not_proof===true);return v;}
 if(v.first_setup===true){need(!record&&['starting','ready','closing','unknown'].includes(v.state)&&uuid(v.operation_id)&&v.credentials_returned===false&&v.cellular_activation===false&&typeof v.owner_adoption_required==='boolean');return v;}
 need(['starting','ready','closing','closed','unknown'].includes(v.state)&&uuid(v.operation_id)&&digest(v.request_key_sha256));
 need(v.credentials_returned===false&&v.cellular_activation===false);
 if(record)need(v.request_key_sha256===record.request_key_sha256&&(!record.operation_id||v.operation_id===record.operation_id));
 return v;
}
function trialInput(v,r){
 need(v&&v.known===true&&v.operation_id===r.operation_id&&v.trial_id===r.trials.at(-1));
 need(['testing','ready','failed','cancelled','unknown'].includes(v.state)&&v.credentials_returned===false&&v.cellular_activation===false&&typeof v.authoritative_result==='boolean'&&typeof v.ap_return_verified==='boolean'&&typeof v.requires_confirmation==='boolean');
 if(['ready','failed','cancelled'].includes(v.state))need(v.authoritative_result&&v.ap_return_verified);
 if(v.state==='ready')need(v.settings_saved===false);
 return v;
}
function commitInput(v,r,c){
 need(v&&v.known===true&&v.operation_id===r.operation_id&&v.trial_id===c.trial_id&&v.commit_id===c.id);
 need(['submitted','saving','saved','restored_ap','unknown'].includes(v.state)&&v.credentials_returned===false&&v.cellular_activation===false&&v.not_resubmitted===true&&typeof v.authoritative_result==='boolean'&&typeof v.settings_saved==='boolean');
 need(v.settings_saved===(v.state==='saved'));if(v.state==='saved'||v.state==='restored_ap')need(v.authoritative_result);
 return v;
}
const messages={idle:'输入 Wi-Fi 名称和密码，一次点击即可开始。',starting:'正在准备配置。切网可能使页面短暂断开，请勿重复点击。',parent_ready:'设备已准备好，可以开始试连。',testing:'正在试连，设备会自动返回配置热点。页面断开时请重新连接原热点，再查询进度。',trial_ready:'试连成功，尚未保存。确认后设备才会保存 Wi-Fi。',failed:'未能连接，设备已返回热点。请检查密码后重新试连。',saving:'已提交保存。页面可能断开，进度会保留，请勿重复保存。',saved:'设备已确认保存 Wi-Fi，配置完成。',restored_ap:'保存未完成，设备已返回配置热点。可重新填写相同名称和密码，再明确确认。',closing:'已请求结束本次配置，正在核对；不会重新提交。',closed:'设备已确认本次配置会话结束。',unknown:'操作结果未确认。记录已保留，只会查询，不会重新试连或保存。',unavailable:'设备暂未开放热点配网，可使用网络配置区的其他入口。'};
export class ApConfigurator {
 constructor({document,env=globalThis,crypto=globalThis.crypto,request,login,getToken,loadPending,savePending,archivePending,blocked=()=>false,scan=null}){
  Object.assign(this,{doc:document,env,crypto,request,login,getToken,loadPending,savePending,archivePending,blocked,scan});
  this.scanRecord=null;this.scanDomain=null;this.scanFacts=null;this.scanTimer=null;
  this.$=id=>document.getElementById(id);this.epoch=0;this.record=null;this.working=false;this.opened=false;this.initialized=false;this.storageError=false;this.phase='idle';this.secret=null;this.trial=null;this.commit=null;this.parent=null;this.terminal=false;
  this.$('ap-wifi-begin').onclick=()=>this.action(()=>this.begin());
  this.$('ap-wifi-open').onclick=()=>this.action(()=>this.openAp());
  this.$('ap-wifi-confirm').onclick=()=>this.action(()=>this.confirm());
  this.$('ap-wifi-cancel').onclick=()=>this.action(()=>this.scanRecord?this.scanCancel():this.cancel());
  this.$('ap-wifi-query').onclick=()=>this.action(()=>this.refresh());
  this.$('ap-wifi-finish').onclick=()=>this.action(()=>this.finish());
  this.$('ap-wifi-show').onchange=()=>{this.$('ap-wifi-passphrase').type=this.$('ap-wifi-show').checked?'text':'password';};
  if(scan)scan.button.onclick=()=>this.action(()=>this.scanRecord?this.scanQuery():this.scanBegin());
  this.render();
 }
 get managing(){return !!this.record&&!this.terminal;}
 check(g){
  if(this.scanDomain!==null&&this.scan?.domain()!==this.scanDomain)this.suspend();
  need(g===this.epoch&&this.opened,'本页配置已关闭或设备已更换，进度已保留');
 }
 wipe(){this.secret=null;this.$('ap-wifi-ssid').value='';this.$('ap-wifi-passphrase').value='';this.$('ap-wifi-passphrase').type='password';this.$('ap-wifi-show').checked=false;}
 async initialize(){
  const g=this.epoch,domain=this.scan?this.scan.domain():null;
  try{
   const record=pendingInput(await this.loadPending()),scanRecord=this.scan?scanPendingInput(await this.scan.load(),domain):null;
   if(g!==this.epoch||this.scan&&this.scan.domain()!==domain)return;
   this.scanDomain=domain;this.record=record;this.scanRecord=scanRecord;this.phase=record||scanRecord?'unknown':'idle';
  }catch{if(g!==this.epoch)return;this.storageError=true;this.phase='unknown';}
  if(g!==this.epoch)return;this.initialized=true;this.render();
 }
 async open(){const g=this.epoch;this.opened=true;if(!this.initialized)await this.initialize();if(g!==this.epoch||!this.opened)return;this.render();if((this.record||this.scanRecord)&&this.getToken())await this.action(()=>this.refresh());}
 suspend(){this.epoch++;this.env.clearTimeout?.(this.scanTimer);this.scanTimer=null;this.scanFacts=null;this.opened=false;this.working=false;this.wipe();this.trial=null;this.commit=null;this.parent=null;this.terminal=false;this.phase=this.record?'unknown':'idle';this.initialized=false;this.scanRecord=null;this.record=null;this.render();}
 render(message=null){
  const state=this.phase,waiting=['starting','testing','saving','closing'].includes(state);
  const reenter=state==='trial_ready'&&!this.secret?'试连成功，尚未保存。请重新填写目标 Wi-Fi 名称和密码，再点击“确认保存并连接”。':null;
  this.$('ap-wifi-state').textContent=this.storageError?'本机配网记录无法核对，已停止提交。请保留记录并通过维护入口处理。':message||reenter||state==='closed'&&this.refusal&&refusals[this.refusal]||messages[state]||messages.unknown;
  this.$('ap-wifi-state').dataset.phase=state;this.$('ap-wifi-progress').setAttribute('aria-busy',String(this.working&&waiting));
  for(const [i,id] of ['ap-wifi-step-input','ap-wifi-step-test','ap-wifi-step-save'].entries()){
   const n=state==='saved'?3:['saving','trial_ready','restored_ap'].includes(state)?2:state==='testing'?1:0;
   this.$(id).dataset.complete=String(n>i);this.$(id).setAttribute('aria-current',n===i?'step':'false');
  }
  const begin=!this.record||(!this.record.commits.length&&!this.record.cancel_sent&&['parent_ready','failed'].includes(state));
  const confirm=this.record&&!this.record.cancel_sent&&['trial_ready','restored_ap'].includes(state);
  this.$('ap-wifi-begin').hidden=!begin;this.$('ap-wifi-begin').disabled=this.working||this.storageError||state==='unavailable';
  this.$('ap-wifi-begin').textContent=state==='failed'?'重新试连':'开始试连';
  this.$('ap-wifi-open').hidden=!!this.record;this.$('ap-wifi-open').disabled=this.working||this.storageError;
  this.$('ap-wifi-confirm').hidden=!confirm;this.$('ap-wifi-confirm').disabled=this.working;
  this.$('ap-wifi-query').hidden=!this.record||this.terminal;this.$('ap-wifi-query').disabled=this.working;
  this.$('ap-wifi-cancel').hidden=!this.record||this.terminal;this.$('ap-wifi-cancel').disabled=this.working||this.record?.cancel_sent===true;
  this.$('ap-wifi-finish').hidden=!this.terminal;this.$('ap-wifi-finish').disabled=this.working;
  for(const id of ['ap-wifi-ssid','ap-wifi-passphrase'])this.$(id).readOnly=!this.opened||this.working||state==='trial_ready'&&!!this.secret||!!this.record&&!['parent_ready','failed','trial_ready','restored_ap'].includes(state);
  this.$('ap-wifi-details').textContent=this.record?`配置 ${this.record.operation_id||'等待设备确认'}${this.record.trials.length?'\n试连 '+this.record.trials.at(-1):''}${this.record.commits.length?'\n保存 '+this.record.commits.at(-1).id:''}`:'';
  this.$('ap-wifi-reenter').hidden=!(confirm&&!this.secret);
  this.$('ap-wifi-fields').hidden=this.terminal;
  const blocked=!this.opened||this.opened&&this.blocked();
  for(const id of ['ap-wifi-begin','ap-wifi-open','ap-wifi-confirm','ap-wifi-query','ap-wifi-cancel','ap-wifi-finish'])if(blocked)this.$(id).disabled=true;
  if(this.scan){
   const cooling=this.scanFacts&&this.scanFacts.cooldownUntil>this.clock();
   this.scan.button.disabled=blocked||this.working||this.storageError||!this.scanRecord&&!!cooling;
   this.scan.button.textContent=this.scanRecord?'查询扫描结果':'刷新附近 Wi-Fi';
   this.scan.button.title=blocked?'页面未打开或其他操作正在进行':this.working?'正在核对本次操作，请勿重复提交':cooling?'扫描冷却中；结束后仍需明确点击刷新':'';
   if(this.scanRecord){
    for(const id of ['ap-wifi-begin','ap-wifi-open','ap-wifi-confirm','ap-wifi-finish'])this.$(id).disabled=true;
    this.$('ap-wifi-query').hidden=true;
    this.$('ap-wifi-cancel').hidden=false;
    this.$('ap-wifi-cancel').textContent=this.scanRecord.cancel_sent?'已请求取消扫描':'取消本次扫描';
    this.$('ap-wifi-cancel').disabled=blocked||this.working||this.storageError||this.scanRecord.cancel_sent;
    this.$('ap-wifi-details').textContent+=`\n扫描 ${this.scanRecord.scan_id}`;
   }else this.$('ap-wifi-cancel').textContent='结束本次配置';
   this.renderScanFacts();
  }
 }
 async action(work){
  if(this.working||!this.opened)return;if(this.blocked()){this.render('请先完成其他测试或通话，再配置 Wi-Fi。');return;}
  this.opened=true;const g=this.epoch;this.working=true;this.render();
  try{if(!this.initialized)await this.initialize();this.check(g);need(!this.storageError,'配网记录不可用');await work(g);}
  catch(error){if(g===this.epoch){if(error instanceof WifiInputError){this.render('尚未提交：'+error.message);}else{this.terminal=false;this.phase=this.record||this.scanRecord?'unknown':'idle';this.render(this.record||this.scanRecord?'进度已保留，请查询。'+(error.message||'操作未确认'):(error.message||'暂不能配置'));}}}
  finally{if(g===this.epoch){this.working=false;this.render(this.$('ap-wifi-state').textContent);}}
 }
 async currentRecord(g=this.epoch){
  const value=pendingInput(await this.loadPending());this.check(g);
  if((value?.id??null)!==(this.record?.id??null)||(value?.revision??null)!==(this.record?.revision??null)){
   this.record=value;this.wipe();this.terminal=false;this.trial=null;this.commit=null;this.parent=null;this.phase=value?'unknown':'idle';throw Error('进度已由另一页面更新，请查询后继续');
  }
 }
 async write(next,g=this.epoch){
  const expected=this.record?{id:this.record.id,revision:this.record.revision}:null;
  next=pendingInput(next);this.check(g);await this.savePending(next,expected);this.check(g);this.record=next;
 }
 async update(change,g=this.epoch){const next=clone(this.record);Object.assign(next,change);next.revision++;await this.write(next,g);}
 async signed(fresh,g=this.epoch){if(fresh||!this.getToken())await this.login();this.check(g);}
 async call(command,g=this.epoch){this.check(g);const v=await this.request(command);this.check(g);return v;}
 async parentStatus(g=this.epoch){
  const r=this.record;const v=parentInput(await this.call({op:'setup_ap_operation_status',request_key_sha256:r.request_key_sha256},g),r);
  if(v.known!==true){this.phase='unknown';this.render();return null;}
  if(!r.operation_id)await this.update({operation_id:v.operation_id},g);
  this.parent=v;this.setScanFacts(v.scan,v.expires_in_seconds);
  if(v.state==='closed'){this.terminal=true;this.phase='closed';this.refusal=v.refusal||null;this.wipe();this.render();}
  return v;
 }
 async pause(ms,g=this.epoch){await new Promise(resolve=>this.env.setTimeout(resolve,ms));this.check(g);}
 async readyParent(g=this.epoch){
  for(let i=0;i<30;i++){
   const p=await this.parentStatus(g);if(!p)return false;
   if(p.state==='ready'&&p.station_trial_api===true&&p.station_save_api===true)return true;
   if(p.state!=='starting'){this.phase=this.terminal?'closed':'unknown';this.render();return false;}
   this.phase='starting';this.render();await this.pause(1000,g);
  }
  this.phase='unknown';this.render();return false;
 }
 // Shared by "开启热点" and "开始试连": one durable record, at most one start, then wait for ready.
 async prepareParent(g){
  await this.signed(true,g);await this.currentRecord(g);
  if(!this.record){
   const p=parentInput(await this.call({op:'setup_ap_status'},g));
   if(p.state!=='ready'&&p.state!=='idle'||p.available!==true||p.station_trial_api!==true||p.station_save_api!==true||p.first_setup===true&&p.owner_adoption_required!==true){this.phase='unavailable';this.render();return;}
   const created=p.state==='idle'||p.first_setup===true&&p.owner_adoption_required===true,start=created?this.crypto.randomUUID():null;
   const key=created?Array.from(new Uint8Array(await this.crypto.subtle.digest('SHA-256',new TextEncoder().encode(start))),b=>b.toString(16).padStart(2,'0')).join(''):p.request_key_sha256;
   this.check(g);await this.write({format:1,id:this.crypto.randomUUID(),revision:0,source:created?'created':'existing',start_key:start,request_key_sha256:key,operation_id:created?null:p.operation_id,trials:[],commits:[],cancel_sent:false},g);
   if(created){this.phase='starting';this.render();const v=parentInput(await this.call({op:'setup_ap_start',idempotency_key:start},g),this.record);await this.update({operation_id:v.operation_id},g);}
  }
  need(!this.record.cancel_sent&&!this.record.commits.length,'已有保存或结束请求，只能先查询');
  return this.readyParent(g);
 }
 // Opening the AP moves Wi-Fi off the current network, so this page disconnects; the owner
 // continues from a device joined to the AP, at the same fixed address.
 async openAp(){
  const g=this.epoch,host=globalThis.location?.host;
  const where=host&&host.endsWith('.local:8443')?'https://'+host:'设备的固定网址（https://rustsimhub-设备号.local:8443）';
  this.phase='starting';this.render('正在开启配置热点。本页面会断开：请把手机或电脑连接到设备热点，再打开 '+where+' 继续。');
  if(!await this.prepareParent(g))return;
  this.phase='parent_ready';this.render('配置热点已开启。在已连接热点的设备上打开 '+where+'，填写要连接的 Wi-Fi 后开始试连。');
 }
 async begin(){
  need(!this.scanRecord,'请先查询原扫描结果，不会再次试连');
  const g=this.epoch,creds=credentialsInput(this.$('ap-wifi-ssid').value,this.$('ap-wifi-passphrase').value);
  if(!await this.prepareParent(g))return;
  if(this.record.trials.length){const v=trialInput(await this.call({op:'setup_ap_wifi_status',operation_id:this.record.operation_id,trial_id:this.record.trials.at(-1)},g),this.record);need(['failed','cancelled'].includes(v.state)&&v.ap_return_verified&&v.authoritative_result,'上次试连仍需核对，不会再次提交');}
  need(this.record.trials.length<16,'本次配置的试连次数已用完，请结束后再配置');
  this.secret=creds;const id=this.crypto.randomUUID();await this.update({trials:[...this.record.trials,id]},g);
  this.phase='testing';this.render();const value=await this.call({op:'setup_ap_wifi_begin',operation_id:this.record.operation_id,trial_id:id,...creds},g);this.acceptTrial(trialInput(value,this.record));
  await this.observe('trial',g);
 }
 acceptTrial(v){
  this.trial=v;this.phase=v.state==='ready'?(v.requires_confirmation?'trial_ready':'unknown'):v.state==='failed'||v.state==='cancelled'?'failed':v.state;
  if(this.phase==='failed')this.secret=null;this.render();
 }
 async confirm(){
  need(!this.scanRecord,'请先查询原扫描结果，不会提交保存');
  const g=this.epoch;await this.currentRecord(g);need(this.record&&!this.record.cancel_sent,'请先试连');
  let creds=this.secret||credentialsInput(this.$('ap-wifi-ssid').value,this.$('ap-wifi-passphrase').value);
  await this.signed(true,g);
  const previous=this.record.commits.at(-1);
  if(previous){const v=commitInput(await this.call({op:'setup_ap_wifi_commit_status',operation_id:this.record.operation_id,commit_id:previous.id},g),this.record,previous);need(v.state==='restored_ap'&&v.authoritative_result,'保存结果未确定，不会重新提交');}
  if(!await this.readyParent(g))return;
  const t=trialInput(await this.call({op:'setup_ap_wifi_status',operation_id:this.record.operation_id,trial_id:this.record.trials.at(-1)},g),this.record);need(t.state==='ready'&&t.requires_confirmation,'请先完成试连');
  need(this.record.commits.length<16,'本次确认次数已用完，请先结束配置');
  const id=this.crypto.randomUUID(),trial_id=this.record.trials.at(-1);await this.update({commits:[...this.record.commits,{id,trial_id}]},g);
  this.phase='saving';this.render();
  try{const v=await this.call({op:'setup_ap_wifi_confirm',operation_id:this.record.operation_id,trial_id,commit_id:id,...creds},g);this.acceptCommit(commitInput(v,this.record,this.record.commits.at(-1)));}
  finally{this.wipe();creds=null;}
  await this.observe('commit',g);
 }
 acceptCommit(v){this.commit=v;this.phase=['submitted','saving'].includes(v.state)?'saving':v.state;if(v.state==='saved'){this.terminal=true;this.wipe();}this.render();}
 async observe(kind,g=this.epoch){
  let errors=0;const clock=()=>this.env.performance?.now?.()??Date.now(),deadline=clock()+65000;
  for(let i=0;i<60&&clock()<deadline;i++){
   this.check(g);if(this.terminal||!['testing','saving'].includes(this.phase))return;
   await this.pause(1000,g);
   try{await this.readResult(g);errors=0;}catch(error){this.check(g);if(++errors>=3)throw error;this.render('连接暂时中断，正在只读查询原操作。请重新连接设备，勿重复提交。');}
  }
  if(!this.terminal&&['testing','saving'].includes(this.phase)){this.phase='unknown';this.render();}
 }
 async readResult(g=this.epoch){
  const r=this.record,c=r.commits.at(-1);
  if(c){const v=await this.call({op:'setup_ap_wifi_commit_status',operation_id:r.operation_id,commit_id:c.id},g);if(v?.known===false){need(v.not_found_is_not_proof===true);this.phase='unknown';this.render();return;}this.acceptCommit(commitInput(v,r,c));}
  else if(r.trials.length){const v=await this.call({op:'setup_ap_wifi_status',operation_id:r.operation_id,trial_id:r.trials.at(-1)},g);if(v?.known===false){need(v.not_found_is_not_proof===true);this.phase='unknown';this.render();return;}this.acceptTrial(trialInput(v,r));}
  else {const p=await this.parentStatus(g);if(p?.state==='ready'&&p.station_trial_api===true&&p.station_save_api===true){this.phase='parent_ready';this.render();}}
  if(!this.terminal&&(r.cancel_sent||this.phase==='unknown'))await this.parentStatus(g);
  if(r.cancel_sent&&!this.terminal){this.phase='closing';this.render();}
 }
 async refresh(){
  if(this.scanRecord)return this.scanQuery();
  const g=this.epoch;await this.currentRecord(g);need(this.record,'没有待查询的配置');await this.signed(false,g);await this.readResult(g);
 }
 async cancel(){
  need(!this.scanRecord,'请先查询原扫描结果，不会结束父配置');
  const g=this.epoch;await this.currentRecord(g);need(this.record&&!this.record.cancel_sent,'请查询原来的结束请求');await this.signed(true,g);
  if(!this.record.operation_id){await this.parentStatus(g);need(this.record.operation_id,'尚未确定配置操作，只能继续查询');}
  await this.update({cancel_sent:true},g);this.wipe();this.phase='closing';this.render();
  const v=await this.call({op:'setup_ap_cancel',operation_id:this.record.operation_id},g);
  if(v?.first_setup===true)need(v.operation_id===this.record.operation_id&&['closing','unknown'].includes(v.state)&&v.credentials_returned===false&&v.cellular_activation===false);
  else parentInput(v,this.record);
  await this.readResult(g);
 }
 async finish(){
  need(!this.scanRecord,'请先查询原扫描结果');
  const g=this.epoch;need(this.terminal&&this.record,'请先查询设备确认的结果');await this.currentRecord(g);
  const receipt={record:clone(this.record),state:this.phase};await this.archivePending(receipt);this.check(g);
  await this.savePending(null,{id:this.record.id,revision:this.record.revision});this.check(g);
  this.record=null;this.terminal=false;this.wipe();this.phase='idle';this.render('本次配置已完成，回执已保存在本浏览器。');
 }
 clock(){return this.env.performance?.now?.()??Date.now();}
 scanRefusal(reason){return {cooldown:'扫描仍在冷却中',limit:'本轮扫描次数已用完，请结束配置',conflict:'另一个网络操作正在进行',stale:'扫描能力已过期，请重新核对',window_closing:'父配置窗口即将结束',superseded:'已有更新的扫描',cancelled:'扫描已取消',capacity:'设备暂不能保留更多扫描记录',parent_closed:'父配置窗口已关闭'}[reason]||'扫描暂不可用，请明确点击刷新重新核对';}
 setScanFacts(cap,seconds){
  if(!this.scan)return;
  this.env.clearTimeout?.(this.scanTimer);this.scanTimer=null;
  if(!cap){this.scanFacts=null;this.renderScanFacts();return;}
  const ms=Number.isSafeInteger(cap.cooldown_remaining_ms)&&cap.cooldown_remaining_ms>0&&cap.cooldown_remaining_ms<=30000?cap.cooldown_remaining_ms:0;
  this.scanFacts={cap:clone(cap),seconds,observed:this.clock(),cooldownUntil:this.clock()+ms};
  if(ms){const g=this.epoch;this.scanTimer=this.env.setTimeout(()=>{if(g!==this.epoch||!this.opened||this.scan.domain()!==this.scanDomain)return;this.scanTimer=null;this.render();},ms);}
  this.renderScanFacts();
 }
 renderScanFacts(){
  if(!this.scan?.facts)return;
  if(!this.opened){this.scan.facts.textContent='本页已关闭；重新打开后查询原进度。';return;}
  if(this.working){this.scan.facts.textContent='正在处理本次配网操作，其他操作已暂停。';return;}
  if(this.blocked()){this.scan.facts.textContent='正在处理其他操作，请稍后明确点击刷新。';return;}
  const f=this.scanFacts;
  if(!f){this.scan.facts.textContent='刷新会短暂断开热点；只在你确认后扫描。也可手动填写名称。';return;}
  const c=f.cap,parts=[];
  if(c.supported!==true)parts.push('当前没有可核验的主动扫描能力，可手动填写名称；点击刷新只会先核对条件');
  else {
   if(Number.isInteger(c.remaining)&&Number.isInteger(c.max_per_parent))parts.push(`上次核验本轮剩余 ${c.remaining}/${c.max_per_parent} 次`);
   const remaining=Math.max(0,Math.ceil((f.cooldownUntil-this.clock())/1000));
   if(remaining)parts.push(`扫描冷却约 ${remaining} 秒，结束后请明确点击刷新`);
   else if(c.refusal==='cooldown')parts.push('本地冷却计时已结束，点击刷新重新核对设备条件');
   else if(c.available!==true)parts.push(this.scanRefusal(c.refusal));
  }
  if(Number.isSafeInteger(f.seconds)&&f.seconds>=0)parts.push(`父配置窗口上次核验剩余 ${f.seconds} 秒`);
  this.scan.facts.textContent=parts.join('；')+'。不会排队自动扫描。';
 }
 async scanCancel(){
  const g=this.epoch;await this.scanCurrent(g);need(this.scanRecord&&!this.scanRecord.cancel_sent,'请查询原来的取消请求');
  await this.signed(true,g);await this.scanCurrent(g);
  const old=this.scanRecord,next={...old,cancel_sent:true};
  await this.scan.save(next,old);this.check(g);this.scanRecord=next;this.wipe();this.render();
  this.scan.note.textContent='已记录取消请求，正在核对设备恢复；不会重复取消或扫描。';
  try{await this.scanAccept(await this.call({op:'setup_ap_scan_cancel',operation_id:next.operation_id,scan_id:next.scan_id},g),g);}
  catch{this.check(g);this.scan.note.textContent='取消结果或热点恢复未确认，记录已保留，请查询原扫描。';}
 }
 async scanCurrent(g=this.epoch){
  need(this.scan,'本页不支持重新扫描');
  const current=scanPendingInput(await this.scan.load(),this.scan.domain());this.check(g);
  need(JSON.stringify(current)===JSON.stringify(this.scanRecord),'扫描记录已在另一页面更新，请重新打开本页');
 }
 async scanBegin(){
  const g=this.epoch;await this.scanCurrent(g);need(!this.scanRecord,'只会查询已有扫描');
  if(!this.env.confirm('重新扫描会短暂断开配置热点，并结束尚未保存的试连。请稍后重新连接同一热点，页面会查询本次结果。继续吗？'))return;
  if(!await this.prepareParent(g))return;
  const cap=this.parent?.scan;
  if(!cap||cap.supported!==true){this.scan.note.textContent='当前没有可核验的主动扫描能力（未启用或能力已过期），可手动填写 Wi-Fi 名称；稍后明确点击刷新重新核对。';return;}
  if(cap.available!==true){
   this.scan.note.textContent=cap.refusal==='cooldown'?(Number.isSafeInteger(cap.cooldown_remaining_ms)&&cap.cooldown_remaining_ms>0?`扫描冷却中，约 ${Math.ceil(cap.cooldown_remaining_ms/1000)} 秒后可再试。`:'扫描冷却中，请稍后明确点击刷新重新核对。'):cap.refusal==='limit'?'本轮扫描次数已用完，请先结束配置。':'当前正在处理其他配网操作，暂不能扫描。';return;
  }
  const r=scanPendingInput({format:2,cancel_sent:false,device_key:this.scan.domain(),operation_id:this.record.operation_id,request_key_sha256:this.record.request_key_sha256,scan_id:this.crypto.randomUUID()},this.scan.domain());
  // Persist the device/parent/scan binding before the sole mutation. Loss of its
  // response can only lead to a query, including after reload or a new login.
  await this.scan.save(r,null);this.check(g);this.scanRecord=r;
  this.secret=null;this.trial=null;this.$('ap-wifi-passphrase').value='';
  this.phase='unknown';this.scan.note.textContent='正在重新扫描。热点可能短暂断开，请重新连接原热点；不会重复扫描。';this.render();
  try{const v=await this.call({op:'setup_ap_scan_begin',operation_id:r.operation_id,scan_id:r.scan_id},g);if(await this.scanAccept(v,g))return;}
  catch{this.check(g);this.scan.note.textContent='扫描请求结果未确认，请重新连接原热点后查询；不会再次提交。';return;}
  const clock=()=>this.env.performance?.now?.()??Date.now(),deadline=clock()+45000;
  for(let i=0;i<40&&clock()<deadline;i++){
   await this.pause(1000,g);
   try{if(await this.scanRead(g))return;}catch{this.check(g);this.scan.note.textContent='热点暂时断开，请重新连接后点击“查询扫描结果”。';return;}
  }
  this.scan.note.textContent='扫描结果及热点恢复尚未确认，记录已保留，请重新连接原热点后查询。';
 }
 async scanQuery(){
  const g=this.epoch;await this.scanCurrent(g);need(this.scanRecord,'没有待查询的扫描');await this.signed(false,g);await this.scanRead(g);
 }
 async scanRead(g){
  const r=this.scanRecord;return this.scanAccept(await this.call({op:'setup_ap_scan_status',operation_id:r.operation_id,scan_id:r.scan_id},g),g);
 }
 async scanAccept(v,g){
  await this.scanCurrent(g);const r=this.scanRecord;
  if(v?.known===false){need(v.not_found_is_not_proof===true);this.scan.note.textContent='尚未查到原扫描，不能证明未执行；记录已保留，只会查询。';return false;}
  need(v&&v.operation_id===r.operation_id&&v.scan_id===r.scan_id&&v.credentials_returned===false&&v.cellular_activation===false&&v.settings_saved===false);
  need(v.known===true&&['submitting','scanning','ready','failed','refused','cancelled','unknown'].includes(v.state)&&typeof v.authoritative_result==='boolean');
  this.setScanFacts(v.scan,v.parent_expires_in_seconds);
  if(!v.authoritative_result||!['ready','failed','refused','cancelled'].includes(v.state)){
   this.scan.note.textContent='扫描结果或热点恢复尚未确认，请重新连接原热点后查询。保留原记录，不会重复扫描。';return false;
  }
  let snapshot=null,message;
  if(v.state==='ready'){
   need(typeof v.historical==='boolean'&&(v.historical||v.ap_return_verified===true));
   const s=scanSnapshotInput(v.snapshot);
   if(!v.historical)snapshot=s;
   message=v.historical?'已核对历史扫描，未替换当前列表。':`扫描完成，找到 ${s.networks.filter(n=>n.security==='wpa2_personal').length} 个可选 WPA2 网络。`;
  }else {
   need(v.historical===true||v.ap_return_verified===true||v.state==='refused'&&v.execution_started===false,'热点恢复尚未核验，保留原扫描记录，请继续查询');
   message=v.state==='refused'?`设备未执行本次扫描：${this.scanRefusal(v.refusal)} 可手动输入 Wi-Fi。`:v.historical?'已核对历史扫描，未替换当前列表。':`本次扫描${v.state==='cancelled'?'已取消':'失败'}，设备已核验返回热点；保留原列表，可手动填写。`;
  }
  await this.scanCurrent(g);await this.scan.save(null,r);this.check(g);this.scanRecord=null;this.phase='unknown';
  if(snapshot)this.scan.snapshot(snapshot);this.scan.note.textContent=message;
  // The old trial may have been ended by scanning. Read its real result rather
  // than reviving a cached confirmation or ever persisting Wi-Fi credentials.
  if(this.record){
   try{await this.readResult(g);}
   catch{this.check(g);this.phase='unknown';this.render('扫描回执已核对，父配置进度暂不可读，请查询原配置；尚未保存 Wi-Fi。');}
  }else {this.phase='idle';this.render();}
  return true;
 }
}
export function scanPendingInput(v,domain){
 if(v===null||v===undefined)return null;
 const keys=['format','device_key','operation_id','request_key_sha256','scan_id',...(v?.format===2?['cancel_sent']:[])];
 need(v&&Object.keys(v).length===keys.length&&Object.keys(v).every(k=>keys.includes(k))&&(v.format===1||v.format===2&&typeof v.cancel_sent==='boolean')&&digest(domain)&&v.device_key===domain&&digest(v.request_key_sha256)&&uuid(v.operation_id)&&uuid(v.scan_id),'扫描记录不匹配，已停止提交');
 return {...clone(v),format:2,cancel_sent:v.cancel_sent===true};
}
export function scanSnapshotInput(s){
 need(s&&Array.isArray(s.networks)&&s.networks.length<=32&&Number.isInteger(s.omitted)&&s.omitted>=0&&s.omitted<=128&&typeof s.truncated==='boolean');
 for(const k of ['requested_uptime_ms','last_scan_uptime_ms','completed_uptime_ms'])need(Number.isSafeInteger(s[k])&&s[k]>=0);
 need(s.requested_uptime_ms<=s.last_scan_uptime_ms&&s.last_scan_uptime_ms<=s.completed_uptime_ms&&s.completed_uptime_ms-s.requested_uptime_ms<=8000&&(!s.truncated||s.networks.length===32));
 for(const n of s.networks)need(n&&typeof n.ssid==='string'&&n.ssid.length>0&&new TextEncoder().encode(n.ssid).length<=32&&!/[\u0000-\u001f\u007f-\u009f]/u.test(n.ssid)&&Number.isInteger(n.strength)&&n.strength>=0&&n.strength<=100&&Number.isInteger(n.frequency_mhz)&&n.frequency_mhz>=2000&&n.frequency_mhz<=8000&&['wpa2_personal','open','unsupported'].includes(n.security));
 return clone(s);
}
