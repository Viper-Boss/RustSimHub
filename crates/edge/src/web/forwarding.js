// SPDX-License-Identifier: AGPL-3.0-only
// No passive polling or automatic mutation. Explicit synthetic tests have a
// bounded read-only receipt observation. Credentials stay in password inputs.
class FormError extends Error {}
const uuid=s=>typeof s==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s);
const generation=n=>Number.isSafeInteger(n)&&n>=0;
const testKind=kind=>kind==='test'||kind==='quick_test';
const retryableRead=error=>error?.coreCode==='busy'||[429,502,503,504].includes(error?.httpStatus);
async function readDuringPoll(read){
 try{return await read();}catch(error){if(!retryableRead(error))throw error;}
}
const kinds={webhook:'Webhook',bark:'Bark',telegram:'Telegram',dingtalk:'钉钉群机器人',wecom:'企业微信群机器人',feishu:'飞书群机器人',serverchan:'Server酱',pushplus:'PushPlus',gotify:'Gotify',email:'邮件（SMTP TLS）'};
// 聊天类通道：key 是主凭据，chat 复用为可选的加签密钥。可以直接粘贴完整的机器人地址，页面只取其中的凭据。
const chatKinds={dingtalk:{key:'access_token（或粘贴完整 Webhook 地址）',secret:'加签密钥 SEC…（可选）',from:/[?&]access_token=([A-Za-z0-9._-]+)/},wecom:{key:'key（或粘贴完整 Webhook 地址）',from:/[?&]key=([A-Za-z0-9._-]+)/},feishu:{key:'Webhook 末尾的 hook 标识（或粘贴完整地址）',secret:'签名校验密钥（可选）',from:/\/hook\/([A-Za-z0-9._-]+)/},serverchan:{key:'SendKey',from:/\/([A-Za-z0-9._-]+)\.send/},pushplus:{key:'token'},gotify:{key:'应用 token',endpoint:'Gotify 服务地址（HTTPS 根地址）'}};
const tokenText=s=>typeof s==='string'&&/^[A-Za-z0-9._-]{1,128}$/.test(s);
export function chatToken(kind,value){const s=String(value??'').trim(),m=/^https:\/\//.test(s)?chatKinds[kind]?.from?.exec(s):null;return m?m[1]:s;}
const states={queued:'排队中',in_flight:'递送中',confirmed:'已确认',failed:'失败',unknown:'结果不确定，不会自动重发',cancelled:'已取消'};
function requireValue(ok,message='响应无效，请重新读取状态'){if(!ok)throw new FormError(message);}
function text(s,max,empty=true){return typeof s==='string'&&(!empty?s.length>0:true)&&!s.includes('\0')&&new TextEncoder().encode(s).length<=max;}
function https(s,base=false){
 requireValue(text(s,2048,false)&&!/[\s\x00-\x1f\x7f]/.test(s),'请输入有效的 HTTPS 地址');
 let u;try{u=new URL(s);}catch{throw new FormError('请输入有效的 HTTPS 地址');}
 requireValue(u.protocol==='https:'&&!u.username&&!u.password&&!u.hash,'地址须为 HTTPS，且不含用户名、密码或片段');
 if(base)requireValue(u.pathname==='/'&&!u.search,'Bark 服务地址不能含路径或查询参数');
 return s;
}
function minute(value){
 requireValue(typeof value==='string'&&/^[0-9]{2}:[0-9]{2}$/.test(value),'时间须为 HH:MM');
 const [h,m]=value.split(':').map(Number);requireValue(h<24&&m<60,'时间超出范围');return h*60+m;
}
const clock=n=>String(Math.floor(n/60)).padStart(2,'0')+':'+String(n%60).padStart(2,'0');
function ruleRow(r){
 const w=r?.conditions?.time_window;
 return {id:r?.id,targets:r?.targets,sender:r?.sender_equals||'',contains:r?.body_contains||'',line:r?.line??'',full:r?.share==='full_text',masked:r?.share==='masked_text',regex:r?.conditions?.body_regex??'',window:!!w,start:w?clock(w.start_minute):'09:00',end:w?clock(w.end_minute):'18:00',offset:String(w?.utc_offset_minutes??480)};
}
const emailFields=['smtp_host','username','password','mail_from','rcpt_to'];
function dns(s){return text(s,253,false)&&s.split('.').every(p=>p.length<=63&&/^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/.test(p))&&!/^[0-9.]+$/.test(s);}
function mailbox(s){if(!text(s,254,false))return false;const parts=s.split('@');return parts.length===2&&parts[0].length<=64&&parts[0].split('.').every(p=>p.length>0&&/^[A-Za-z0-9!#$%&'*+\/=?^_`{|}~-]+$/.test(p))&&dns(parts[1]);}
export function policyInput(targets,rules,enabled,rulesRevision=null,emailRevision=null){
 requireValue(emailRevision===null||emailRevision===1,'邮件接口版本不支持，请更新页面');
 requireValue(rulesRevision===null||rulesRevision===2,'规则版本不支持，请更新页面');
 requireValue(typeof enabled==='boolean','开关状态无效');
 requireValue(Array.isArray(targets)&&targets.length<=8&&Array.isArray(rules)&&rules.length<=32,'最多8个目标、32条规则');
 const ids=new Set();
 const selected=targets.map(t=>{
  requireValue(uuid(t.id)&&!ids.has(t.id)&&Object.hasOwn(kinds,t.kind),'目标无效');ids.add(t.id);
  requireValue(t.kind!=='email'||emailRevision===1,'设备尚未启用邮件发送，请保留原配置');
  let destination;
  if(t.copyFrom){requireValue(!t.existing&&uuid(t.copyFrom)&&t.copyFrom!==t.id,'复制来源无效');destination={kind:'copy',source_id:t.copyFrom};}
  else if(t.keep){requireValue(t.existing===true,'新目标需要填写凭据');destination={kind:'keep'};}
  else if(t.kind==='webhook'){
   requireValue(!t.bearer||text(t.bearer,2048,false)&&/^[\x21-\x7e]+$/.test(t.bearer),'Webhook Bearer 格式无效');
   destination={kind:'webhook',url:https(t.endpoint),bearer:t.bearer||null};
  }else if(t.kind==='bark'){
   requireValue(text(t.key,256,false),'请输入 Bark key（最多256字节）');
   destination={kind:'bark',server:https(t.endpoint,true),key:t.key};
  }else if(t.kind==='email'){
   requireValue(dns(t.smtp_host),'请输入 SMTP DNS 主机名');
   requireValue(text(t.username,64,false)&&/^[\x21-\x7e]+$/.test(t.username),'邮件用户名须为1至64个可见 ASCII 字符');
   requireValue(text(t.password,256,false)&&/^[\x20-\x7e]+$/.test(t.password),'邮件密码须为1至256个可打印 ASCII 字符');
   requireValue(mailbox(t.mail_from)&&mailbox(t.rcpt_to),'发件人和唯一收件人须为单个 ASCII 邮箱地址');
   destination={kind:'email',smtp_host:t.smtp_host,port:465,tls:'implicit',username:t.username,password:t.password,mail_from:t.mail_from,rcpt_to:t.rcpt_to};
  }else if(Object.hasOwn(chatKinds,t.kind)){
   const key=chatToken(t.kind,t.key),secret=String(t.chat??'').trim();
   requireValue(tokenText(key),`请输入有效的${kinds[t.kind]}凭据（只含字母、数字、点、横线、下划线，最多128个字符）`);
   requireValue(!secret||chatKinds[t.kind].secret&&tokenText(secret),'加签密钥格式无效');
   destination=t.kind==='dingtalk'?{kind:'dingtalk',access_token:key,secret:secret||null}:t.kind==='wecom'?{kind:'wecom',key}:t.kind==='feishu'?{kind:'feishu',hook:key,secret:secret||null}:t.kind==='serverchan'?{kind:'serverchan',send_key:key}:t.kind==='pushplus'?{kind:'pushplus',token:key}:{kind:'gotify',server:https(t.endpoint,true),token:key};
  }else{
   requireValue(text(t.key,256,false)&&/^[0-9]+:[A-Za-z0-9_-]+$/.test(t.key),'请输入有效的 Telegram bot token');
   requireValue(text(t.chat,128,false),'请输入 Telegram chat ID（最多128字节）');
   destination={kind:'telegram',bot_token:t.key,chat_id:t.chat};
  }
  return {id:t.id,destination};
 });
 const ruleIds=new Set();
 const matched=rules.map(r=>{
  requireValue(uuid(r.id)&&!ruleIds.has(r.id),'规则无效');ruleIds.add(r.id);
  requireValue(Array.isArray(r.targets)&&r.targets.length>0&&r.targets.length<=8&&new Set(r.targets).size===r.targets.length&&r.targets.every(id=>ids.has(id)),'每条规则至少选择一个现有目标');
  requireValue(text(r.sender,256)&&text(r.contains,256),'发送方和正文条件各最多256字节');
  const line=r.line===''||r.line===null?null:Number(r.line);
  requireValue(line===null||generation(line)&&line>0,'SIM 代次须为正整数，留空表示不限');
  const regex=r.regex??'';requireValue(text(regex,256),'正则条件最多256个UTF-8字节');
  requireValue(!(r.full&&r.masked),'请只选择一种正文模式');
  let time_window=null;
  if(r.window){
   const start_minute=minute(r.start),end_minute=minute(r.end);
   requireValue(typeof r.offset==='string'&&/^-?[0-9]{1,3}$/.test(r.offset),'UTC偏移须为分钟整数');
   const utc_offset_minutes=Number(r.offset);requireValue(start_minute!==end_minute&&utc_offset_minutes>=-840&&utc_offset_minutes<=840,'起止不能相同，UTC偏移须在-840到840分钟内');
   time_window={start_minute,end_minute,utc_offset_minutes};
  }
  requireValue(!regex&&!time_window&&!r.masked||rulesRevision===2,'设备尚不支持新版规则，请保持原配置并升级');
  return {id:r.id,targets:r.targets.slice(),sender_equals:r.sender||null,body_contains:r.contains||null,line,share:r.full?'full_text':r.masked?'masked_text':'notice',...(regex||time_window?{conditions:{body_regex:regex||null,time_window}}:{})};
 });
 requireValue(!enabled||selected.length>0&&matched.length>0,'请先保存至少一个目标和一条规则');
 return {enabled:enabled===true,targets:selected,rules:matched,...(rulesRevision===2?{rules_revision:2}:{})};
}
export function pendingInput(value){
 if(value===null||value===undefined)return null;
 requireValue(value&&value.format===1&&uuid(value.id)&&['configure','enable','disable','test','quick_test'].includes(value.kind)&&generation(value.expected_generation)&&Object.keys(value).every(k=>['format','id','kind','expected_generation','target_id'].includes(k))&&(testKind(value.kind)?value.expected_generation>0&&uuid(value.target_id):!Object.hasOwn(value,'target_id')),'本机保存回执损坏，请保留记录并使用维护入口核对');
 return {format:1,id:value.id,kind:value.kind,expected_generation:value.expected_generation,...(testKind(value.kind)?{target_id:value.target_id}:{})};
}
export function processText(s){
 if(s?.quick_test_active===true)return '合成测试进行中；真实短信转发保持关闭';
 if(s?.process_state_verified===true&&s.enabled===false&&s.state==='disabled')return '转发进程已关闭';
 if(s?.process_state_verified===true&&s.enabled===true&&s.state==='running')return '转发服务运行中';
 if(s?.enabled===false)return '开关意图为关闭，进程尚未确认';
 if(s?.enabled===true)return '开关意图为开启，服务尚未确认';
 return '尚未读取转发状态';
}
function viewInput(v,s){
 requireValue(v&&v.generation===s.generation&&v.enabled===s.enabled&&Array.isArray(v.targets)&&v.targets.length<=8&&Array.isArray(v.rules)&&v.rules.length<=32,'配置已变化或响应无效，请重新读取');
 const ids=new Set();
 for(const t of v.targets){requireValue(uuid(t.id)&&!ids.has(t.id)&&Object.hasOwn(kinds,t.kind)&&t.credentials_redacted===true&&Object.keys(t).every(k=>['id','kind','credentials_redacted'].includes(k)));ids.add(t.id);}
 const ruleIds=new Set();
 for(const r of v.rules){requireValue(uuid(r.id)&&!ruleIds.has(r.id)&&Array.isArray(r.targets)&&r.targets.length>0&&r.targets.length<=8&&r.targets.every(id=>ids.has(id))&&new Set(r.targets).size===r.targets.length&&text(r.sender_equals??'',256)&&text(r.body_contains??'',256)&&Object.keys(r).every(k=>['id','targets','sender_equals','body_contains','line','share','conditions'].includes(k)));ruleIds.add(r.id);
  if(r.conditions!==null&&r.conditions!==undefined){
   const c=r.conditions;requireValue(c&&typeof c==='object'&&!Array.isArray(c)&&Object.keys(c).every(k=>['body_regex','time_window'].includes(k))&&(c.body_regex!==null&&c.body_regex!==undefined||c.time_window!==null&&c.time_window!==undefined));
   if(c.body_regex!==null&&c.body_regex!==undefined)requireValue(text(c.body_regex,256,false));
   if(c.time_window!==null&&c.time_window!==undefined){const w=c.time_window;requireValue(w&&typeof w==='object'&&!Array.isArray(w)&&Object.keys(w).every(k=>['start_minute','end_minute','utc_offset_minutes'].includes(k))&&generation(w.start_minute)&&w.start_minute<1440&&generation(w.end_minute)&&w.end_minute<1440&&w.start_minute!==w.end_minute&&Number.isSafeInteger(w.utc_offset_minutes)&&Math.abs(w.utc_offset_minutes)<=840);}
  }
 }
 // Reuse production input limits without requesting any retained secret.
 policyInput(v.targets.map(t=>({id:t.id,kind:t.kind,existing:true,keep:true})),v.rules.map(ruleRow),v.enabled,s.rules_revision??null,s.email_revision??null);
 for(const r of v.rules)requireValue(['notice','masked_text','full_text'].includes(r.share));
 return v;
}
export class ForwardingPanel {
 constructor({document,request,getToken,confirm,loadPending,savePending,archivePending,crypto=globalThis.crypto,delay=()=>new Promise(resolve=>setTimeout(resolve,1000))}){
  Object.assign(this,{doc:document,request,getToken,confirm,loadPending,savePending,archivePending,crypto,delay});
  this.epoch=0;
  this.secretInputs=new Set();
  for(const [id,fn] of [
   ['forwarding-refresh',g=>this.read(g)],['forwarding-toggle',g=>this.toggle(g)],
   ['forwarding-query',g=>this.query(g)],['forwarding-archive',g=>this.archiveUnknown(g)],
   ['forwarding-receipts-refresh',g=>this.refreshRecords(g)],['forwarding-receipts-more',g=>this.receipts(g,false)],
  ])this.$(id).onclick=()=>this.action(fn);
  this.pages=['records','channels','rules','settings'];
  for(const page of this.pages)this.$('fw-nav-'+page).onclick=()=>this.select(page);
  for(const id of ['fw-record-filter','fw-log-kind','fw-log-target'])this.$(id).onchange=()=>this.action(g=>this.receipts(g,true));
  this.$('fw-log-reset').onclick=()=>{this.resetLogFilter();return this.action(g=>this.receipts(g,true));};
  this.$('fw-log-clear').onclick=()=>this.action(g=>this.clearLog(g));

  this.$('forwarding-add-target').onclick=()=>this.edit('channel');
  this.$('forwarding-add-rule').onclick=()=>this.edit('rule');
  this.$('fw-rule-channel').onclick=()=>this.select('channels');
  this.$('fw-next').onclick=()=>this.select(this.nextPage);
  this.$('fw-dialog-form').onsubmit=event=>{event.preventDefault();return this.action(g=>this.save(g));};
  this.$('fw-dialog-remove').onclick=()=>this.action(g=>this.openReceipt?this.clearLog(g,this.openReceipt):this.remove(g));
  this.$('fw-dialog-close').onclick=this.$('fw-dialog-cancel').onclick=()=>this.closeEditor();
  this.$('fw-dialog').oncancel=event=>{event.preventDefault();if(!this.busy)this.closeEditor();};
  this.reset();
 }
 $(id){return this.doc.getElementById(id);}
 current(g){if(g!==this.epoch||!this.getToken())throw new FormError('会话已变化，请重新读取');}
 clearSecrets(){for(const input of this.secretInputs)input.value='';}
 suspend(){this.reset();}
 reset(){
  this.epoch++;this.busy=false;this.ready=false;this.sample=null;this.view=null;this.pending=null;
  this.cursor=null;this.more=false;this.receiptsLoaded=false;this.receiptRows=[];
  this.resetLogFilter();this.logSnapshot=null;
  this.closeEditor(false);this.message('');this.select('records',false);
  this.render();
 }
 message(text){this.$('forwarding-state').textContent=text;}
 get blocked(){return this.busy||!this.ready||!!this.pending||this.sample?.quick_test_active===true;}
 get quickTestReady(){
  return this.sample?.quick_test_api===true&&this.sample.enabled===false&&this.sample.process_state_verified===true&&this.sample.state==='disabled';
 }
 get testReady(){
  return this.quickTestReady||this.sample?.synthetic_test_api===true&&this.sample.enabled===true&&
   this.sample.process_state_verified===true&&this.sample.state==='running'&&this.sample.delivery_api===true;
 }
 select(page,load=true){
  this.page=page;
  for(const key of this.pages){
   this.$('fw-page-'+key).hidden=page!==key;
   const nav=this.$('fw-nav-'+key);
   if(page===key)nav.setAttribute('aria-current','page');
   else nav.removeAttribute('aria-current');
  }
  if(load&&page==='records'&&this.ready&&!this.receiptsLoaded)void this.action(g=>this.receipts(g,true));
 }
 enter(){
  if(this.getToken()&&!this.view)return this.action(g=>this.read(g));
 }
 update(){
  const blocked=this.blocked,targets=this.view?.targets?.length??0,rules=this.view?.rules?.length??0;
  this.$('forwarding-login').hidden=this.ready&&!this.pending;
  this.$('forwarding-toggle').disabled=blocked||(!this.sample?.enabled&&(!targets||!rules));
  this.$('forwarding-toggle').textContent=this.sample?.enabled?'关闭转发':'开启转发';
  this.$('fw-toggle-hint').textContent=!this.view?'登录后可管理总开关。':!targets||!rules?'添加发送通道和转发规则后可开启。':'开启后按规则处理新收到的短信。';
  const badge=this.$('forwarding-status-badge');
  const running=this.sample?.state==='running'&&this.sample.enabled===true&&this.sample.process_state_verified===true;
  badge.textContent=!this.sample?'未读取':this.sample.quick_test_active?'正在测试':running?'转发已开启':this.sample.process_state_verified&&this.sample.state==='disabled'?'转发已关闭':'状态待核对';
  badge.className='fw-status'+(running?' is-on':'');
  this.$('forwarding-add-target').disabled=blocked||targets>=8;
  this.$('forwarding-add-rule').disabled=blocked||!targets||rules>=32;
  this.$('fw-rule-channel').hidden=targets>0||!this.ready;
  this.$('fw-rule-hint').textContent=!targets?'先添加发送通道，再为它设置转发规则。':'选择哪些短信需要转发，以及发送到哪些通道。';
  this.$('fw-recovery').hidden=!this.pending;
  this.$('forwarding-query').disabled=this.busy||!this.pending;
  this.$('forwarding-archive').disabled=this.busy||!this.pending;
  this.$('forwarding-pending').textContent=this.pending?'上次请求结果待核对。查询回执不会重复提交。':'';
  for(const id of ['forwarding-refresh','forwarding-receipts-refresh'])this.$(id).disabled=this.busy||!this.getToken();
  this.$('forwarding-receipts-more').disabled=this.busy||!this.more;
  this.$('forwarding-receipts-more').hidden=!this.more;
  this.$('forwarding-save').disabled=blocked||!this.draft;
  this.$('fw-dialog-remove').disabled=blocked;
  const filtered=Object.keys(this.logFilter()).length>0;
  this.$('fw-log-clear').textContent=filtered?'清空筛选结果':'清空记录';
  this.$('fw-log-clear').disabled=blocked||!this.logSnapshot?.removable;
  this.$('fw-log-reset').hidden=!filtered;
  for(const id of ['fw-record-filter','fw-log-kind','fw-log-target','fw-log-reset'])this.$(id).disabled=this.busy||!this.getToken();
  for(const id of ['fw-dialog-close','fw-dialog-cancel'])this.$(id).disabled=this.busy;
  for(const {button,test} of this.rowActions??[])button.disabled=blocked||(test&&!this.testReady);
  this.renderReceiptEmpty();
 }
 async action(fn){
  if(this.busy)return;
  const g=this.epoch;this.busy=true;this.update();
  try{await fn(g);}
  catch(error){
   if(g===this.epoch){
    if(this.pending)this.closeEditor();
    const message=error instanceof FormError?error.message:
     this.pending?'上次请求还需核对，记录已保留。请重新登录或点击「核对上次请求」。':
     error?.coreCode==='denied'?'转发请求被拒绝，请使用所有者账号重新登录。':'转发操作暂未确认，请先刷新状态。';
    if(this.draft){const issue=this.$('fw-dialog-error');issue.textContent=message;issue.hidden=false;}
    else this.message(message);
   }
  }finally{if(g===this.epoch){this.busy=false;this.update();}}
 }
 status(s){requireValue(s&&s.available===true&&s.configuration_api===true&&generation(s.generation)&&typeof s.enabled==='boolean'&&typeof s.process_state_verified==='boolean');return s;}
 manageable(s){return s.service_management_api===true&&s.retained_credentials_api===true&&s.request_receipt_api===true;}
 async observe(g,command){
  this.current(g);
  try{return await this.request(command);}catch(error){
   if(!retryableRead(error))throw error;
   this.current(g);await this.delay();this.current(g);
   return this.request(command);
  }
 }
 async load(g){
  requireValue(!!this.getToken(),'请先使用所有者浏览器登录');
  this.ready=false;this.sample=null;this.view=null;this.render();
  const s=this.status(await this.observe(g,{op:'forwarding_status'}));this.current(g);
  const v=viewInput(await this.observe(g,{op:'forwarding_rules'}),s);this.current(g);
  this.sample=s;this.view=v;this.ready=this.manageable(s);this.render();
  return s;
 }
 async read(g){
  this.current(g);
  const pending=pendingInput(await this.loadPending());this.current(g);this.pending=pending;
  await this.load(g);
  this.message(this.ready?'':'当前设备尚未启用完整的转发管理。');
  if(this.pending&&this.ready)await this.query(g);
  if(this.page==='records')await this.receipts(g,true);
 }
 async refreshRecords(g){
  if(!this.view){await this.read(g);return;}
  await this.receipts(g,true);
 }
 element(tag,text){const e=this.doc.createElement(tag);if(text!==undefined)e.textContent=text;return e;}
 label(text,input,cls=''){const label=this.element('label',text);if(cls)label.className=cls;label.append(input);return label;}
 button(text,fn,cls=''){
  const button=this.element('button',text);button.type='button';button.className=cls;button.onclick=fn;return button;
 }
 options(entries,value){
  const select=this.element('select');
  for(const [key,label] of entries){const option=this.element('option',label);option.value=key;select.append(option);}
  select.value=value;
  return select;
 }
 channelName(id){
  const channel=this.view?.targets.find(t=>t.id===id);
  if(!channel)return '通道 '+id.slice(-8);
  const count=this.view.targets.filter(t=>t.kind===channel.kind).length;
  return kinds[channel.kind]+(count>1?' · '+id.slice(-8):'');
 }
 target(t=null){
  return {id:t?.id||this.crypto.randomUUID(),existing:!!t,kind:t?.kind||'webhook',keep:!!t};
 }
 rule(r=null){return {...ruleRow(r),id:r?.id||this.crypto.randomUUID(),targets:r?.targets.slice()||(this.view.targets.length===1?[this.view.targets[0].id]:[])};}
 shareLabel(r){return {notice:'仅发送通知',masked_text:'发送脱敏正文',full_text:'发送完整正文'}[r.share];}
 ruleTitle(r){
  const conditions=[];
  if(r.sender_equals)conditions.push('发送方等于 '+r.sender_equals);
  if(r.body_contains)conditions.push('正文包含 '+r.body_contains);
  if(r.conditions?.body_regex)conditions.push('正文匹配正则');
  if(r.line)conditions.push('SIM 代次 '+r.line);
  if(r.conditions?.time_window)conditions.push('指定时间段');
  return conditions.join(' · ')||'所有新短信';
 }
 listRow(mark,title,subtitle){
  const row=this.element('li');row.className='fw-row';
  const icon=this.element('span',mark);icon.className='fw-row-icon';icon.setAttribute('aria-hidden','true');
  const body=this.element('div');body.className='fw-row-body';body.append(this.element('strong',title),this.element('small',subtitle));
  const actions=this.element('div');actions.className='fw-actions';
  row.append(icon,body,actions);return {row,actions};
 }
 render(){
  const channels=this.view?.targets??[],rules=this.view?.rules??[];
  if(this.view){
   const select=this.$('fw-log-target'),previous=select.value;
   const options=[['all','全部通道'],...channels.map(target=>[target.id,this.channelName(target.id)])];
   select.replaceChildren(...options.map(([value,label])=>{const option=this.element('option',label);option.value=value;return option;}));
   select.value=options.some(([value])=>value===previous)?previous:'all';
  }
  this.rowActions=[];
  const pressed=(text,fn,test=false)=>{
   const button=this.button(text,fn);this.rowActions.push({button,test});return button;
  };
  this.$('forwarding-targets').replaceChildren(...channels.map(t=>{
   const count=rules.filter(r=>r.targets.includes(t.id)).length;
   const {row,actions}=this.listRow(kinds[t.kind].slice(0,1),this.channelName(t.id),count?`关联 ${count} 条规则`:'尚未关联规则');
   const test=pressed('测试',()=>this.action(g=>this.test(g,t.id)),true);
   test.title=this.testReady?'发送一条合成测试通知':'请先刷新服务状态。当前转发服务无法进行测试。';
   actions.append(test,pressed('编辑',event=>this.edit('channel',t,event.currentTarget)),
    pressed('复制',event=>this.edit('channel',t,event.currentTarget,true)));return row;
  }));
  this.$('forwarding-rules').replaceChildren(...rules.map(r=>{
   const {row,actions}=this.listRow('规',this.ruleTitle(r),r.targets.map(id=>this.channelName(id)).join('、')+' · '+this.shareLabel(r));
   actions.append(pressed('编辑',event=>this.edit('rule',r,event.currentTarget)),
    pressed('复制',event=>this.edit('rule',r,event.currentTarget,true)));return row;
  }));
  for(const [key,rows] of [['target',channels],['rule',rules]]){
   this.$('forwarding-'+key+'-count').textContent=this.view?String(rows.length):'';
   this.$('forwarding-'+key+'-empty').hidden=rows.length>0;
  }
  this.renderReceipts();this.update();
 }
 edit(kind,item=null,trigger=null,copy=false){
  if(this.blocked||((!item||copy)&&this.$(kind==='channel'?'forwarding-add-target':'forwarding-add-rule').disabled))return;
  const row=kind==='channel'?this.target(copy?null:item):this.rule(item);
  if(copy){
   row.id=this.crypto.randomUUID();
   if(kind==='channel'){row.kind=item.kind;row.copyFrom=item.id;}
  }
  this.draft={kind,row,existing:!!item&&!copy};
  this.lastTrigger=trigger??this.$(kind==='channel'?'forwarding-add-target':'forwarding-add-rule');
  this.$('fw-dialog-title').textContent=(copy?'复制':item?'编辑':'添加')+(kind==='channel'?'发送通道':'转发规则');
  this.$('fw-dialog-error').hidden=true;
  this.$('fw-dialog-remove').hidden=!item||copy;
  this.$('fw-dialog-remove').textContent='删除';
  this.$('forwarding-save').hidden=false;this.$('fw-dialog-cancel').textContent='取消';
  if(kind==='channel')this.channelFields();else this.ruleFields();
  this.update();this.$('fw-dialog').showModal();
 }
 closeEditor(restore=true){
  const trigger=this.lastTrigger;this.lastTrigger=null;
  this.clearSecrets();this.secretInputs.clear();this.draft=null;this.openReceipt=null;
  this.$('fw-dialog-body').replaceChildren();
  const issue=this.$('fw-dialog-error');issue.textContent='';issue.hidden=true;
  if(this.$('fw-dialog').open)this.$('fw-dialog').close();
  if(restore)trigger?.focus?.();
 }
 field(row,name,label,{secret=false,max=256,placeholder='',mode=''}={}){
  const input=this.element('input');input.type=secret?'password':'text';input.autocomplete=secret?'new-password':'off';
  input.maxLength=max;input.placeholder=placeholder;if(mode)input.inputMode=mode;
  if(secret){this.secretInputs.add(input);row[name+'Input']=input;}
  else{input.value=row[name]??'';input.oninput=()=>{row[name]=input.value;};}
  return this.label(label,input);
 }
 channelFields(){
  const t=this.draft.row;
  this.clearSecrets();this.secretInputs.clear();
  for(const name of ['endpoint','bearer','key','chat',...emailFields])delete t[name+'Input'];
  const body=this.$('fw-dialog-body');body.replaceChildren();
  const kind=this.options(Object.entries(kinds).filter(([key])=>key!=='email'||this.sample.email_revision===1),t.kind);
  kind.disabled=t.keep||!!t.copyFrom;
  kind.onchange=()=>{t.kind=kind.value;this.channelFields();body.firstElementChild?.querySelector('select')?.focus();};
  body.append(this.label('通道类型',kind));
  if(t.existing){
   const replace=this.element('input');replace.type='checkbox';replace.checked=!t.keep;
   replace.onchange=()=>{t.keep=!replace.checked;this.channelFields();body.querySelector('input[type=checkbox]')?.focus();};
   const label=this.label('更换地址或凭据',replace,'fw-check');label.prepend(replace);body.append(label);
  }
  if(t.copyFrom){
   body.append(this.element('p','将沿用 '+this.channelName(t.copyFrom)+' 的地址和凭据。'));
   body.append(this.button('填写新的地址和凭据',()=>{t.copyFrom=null;this.channelFields();}));
   return;
  }
  if(t.keep){body.append(this.element('p','设备已保存此通道的凭据。'));return;}
  const secret=(name,label,max=256,placeholder='')=>this.field(t,name,label,{secret:true,max,placeholder});
  if(t.kind==='email'){
   for(const [name,label,max] of [['smtp_host','SMTP 服务器',253],['username','用户名',64],['password','密码或应用专用密码',256],['mail_from','发件地址',254],['rcpt_to','收件地址',254]])body.append(secret(name,label,max));
   body.append(this.element('p','使用 TLS，端口 465。记录中的“已确认”表示邮件服务器已接受。'));
  }else if(Object.hasOwn(chatKinds,t.kind)){
   const c=chatKinds[t.kind];if(c.endpoint)body.append(secret('endpoint',c.endpoint,2048));
   body.append(secret('key',c.key,2048));if(c.secret)body.append(secret('chat',c.secret,128));
   if(['dingtalk','wecom','feishu'].includes(t.kind))body.append(this.element('p','如果机器人设有关键词，请添加 RustSimHub。'));
  }else if(t.kind==='webhook'){
   body.append(secret('endpoint','Webhook HTTPS 地址',2048,'https://'),secret('bearer','Bearer 令牌（选填）',2048));
  }else if(t.kind==='bark'){
   body.append(secret('endpoint','Bark 服务地址',2048,'https://api.day.app/'),secret('key','设备 Key'));
  }else{
   body.append(secret('key','机器人 Token'),secret('chat','接收方 Chat ID',128));
  }
 }
 ruleFields(){
  const r=this.draft.row,body=this.$('fw-dialog-body');
  const group=this.element('fieldset');group.className='fw-channel-choices';group.append(this.element('legend','发送通道'));
  for(const t of this.view.targets){
   const box=this.element('input');box.type='checkbox';box.checked=r.targets.includes(t.id);
   box.onchange=()=>{r.targets=box.checked?[...r.targets,t.id]:r.targets.filter(id=>id!==t.id);};
   const label=this.label(this.channelName(t.id),box,'fw-check');label.prepend(box);group.append(label);
  }
  const extended=this.sample.rules_revision===2;
  const share=this.options([['notice','仅发送通知'],...(extended?[['masked_text','发送脱敏正文']]:[]),['full_text','发送完整正文和发送方']],r.full?'full_text':r.masked?'masked_text':'notice');
  share.onchange=()=>{r.full=share.value==='full_text';r.masked=share.value==='masked_text';};
  const match=this.element('div');match.className='fw-match';
  match.append(this.field(r,'sender','发送方等于',{placeholder:'不限发送方'}),this.field(r,'contains','短信正文包含',{placeholder:'不限关键词'}));
  const advanced=this.element('details');advanced.open=!!(r.regex||r.window||r.line);
  advanced.append(this.element('summary','高级筛选'),this.field(r,'line','SIM 代次',{max:16,placeholder:'不限 SIM 代次',mode:'numeric'}));
  if(extended){
   const timed=this.element('input');timed.type='checkbox';timed.checked=r.window;
   const times=this.element('div');times.className='fw-times';times.hidden=!r.window;
   timed.onchange=()=>{r.window=timed.checked;times.hidden=!r.window;};
   const label=this.label('限定短信时间段',timed,'fw-check');label.prepend(timed);
   times.append(this.field(r,'start','开始 HH:MM',{max:5}),this.field(r,'end','结束 HH:MM',{max:5}),this.field(r,'offset','UTC 偏移分钟',{max:4}));
   advanced.append(this.field(r,'regex','正文正则',{placeholder:'可留空'}),label,times);
  }
  body.replaceChildren(group,this.label('转发内容',share),match,this.element('p','填写的条件需同时满足。条件全部留空时匹配所有新短信。'),advanced);
 }
 payload(t){
  return {...t,...Object.fromEntries(['endpoint','bearer','key','chat',...emailFields].map(name=>[name,t[name+'Input']?.value??'']))};
 }
 async save(g){
  this.current(g);
  requireValue(this.ready&&!this.pending&&this.draft,'请先读取配置并核对上次请求');
  const {row,kind,existing}=this.draft;
  if(kind==='channel'&&row.keep){this.closeEditor();return;}
  const targets=this.view.targets.map(t=>t.id===row.id&&kind==='channel'?this.payload(row):this.target(t));
  const rules=this.view.rules.map(r=>r.id===row.id&&kind==='rule'?row:this.rule(r));
  if(!existing){if(kind==='channel')targets.push(this.payload(row));else rules.push(row);}
  const policy=this.policy(targets,rules);
  if(kind==='rule'&&row.full&&!this.confirm('允许这条规则向所选通道发送完整短信正文和发送方？'))return;
  if(kind==='channel'&&existing&&!row.keep&&policy.enabled&&!this.confirm('替换发送通道？新收到且符合规则的短信将发往新地址。'))return;
  await this.mutate(g,'configure',{op:'forwarding_configure',policy},kind==='channel'?'通道已保存。':'规则已保存。');
 }
 async remove(g){
  this.current(g);requireValue(this.ready&&!this.pending&&this.draft?.existing,'请先读取配置并核对上次请求');
  const {kind,row}=this.draft;const channel=kind==='channel';
  const uses=this.view.rules.filter(r=>r.targets.includes(row.id)).length;
  requireValue(!channel||uses===0,`此通道关联 ${uses} 条规则。请先编辑这些规则，取消关联。`);
  const targets=this.view.targets.filter(t=>!channel||t.id!==row.id).map(t=>this.target(t));
  const rules=this.view.rules.filter(r=>channel||r.id!==row.id).map(r=>this.rule(r));
  requireValue(!this.sample.enabled||rules.length>0,'请先关闭转发，再删除最后一条规则。');
  if(!this.confirm(channel?'删除这个发送通道？':'删除这条转发规则？'))return;
  await this.mutate(g,'configure',{op:'forwarding_configure',policy:this.policy(targets,rules)},channel?'通道已删除。':'规则已删除。');
 }
 policy(targets,rules){
  return policyInput(targets,rules,this.sample.enabled,this.sample.rules_revision??null,this.sample.email_revision??null);
 }
 async archive(record,applied,g){
  await this.archivePending({...record,applied_generation:applied,uncertain:applied===null});this.current(g);
  requireValue(this.pending?.id===record.id,'保存记录已变化，请重新读取');
  await this.savePending(null,record.id);this.current(g);this.pending=null;
 }
 async mutate(g,kind,fields,description){
  this.current(g);requireValue(this.ready&&this.sample&&!this.pending,'请先读取状态并核对上次保存');
  requireValue(this.sample.generation<Number.MAX_SAFE_INTEGER,'配置代次超出网页可编辑范围，请使用维护入口');
  const record={format:1,id:this.crypto.randomUUID(),kind,expected_generation:this.sample.generation};
  this.clearSecrets();
  try{await this.savePending(record,null);}catch{throw new FormError('无法保存本机回执，本次未提交。请刷新配置并重新填写凭据。');}this.current(g);this.pending=record;this.update();
  const result=await this.request({...fields,expected_generation:record.expected_generation,request_id:record.id});this.current(g);
  requireValue(result.configuration_applied===true&&result.request_id===record.id&&result.generation===record.expected_generation+1,'保存回执不匹配，请核对上次保存');
  this.status(result);await this.archive(record,result.generation,g);
  this.closeEditor();
  this.ready=false;this.sample=result;this.view=null;this.render();
  const view=viewInput(await this.observe(g,{op:'forwarding_rules'}),result);this.current(g);
  this.view=view;this.ready=this.manageable(result);this.render();
  this.message(description??processText(result)+'。');
 }
 async toggle(g){
  this.current(g);requireValue(this.ready&&!this.pending,'请先读取状态并核对上次保存');
  requireValue(this.sample&&this.view,'请先读取状态');const enabled=!this.sample.enabled;
  requireValue(!enabled||this.view.targets.length>0&&this.view.rules.length>0,'请先保存至少一个目标和一条规则');
  if(!this.confirm((enabled?'开启短信转发？默认只处理之后的新短信。':'关闭短信转发并停止转发进程？')+(enabled&&this.view.rules.some(r=>r.share==='full_text')?'现有规则会向目标发送完整短信正文。':'')))return;
  await this.mutate(g,enabled?'enable':'disable',{op:'set_forwarding_enabled',enabled});
 }
 async query(g){
  this.current(g);
  requireValue(!!this.pending,'没有待核对的保存');const record=this.pending;
  if(testKind(record.kind)){await this.inspectTest(g,record,await this.observe(g,{op:'forwarding_test_request',request_id:record.id}));return;}
  const result=await this.observe(g,{op:'forwarding_request',request_id:record.id});this.current(g);
  requireValue(result.request_id===record.id&&typeof result.known==='boolean'&&result.not_found_is_not_proof===true&&generation(result.current_generation));
  if(!result.known){this.message('没有找到这笔保存回执；记录可能未受理、已清理或来自恢复前。不会重提，记录继续保留。');return;}
  requireValue(generation(result.applied_generation)&&result.applied_generation===record.expected_generation+1&&result.applied_generation<=result.current_generation);
  await this.archive(record,result.applied_generation,g);const sample=await this.load(g);this.message('已核对并保存回执。'+processText(sample)+'。');
 }
 async inspectTest(g,record,result){
  this.current(g);requireValue(result.request_id===record.id&&(result.accepted===true||typeof result.known==='boolean'));
  if(result.known===false){requireValue(result.not_found_is_not_proof===true);this.message('没有找到这笔合成测试回执；不代表从未外发，不会重新提交。');return true;}
  const r=result.receipt;requireValue(r&&r.id===record.id&&r.target_id===record.target_id&&r.generation===record.expected_generation&&r.synthetic===true&&Object.hasOwn(states,r.state));
  if(record.kind==='quick_test'&&result.automatic_stop_verified!==true){
   if(result.quick_active===true||result.accepted===true){this.message('正在核对测试回执和服务停止状态…');return false;}
   this.message('测试已停止，转发进程关闭尚待核对。');return true;
  }
  if(['confirmed','failed','cancelled'].includes(r.state)){
   await this.archive(record,r.generation,g);this.receiptsLoaded=false;await this.load(g);
   const outcome=r.state==='confirmed'?'合成测试已获目标确认。':'合成测试'+states[r.state]+'。';
   this.message(outcome+(record.kind==='quick_test'?'已确认转发服务回到关闭状态。':''));return true;
  }
  if(r.state==='unknown'){this.message('合成测试结果不确定，回执已保留；不会自动重发。');return true;}
  this.message('合成测试'+states[r.state]+'，正在核对回执；只提交一次。');return false;
 }
 async test(g,target){
  this.current(g);requireValue(this.ready&&!this.pending&&this.testReady,'请先刷新服务状态');
  requireValue(this.view.targets.some(t=>t.id===target),'请选择已保存的测试通道');
  const quick=this.quickTestReady;
  if(!this.confirm('向 '+this.channelName(target)+' 发送一条合成测试通知？'+(quick?'短信转发总开关保持关闭。':'')))return;
  const record={format:1,id:this.crypto.randomUUID(),kind:quick?'quick_test':'test',expected_generation:this.sample.generation,target_id:target};
  this.clearSecrets();
  try{await this.savePending(record,null);}catch{throw new FormError('本机回执未能保存或另一页面已有请求；本次未提交测试');}this.current(g);this.pending=record;this.update();
  const deadline=Date.now()+(quick?36000:15000);
  const result=await this.request({op:quick?'forwarding_quick_test':'forwarding_test',expected_generation:record.expected_generation,request_id:record.id,target_id:target});
  if(await this.inspectTest(g,record,result))return;
  for(let i=0;i<(quick?35:10)&&Date.now()<deadline;i++){
   await this.delay();this.current(g);if(Date.now()>=deadline)break;
   const receipt=await readDuringPoll(()=>this.request({op:'forwarding_test_request',request_id:record.id}));
   if(receipt!==undefined&&await this.inspectTest(g,record,receipt))return;
  }
  this.message('合成测试仍在处理，回执已保留；可点击核对上次请求，不会重新提交。');
 }
 async archiveUnknown(g){
  this.current(g);
  requireValue(!!this.pending,'没有待核对的保存');
  if(!this.confirm('将这笔未确认保存保留在本机回执中，再读取当前配置？这不会撤回、重发或证明旧请求未执行。'))return;
  const s=await this.load(g);
  requireValue(s.quick_test_active!==true,'合成测试仍在运行，结束后再归档。');
  await this.archive(this.pending,null,g);this.message('未确认记录已保留；'+processText(s)+'。后续改动使用当前配置代次。');
 }
 resetLogFilter(){
  for(const id of ['fw-record-filter','fw-log-kind','fw-log-target'])this.$(id).value='all';
 }
 logFilter(){
  const filter={};
  for(const [id,key] of [['fw-record-filter','state'],['fw-log-kind','kind'],['fw-log-target','target']]){
   const value=this.$(id).value;
   if(value&&value!=='all')filter[key]=value;
  }
  return filter;
 }
 terminalReceipt(r){return ['confirmed','failed','cancelled'].includes(r.state);}
 async clearLog(g,receipt=null){
  this.current(g);
  const {through,removable}=this.logSnapshot??{};
  requireValue(this.ready&&!this.pending&&removable>0&&(!receipt||this.terminalReceipt(receipt)),'没有可删除的记录，请先刷新');
  const scope=receipt?'这条转发记录':Object.keys(this.logFilter()).length?'符合筛选的已结束记录':'全部已结束的转发记录';
  if(!this.confirm('删除'+scope+'？正在发送和待核对的记录会保留。'))return;
  const result=await this.request({op:'forwarding_log_clear',through,only:receipt?.seq??null,filter:this.logFilter()});
  this.current(g);
  requireValue(Number.isSafeInteger(result.changed)&&result.changed>=0,'删除结果待核对，请刷新记录');
  this.closeEditor();
  await this.receipts(g,true);
  this.message('已删除 '+result.changed+' 条转发记录。');
 }
 renderReceiptEmpty(){
  const empty=this.$('fw-record-empty'),next=this.$('fw-next'),hint=this.$('fw-record-hint');
  empty.hidden=this.receiptRows.length>0;
  next.hidden=true;hint.textContent='';this.nextPage='channels';
  const title=this.$('forwarding-receipts-empty');
  if(!this.receiptsLoaded){
   title.textContent=this.busy?'正在读取…':'还未读取转发记录';
   hint.textContent=this.getToken()?'点“刷新记录”重试。':'用所有者浏览器登录后查看。';
  }else if(Object.keys(this.logFilter()).length>0){
   title.textContent='没有匹配的转发记录';
   hint.textContent=this.more?'可以继续加载更早的记录。':'';
  }else{
   title.textContent='还没有转发记录';
   if(!this.ready)return;
   if(!this.view.targets.length){
    hint.textContent='先添加一个接收短信通知的发送通道。';next.textContent='去添加通道';
   }else if(!this.view.rules.length){
    hint.textContent='为已保存的通道添加一条短信匹配规则。';next.textContent='去添加规则';this.nextPage='rules';
   }else if(!this.sample.enabled){
    hint.textContent='通道和规则已就绪，可以在设置中开启转发。';next.textContent='去转发设置';this.nextPage='settings';
   }else{
    hint.textContent='符合规则的新短信或已提交的测试会出现在这里。';return;
   }
   next.hidden=false;
  }
 }
 renderReceipts(){
  this.$('forwarding-receipts').replaceChildren(...this.receiptRows.map(r=>{
   const {row,actions}=this.listRow(r.synthetic?'测':'信',r.synthetic?'合成测试':'短信转发',this.channelName(r.target_id)+' · '+(r.recorded_at===null?'记录 #'+r.seq:new Date(r.recorded_at*1000).toLocaleString()));
   const badge=this.element('span',r.state==='unknown'?'待核对':states[r.state]);badge.className='fw-receipt-status fw-'+r.state;
   actions.append(badge,this.button('详情',event=>this.receiptDetails(r,event.currentTarget)));
   return row;
  }));
  this.renderReceiptEmpty();
 }
 receiptDetails(r,trigger){
  this.lastTrigger=trigger;this.openReceipt=r;
  this.$('fw-dialog-title').textContent=r.synthetic?'合成测试详情':'转发详情';
  this.$('fw-dialog-remove').hidden=!this.terminalReceipt(r);this.$('fw-dialog-remove').textContent='删除记录';this.$('forwarding-save').hidden=true;this.$('fw-dialog-cancel').textContent='完成';
  const fields=this.element('dl');fields.className='fw-details';
  for(const [label,value] of [
   ['状态',states[r.state]],['发送通道',this.channelName(r.target_id)],['记录时间',r.recorded_at===null?'未记录':new Date(r.recorded_at*1000).toLocaleString()],['SIM 代次',String(r.line)],['记录编号',r.seq],['回执 ID',r.id],
  ])fields.append(this.element('dt',label),this.element('dd',value));
  this.$('fw-dialog-body').replaceChildren(fields);
  this.$('fw-dialog').showModal();
 }
 async receipts(g,reset){
  this.current(g);
  const before=reset?null:this.cursor;
  if(reset){this.logSnapshot=null;this.receiptsLoaded=false;this.receiptRows=[];this.cursor=null;this.more=false;this.renderReceipts();}
  const result=await this.observe(g,{op:'forwarding_log_page',before,limit:16,filter:this.logFilter()});this.current(g);
  requireValue(Array.isArray(result.records)&&result.records.length<=16&&sequence(result.through)&&Number.isSafeInteger(result.removable)&&result.removable>=0);
  if(reset)this.logSnapshot={through:result.through,removable:result.removable};
  let cursor=before;
  const rows=result.records.map(r=>{
   requireValue(typeof r.seq==='string'&&/^[1-9][0-9]{0,18}$/.test(r.seq)&&BigInt(r.seq)<=9223372036854775807n&&
    (cursor===null||BigInt(r.seq)<BigInt(cursor))&&uuid(r.id)&&uuid(r.message_id)&&uuid(r.target_id)&&Object.hasOwn(states,r.state));
   cursor=r.seq;
   return {seq:r.seq,id:r.id,target_id:r.target_id,state:r.state,synthetic:r.synthetic===true,line:generation(r.line)?r.line:'—',recorded_at:generation(r.recorded_at)?r.recorded_at:null};
  });
  this.receiptRows=reset?rows:this.receiptRows.concat(rows);
  this.cursor=cursor;this.more=rows.length===16&&cursor!=='1';this.receiptsLoaded=true;
  this.renderReceipts();
  if(!reset&&!rows.length)this.message('没有更早的转发记录。');
 }

}

// A single explicit button owns only the browser receipt. Core owns the finite
// test window and its cleanup even when this page disappears or logs out.
export class ForwardingQuickTest {
 constructor({login,request,loadPending,savePending,archivePending,notify,crypto=globalThis.crypto,delay=()=>new Promise(resolve=>setTimeout(resolve,1000))}){
  Object.assign(this,{login,request,loadPending,savePending,archivePending,notify,crypto,delay});this.busy=false;
 }
 async recoverConfiguration(record){
  const r=await this.request({op:'forwarding_request',request_id:record.id});
  requireValue(r.request_id===record.id&&typeof r.known==='boolean'&&generation(r.current_generation)&&r.not_found_is_not_proof===true,'上次配置回执未获确认；本次没有发送。');
  let applied=null;
  if(r.known){requireValue(r.applied_generation===record.expected_generation+1&&r.applied_generation<=r.current_generation,'上次配置回执不匹配；本次没有发送。');applied=r.applied_generation;}
  else {
   const s=await this.request({op:'forwarding_status'});
   requireValue(s.quick_test_api===true&&s.enabled===false&&s.quick_test_active===false&&s.state==='disabled'&&s.process_state_verified===true&&s.generation===r.current_generation&&s.generation>=record.expected_generation,'上次配置结果仍需核对；本次没有发送。');
  }
  // Missing configuration receipts remain explicitly uncertain in history.
  // Never replay the old switch/rule write; the probe is a separate, OFF-only,
  // generation-checked operation and cannot turn ordinary forwarding on.
  await this.archivePending({...record,applied_generation:applied,uncertain:applied===null});
  await this.savePending(null,record.id);
 }
 async run(){
  if(this.busy)return;this.busy=true;
  try{
   this.notify('正在自动登录并检查 Bark 目标…');await this.login();
   let record=pendingInput(await this.loadPending());
   if(record&&!testKind(record.kind)){this.notify('正在自动核对并保存上次配置回执；不会重做旧操作…');await this.recoverConfiguration(record);record=null;}
   if(record){this.notify('正在核对上一次测试，不会重新发送…');}
   else {
    const s=await this.request({op:'forwarding_status'});
    requireValue(s.quick_test_api===true&&s.service_management_api===true,'当前设备尚未支持一键转发测试，请等待新版部署。');
    requireValue(s.enabled===false&&s.process_state_verified===true&&s.state==='disabled'&&s.quick_test_active===false,'转发尚未确认关闭，或另一测试正在运行；本次未发送。');
    const v=await this.request({op:'forwarding_rules'});
    requireValue(v.generation===s.generation&&v.enabled===false&&Array.isArray(v.targets),'配置已变化；本次未发送。');
    const targets=v.targets.filter(t=>t.kind==='bark'&&t.credentials_redacted===true&&uuid(t.id));
    requireValue(targets.length===1,'请先保存一个 Bark 目标；多个 Bark 目标需先选定，避免发错设备。');
    record=pendingInput({format:1,id:this.crypto.randomUUID(),kind:'quick_test',expected_generation:s.generation,target_id:targets[0].id});
    await this.savePending(record,null);
    this.notify('正在临时准备 Bark 测试；不会开启真实短信转发…');
    const r=await this.request({op:'forwarding_quick_test',request_id:record.id,expected_generation:record.expected_generation,target_id:record.target_id});
    requireValue(r.accepted===true&&r.quick_test===true&&r.request_id===record.id,'测试受理结果未确认；回执已保留，不会重新发送。');
   }
   const deadline=Date.now()+65000;
   for(let attempt=0;attempt<65&&Date.now()<deadline;attempt++){
    const r=await readDuringPoll(()=>this.request({op:'forwarding_test_request',request_id:record.id}));
    if(r===undefined){await this.delay();continue;}
    requireValue(r.request_id===record.id&&typeof r.known==='boolean','测试回执无效；不会重新发送。');
    requireValue(r.known===true,'没有找到这笔测试回执；不代表未外发，记录已保留，不会重新发送。');
    const receipt=r.receipt;
    requireValue(receipt&&receipt.id===record.id&&receipt.target_id===record.target_id&&receipt.generation===record.expected_generation&&receipt.synthetic===true&&Object.hasOwn(states,receipt.state)&&typeof r.quick_active==='boolean','测试回执不匹配；不会重新发送。');
    if(r.quick_active===false){
     let closed=r.automatic_stop_verified===true;
     if(!closed){
      const s=await readDuringPoll(()=>this.request({op:'forwarding_status'}));
      if(s===undefined){await this.delay();continue;}
      closed=s.quick_test_active===false&&s.enabled===false&&s.state==='disabled'&&s.process_state_verified===true;
     }
     requireValue(closed,'测试已停止，但转发进程关闭尚未确认；回执已保留，请勿重复测试。');
     if(['confirmed','failed','cancelled'].includes(receipt.state)){
      await this.archivePending({...record,applied_generation:record.expected_generation,uncertain:false});
      await this.savePending(null,record.id);
      this.notify(receipt.state==='confirmed'?(record.kind==='quick_test'?'测试完成：Bark 已确认，转发已自动关闭。请看手机是否收到“RustSimHub 合成测试”。':'上次合成测试已确认，转发已关闭；本次没有再次发送。'):`测试${states[receipt.state]}；${record.kind==='quick_test'?'转发已自动关闭':'转发已确认关闭'}，不会自动重发。`);return;
     }
     this.notify('测试结果未确认；转发已关闭，回执已保留，不会自动重发。');return;
    }
    this.notify(receipt.state==='confirmed'?'Bark 已确认，正在自动关闭测试进程…':'正在发送并核对一次合成通知；结束后自动关闭…');
    await this.delay();
   }
   this.notify('等待测试结果超时；回执已保留，不会重复发送。设备会按测试期限自动收尾。');
  }catch(error){this.notify(error instanceof FormError?error.message:'测试未获确认；回执已保留，不会自动重发。已受理的设备测试会自动收尾。');}
  finally{this.busy=false;}
 }
}

// Historical batches have separate durable markers. Refresh/read never repeats
// either mutation, and the server challenge also fences archived/reused IDs.
export function historyPendingInput(value){
 if(value===null||value===undefined)return null;
 requireValue(value&&value.format===1&&uuid(value.id)&&generation(value.expected_generation)&&value.expected_generation>0&&(value.confirmation_id===null||uuid(value.confirmation_id))&&typeof value.submit_started==='boolean'&&typeof value.archive_started==='boolean'&&(!value.submit_started||uuid(value.confirmation_id))&&Object.keys(value).every(k=>['format','id','expected_generation','confirmation_id','submit_started','archive_started'].includes(k)),'历史补发记录损坏；请保留记录并通过维护入口核对');
 return {format:1,id:value.id,expected_generation:value.expected_generation,confirmation_id:value.confirmation_id,submit_started:value.submit_started,archive_started:value.archive_started};
}
const sequence=s=>typeof s==='string'&&/^(0|[1-9][0-9]{0,18})$/.test(s)&&BigInt(s)<=9223372036854775807n;
const shares={notice:'仅通知',masked_text:'正文隐藏英数字',full_text:'完整正文和发送方'};
const batchStates={prepared:'等待确认',submitted:'已入队',cancelled:'已取消',expired:'预览已失效'};
export function historyReceiptInput(value,record){
 requireValue(value&&Object.hasOwn(batchStates,value.state)&&value.not_found_is_not_proof===true&&typeof value.missing_receipts==='boolean'&&generation(value.expires_in_seconds)&&value.expires_in_seconds<=300&&Array.isArray(value.receipts)&&value.receipts.length<=64);
 const p=value.preview;
 requireValue(p&&p.id===record.id&&p.generation===record.expected_generation&&uuid(p.confirmation_id)&&(!record.confirmation_id||p.confirmation_id===record.confirmation_id)&&generation(p.line)&&p.line>0&&uuid(p.cursor?.db_epoch)&&sequence(p.cursor.seq)&&generation(p.scanned)&&p.scanned<=32&&generation(p.skipped_messages)&&p.skipped_messages<=p.scanned&&generation(p.already_queued_or_recorded)&&p.already_queued_or_recorded<=256&&Array.isArray(p.deliveries)&&p.deliveries.length<=64);
 const r=p.range;requireValue(r&&generation(r.limit)&&r.limit>=1&&r.limit<=32&&Array.isArray(r.targets)&&r.targets.length>=1&&r.targets.length<=8&&r.targets.every(uuid)&&new Set(r.targets).size===r.targets.length&&(r.after===null||r.after&&r.after.db_epoch===p.cursor.db_epoch&&sequence(r.after.seq)&&BigInt(p.cursor.seq)>=BigInt(r.after.seq)));
 const seen=new Set();const deliveries=p.deliveries.map(d=>{requireValue(d&&uuid(d.id)&&!seen.has(d.id)&&uuid(d.message_id)&&r.targets.includes(d.target_id)&&uuid(d.rule_id)&&Object.hasOwn(shares,d.share));seen.add(d.id);return {id:d.id,message_id:d.message_id,target_id:d.target_id,rule_id:d.rule_id,share:d.share};});
 const receipts=value.receipts.map(x=>{const d=deliveries.find(d=>d.id===x?.id);requireValue(d&&sequence(x.seq)&&x.message_id===d.message_id&&x.target_id===d.target_id&&x.rule_id===d.rule_id&&x.generation===p.generation&&x.synthetic===false&&Object.hasOwn(states,x.state));return {seq:x.seq,id:x.id,message_id:x.message_id,target_id:x.target_id,rule_id:x.rule_id,generation:x.generation,synthetic:false,state:x.state};});
 requireValue(new Set(receipts.map(r=>r.id)).size===receipts.length&&(value.state==='submitted'?value.missing_receipts||receipts.length===deliveries.length:receipts.length===0));
 return {preview:{id:p.id,confirmation_id:p.confirmation_id,generation:p.generation,line:p.line,range:{after:r.after?{db_epoch:r.after.db_epoch,seq:r.after.seq}:null,limit:r.limit,targets:r.targets.slice()},cursor:{db_epoch:p.cursor.db_epoch,seq:p.cursor.seq},scanned:p.scanned,skipped_messages:p.skipped_messages,already_queued_or_recorded:p.already_queued_or_recorded,deliveries},state:value.state,expires_in_seconds:value.expires_in_seconds,receipts,missing_receipts:value.missing_receipts,not_found_is_not_proof:true};
}
export class ForwardingHistory {
 constructor({document,request,getToken,confirm,loadPending,savePending,archivePending,loadArchives,crypto=globalThis.crypto}){
  Object.assign(this,{doc:document,request,getToken,confirm,loadPending,savePending,archivePending,loadArchives,crypto});this.epoch=0;
  for(const name of ['prepare','commit','cancel','query','archive'])this.$('forwarding-history-'+name).onclick=()=>this.action(g=>this[name](g));
  this.reset();
 }
 $(id){return this.doc.getElementById(id);}
 current(g){requireValue(g===this.epoch&&!!this.getToken(),'会话已变化，请重新读取历史补发状态');}
 reset(){this.epoch++;this.busy=false;this.sample=null;this.pending=null;this.result=null;this.view=null;this.nextCursor=null;this.$('forwarding-history-targets').replaceChildren();this.$('forwarding-history-preview').replaceChildren();this.$('forwarding-history-full').checked=false;this.message('点击管理登录并读取，选择范围和目标，再预览。没有自动补发。');this.update();}
 message(s){this.$('forwarding-history-state').textContent=s;}
 update(){
  const ready=!!this.sample&&!this.busy,prepared=this.result?.state==='prepared'&&!!this.pending?.confirmation_id&&!this.pending.submit_started&&!this.pending.archive_started;
  this.$('forwarding-history-login').disabled=this.busy;
  for(const suffix of ['after','limit','targets'])this.$('forwarding-history-'+suffix).disabled=!ready||!!this.pending;
  this.$('forwarding-history-prepare').disabled=!ready||!!this.pending||this.sample?.quick_test_active===true;
  this.$('forwarding-history-commit').disabled=!ready||!prepared||!this.result?.preview.deliveries.length||!this.sample.enabled||this.pending.expected_generation!==this.sample.generation||this.result.expires_in_seconds===0;
  this.$('forwarding-history-cancel').disabled=!ready||this.result?.state!=='prepared'||!this.pending?.confirmation_id||this.pending.archive_started;
  this.$('forwarding-history-query').disabled=this.busy||!this.pending;
  this.$('forwarding-history-archive').disabled=!ready||!this.pending||this.result?.state==='prepared';
  this.$('forwarding-history-full').disabled=!ready||!prepared||!this.result?.preview.deliveries.some(d=>d.share==='full_text');
 }
 async action(fn){if(this.busy)return;const g=this.epoch;this.busy=true;this.update();try{await fn(g);}catch(e){if(g===this.epoch)this.message(e instanceof FormError?e.message:'操作结果未确认；请核对本批回执。记录已保留，不会重新提交。');}finally{if(g===this.epoch){this.busy=false;this.update();}}}
 async replace(next,expected,g){this.current(g);await this.savePending(next,expected);this.current(g);this.pending=next;}
 rangeOptions(next=null){
  if(next&&next.db_epoch===this.sample?.history_realm&&sequence(next.seq))this.nextCursor=next;
  const select=this.$('forwarding-history-after');select.replaceChildren();const first=this.doc.createElement('option');first.value='0';first.textContent='从最早保存的短信开始';select.append(first);
  if(this.nextCursor?.db_epoch===this.sample?.history_realm&&this.nextCursor.seq!=='0'){const option=this.doc.createElement('option');option.value=this.nextCursor.seq;option.textContent='继续上一批之后的短信';select.append(option);select.value=option.value;}else select.value='0';
 }
 async read(g){
  this.current(g);this.pending=historyPendingInput(await this.loadPending());this.current(g);this.result=null;
  const s=await this.request({op:'forwarding_status'});this.current(g);
  requireValue(s?.history_api===true&&s.history_revision===2&&uuid(s.history_realm)&&generation(s.generation)&&typeof s.enabled==='boolean','此设备尚不支持这版历史补发，请保持原记录并升级');
  const v=viewInput(await this.request({op:'forwarding_rules'}),s);this.current(g);this.sample=s;this.view=v;this.rangeOptions();
  const select=this.$('forwarding-history-targets');select.replaceChildren();
  for(const [i,t] of v.targets.entries()){const o=this.doc.createElement('option');o.value=t.id;o.textContent=`目标 ${i+1} · ${kinds[t.kind]} · ${t.id.slice(0,8)}`;o.selected=v.targets.length===1;select.append(o);}
  if(this.pending)await this.query(g);else{this.render();this.message(s.enabled?'选择本批范围和目标，先预览；不会自动发送。':'转发关闭。可预览；如需补发，请先在上方开启，再读取并重新预览。');}await this.showArchives(g);
 }
 async accept(value,g){
  this.current(g);const r=historyReceiptInput(value,this.pending);
  if(!this.pending.confirmation_id){const old=this.pending;await this.replace({...old,confirmation_id:r.preview.confirmation_id},old,g);}
  this.current(g);this.result=r;this.rangeOptions(r.preview.range.after);this.$('forwarding-history-after').value=r.preview.range.after?.seq??'0';this.$('forwarding-history-limit').value=String(r.preview.range.limit);for(const o of this.$('forwarding-history-targets').options)o.selected=r.preview.range.targets.includes(o.value);this.$('forwarding-history-full').checked=false;this.render();
  this.message(r.state==='prepared'?(this.pending.submit_started?'提交结果尚需核对；设备仍显示预览。不会再次提交，可取消后核对。':`预览已准备：${r.preview.deliveries.length}个递送；确认前不会发送。`):r.state==='submitted'?'本批已入队。请核对逐项回执；Unknown不会自动重投。':batchStates[r.state]+'，没有因此再次发送。');
 }
 async prepare(g){
  this.current(g);requireValue(this.sample&&!this.pending&&this.view,'请先读取并核对上次补发');
  const seq=this.$('forwarding-history-after').value,limit=Number(this.$('forwarding-history-limit').value),targets=Array.from(this.$('forwarding-history-targets').options).filter(o=>o.selected).map(o=>o.value);
  requireValue(sequence(seq)&&Number.isSafeInteger(limit)&&limit>=1&&limit<=32&&targets.length>=1&&targets.length<=8&&targets.every(id=>this.view.targets.some(t=>t.id===id)),'请填写有效列表位置、1～32条范围，并选择目标');
  const record={format:1,id:this.crypto.randomUUID(),expected_generation:this.sample.generation,confirmation_id:null,submit_started:false,archive_started:false};historyPendingInput(record);
  await this.replace(record,null,g);
  await this.accept(await this.request({op:'forwarding_history_prepare',expected_generation:record.expected_generation,request_id:record.id,range:{after:seq==='0'?null:{db_epoch:this.sample.history_realm,seq},limit,targets}}),g);
 }
 async commit(g){
  this.current(g);const old=this.pending,r=this.result;
  requireValue(old&&r?.state==='prepared'&&!old.submit_started&&!old.archive_started&&r.expires_in_seconds>0&&r.preview.deliveries.length&&this.sample.enabled&&old.expected_generation===this.sample.generation,'请先核对有效预览；不能重复确认或在关闭时补发');
  const full=r.preview.deliveries.some(d=>d.share==='full_text');requireValue(!full||this.$('forwarding-history-full').checked,'本批包含完整正文；请先明确勾选同意');
  if(!this.confirm(`按预览向已选目标补发${r.preview.deliveries.length}个递送？${full?'包含完整正文和发送方。':''}已发出的请求无法撤回；仅提交一次。`))return;
  const record={...old,submit_started:true};await this.replace(record,old,g);
  await this.accept(await this.request({op:'forwarding_history_commit',expected_generation:record.expected_generation,request_id:record.id,confirmation_id:record.confirmation_id,accept_full_text:full}),g);
 }
 async cancel(g){
  this.current(g);requireValue(this.pending?.confirmation_id&&this.result?.state==='prepared'&&!this.pending.archive_started,'只可取消未提交预览');
  if(!this.confirm('取消这份未提交预览？如提交已被受理，设备会拒绝取消；请继续核对。'))return;
  await this.accept(await this.request({op:'forwarding_history_cancel',request_id:this.pending.id,confirmation_id:this.pending.confirmation_id}),g);
 }
 async query(g){
  this.current(g);requireValue(this.pending,'没有本批记录，请先读取');const record=this.pending;
  const v=await this.request({op:'forwarding_history_request',request_id:record.id});this.current(g);
  requireValue(v?.request_id===record.id&&typeof v.known==='boolean'&&v.not_found_is_not_proof===true);
  if(!v.known){this.result=null;this.render();this.message('没有找到本批索引；不代表从未入队或外发。记录继续保留，可保存不确定摘要后继续；不会重新提交。');return;}
  await this.accept(v.receipt,g);
 }
 async archive(g){
  this.current(g);const old=this.pending,r=this.result;requireValue(old&&r?.state!=='prepared'&&this.sample,'先取消未提交预览，或查询本批回执');
  const uncertain=!r||r.missing_receipts||r.receipts.some(x=>['queued','in_flight','unknown'].includes(x.state));
  if(!this.confirm(uncertain?'将本批不确定摘要保存在浏览器并继续？设备索引和Unknown保持，不会再次发送。':'保存本批摘要并归档设备批次索引？递送回执保留，不会重新发送。'))return;
  await this.archivePending({record:old,state:r?.state??'not_found',uncertain});this.current(g);
  if(!uncertain){
   const marker={...old,archive_started:true};await this.replace(marker,old,g);
   const ack=await this.request({op:'forwarding_history_archive',expected_generation:this.sample.generation,request_id:marker.id,confirmation_id:marker.confirmation_id});this.current(g);
   requireValue(ack?.archived===true&&ack.jobs_preserved===true&&ack.request_id===marker.id&&ack.confirmation_id===marker.confirmation_id,'归档未获确认；摘要已保留，请查询');
   await this.replace(null,marker,g);
  }else await this.replace(null,old,g);
  this.rangeOptions(r?.preview.cursor);this.result=null;this.render();this.message(uncertain?'不确定摘要已保留，设备索引和递送回执保持；不会自动重投。':'本批摘要已保存并归档；设备递送回执保持。');await this.showArchives(g);
 }
 render(){
  const area=this.$('forwarding-history-preview');area.replaceChildren();if(!this.result)return;
  const r=this.result,p=r.preview;const summary=this.doc.createElement('p');summary.textContent=`${batchStates[r.state]} · 查看${p.scanned}条 · 跳过${p.skipped_messages}条 · 已有任务/回执${p.already_queued_or_recorded}个 · ${p.deliveries.length}个递送`;area.append(summary);
  for(const [i,d] of p.deliveries.entries()){const row=this.doc.createElement('p');const index=this.view?.targets.findIndex(t=>t.id===d.target_id);const receipt=r.receipts.find(x=>x.id===d.id);row.textContent=`第${i+1}条短信 → 目标 ${index>=0?index+1:d.target_id} · ${shares[d.share]} · ${receipt?states[receipt.state]:r.state==='submitted'?'回执缺失，不能推断未发送':'尚未入队'}`;const details=this.doc.createElement('details'),label=this.doc.createElement('summary'),id=this.doc.createElement('p');label.textContent='查看记录标识';id.className='mono';id.textContent='短信 '+d.message_id+' · 递送 '+d.id;details.append(label,id);area.append(row,details);}
 }
 async showArchives(g){
  const rows=await this.loadArchives();this.current(g);requireValue(Array.isArray(rows)&&rows.length<=64,'保存的补发摘要无效');
  const area=this.$('forwarding-history-saved');area.replaceChildren();
  for(const entry of rows){const record=historyPendingInput(entry.record);const row=this.doc.createElement('div');row.className='row mono';row.textContent=`${record.id} · ${entry.uncertain?'未决摘要保留':batchStates[entry.state]??'已保存'} `;const button=this.doc.createElement('button');button.textContent='核对';button.disabled=!!this.pending;button.onclick=()=>this.action(async g=>{requireValue(!this.pending,'请先处理当前批次');await this.replace(record,null,g);await this.query(g);});row.append(button);area.append(row);}
 }
}
