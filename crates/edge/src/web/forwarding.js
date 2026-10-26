// SPDX-License-Identifier: AGPL-3.0-only
// No passive polling or automatic mutation. Explicit synthetic tests have a
// bounded read-only receipt observation. Credentials stay in password inputs.
class FormError extends Error {}
const uuid=s=>typeof s==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s);
const generation=n=>Number.isSafeInteger(n)&&n>=0;
const testKind=kind=>kind==='test'||kind==='quick_test';
const kinds={webhook:'Webhook',bark:'Bark',telegram:'Telegram'};
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
function privacyText(rules){
 if(rules.some(r=>r.share==='full_text'))return '现有规则包含完整正文转发';
 if(rules.some(r=>r.share==='masked_text'))return '部分规则转发正文并隐藏英文字母和数字；不转发原始发送方';
 return '现有规则仅发送新短信通知';
}
export function policyInput(targets,rules,enabled,rulesRevision=null){
 requireValue(rulesRevision===null||rulesRevision===2,'规则版本不支持，请更新页面');
 requireValue(typeof enabled==='boolean','开关状态无效');
 requireValue(Array.isArray(targets)&&targets.length<=8&&Array.isArray(rules)&&rules.length<=32,'最多8个目标、32条规则');
 const ids=new Set();
 const selected=targets.map(t=>{
  requireValue(uuid(t.id)&&!ids.has(t.id)&&Object.hasOwn(kinds,t.kind),'目标无效');ids.add(t.id);
  let destination;
  if(t.keep){requireValue(t.existing===true,'新目标需要填写凭据');destination={kind:'keep'};}
  else if(t.kind==='webhook'){
   requireValue(!t.bearer||text(t.bearer,2048,false)&&/^[\x21-\x7e]+$/.test(t.bearer),'Webhook Bearer 格式无效');
   destination={kind:'webhook',url:https(t.endpoint),bearer:t.bearer||null};
  }else if(t.kind==='bark'){
   requireValue(text(t.key,256,false),'请输入 Bark key（最多256字节）');
   destination={kind:'bark',server:https(t.endpoint,true),key:t.key};
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
 policyInput(v.targets.map(t=>({id:t.id,kind:t.kind,existing:true,keep:true})),v.rules.map(ruleRow),v.enabled,s.rules_revision??null);
 for(const r of v.rules)requireValue(['notice','masked_text','full_text'].includes(r.share));
 return v;
}
export class ForwardingPanel {
 constructor({document,request,getToken,confirm,loadPending,savePending,archivePending,crypto=globalThis.crypto,delay=()=>new Promise(resolve=>setTimeout(resolve,1000))}){
  Object.assign(this,{doc:document,request,getToken,confirm,loadPending,savePending,archivePending,crypto,delay});this.epoch=0;this.secretInputs=new Set();
  for(const [id,fn] of [['forwarding-refresh',g=>this.read(g)],['forwarding-toggle',g=>this.toggle(g)],['forwarding-save',g=>this.save(g)],['forwarding-query',g=>this.query(g)],['forwarding-archive',g=>this.archiveUnknown(g)],['forwarding-receipts-refresh',g=>this.receipts(g,true)],['forwarding-receipts-more',g=>this.receipts(g,false)]])this.$(id).onclick=()=>this.action(fn);
  this.$('forwarding-add-target').onclick=()=>{if(this.busy||!this.ready||this.pending||this.targets.length>=8)return;this.targets.push(this.target());this.dirty=true;this.render();};
  this.$('forwarding-add-rule').onclick=()=>{if(this.busy||!this.ready||this.pending||this.rules.length>=32)return;this.rules.push(this.rule());this.dirty=true;this.render();};
  this.$('forwarding-test').onclick=()=>this.action(g=>this.test(g));
  this.reset();
 }
 $(id){return this.doc.getElementById(id);}
 current(g){if(g!==this.epoch||!this.getToken())throw new FormError('会话已变化，请重新读取');}
 clearSecrets(){for(const input of this.secretInputs)input.value='';}
 suspend(){this.epoch++;this.clearSecrets();this.busy=false;this.ready=false;this.sample=null;this.update();}
 reset(){
  this.epoch++;this.clearSecrets();this.secretInputs.clear();this.busy=false;this.ready=false;this.sample=null;this.view=null;this.pending=null;this.targets=[];this.rules=[];this.cards=[];this.dirty=false;this.cursor='0';this.more=false;
  this.$('forwarding-test-target').replaceChildren();this.$('forwarding-targets').replaceChildren();this.$('forwarding-rules').replaceChildren();this.$('forwarding-receipts').replaceChildren();this.$('forwarding-pending').textContent='';this.message('所有者登录后，点击“读取状态”。此区域不会自动轮询。');this.update();
 }
 message(text){this.$('forwarding-state').textContent=text;}
 update(){
  const blocked=this.busy||!this.ready||!!this.pending||this.sample?.quick_test_active===true;
  for(const card of this.cards)card.disabled=blocked;
  for(const id of ['forwarding-save','forwarding-toggle','forwarding-add-target','forwarding-add-rule'])this.$(id).disabled=blocked;
  this.$('forwarding-add-target').disabled=blocked||this.targets.length>=8;this.$('forwarding-add-rule').disabled=blocked||this.rules.length>=32;
  for(const id of ['forwarding-refresh','forwarding-receipts-refresh'])this.$(id).disabled=this.busy;
  this.$('forwarding-toggle').textContent=this.sample?.enabled?'关闭转发':'开启转发';
  this.$('forwarding-query').hidden=!this.pending;this.$('forwarding-query').disabled=this.busy||!this.pending;
  this.$('forwarding-archive').hidden=!this.pending;this.$('forwarding-archive').disabled=this.busy||!this.pending;
  this.$('forwarding-receipts-more').disabled=this.busy||!this.more;
  this.$('forwarding-test').disabled=blocked||!this.sample?.synthetic_test_api||this.sample?.enabled!==true||this.sample?.process_state_verified!==true||this.sample?.state!=='running'||this.sample?.delivery_api!==true||!this.view?.targets.length;
  this.$('forwarding-test-target').disabled=blocked;
  this.$('forwarding-pending').textContent=this.pending?'上次'+(this.pending.kind==='test'?'合成测试':'保存')+'待核对：'+this.pending.id+'。不会自动重新提交。':'';
 }
 async action(fn){
  if(this.busy)return;const g=this.epoch;this.busy=true;this.update();
  try{await fn(g);}catch(error){if(g===this.epoch)this.message(error instanceof FormError?error.message:'操作未获确认。请核对保存回执、登录与设备状态；不会自动重新提交。');}
  finally{if(g===this.epoch){this.busy=false;this.update();}}
 }
 status(s){requireValue(s&&s.available===true&&s.configuration_api===true&&generation(s.generation)&&typeof s.enabled==='boolean'&&typeof s.process_state_verified==='boolean');return s;}
 manageable(s){return s.service_management_api===true&&s.retained_credentials_api===true&&s.request_receipt_api===true;}
 async load(g){
  requireValue(!!this.getToken(),'请先使用所有者浏览器登录');
  this.ready=false;this.sample=null;this.view=null;
  const s=this.status(await this.request({op:'forwarding_status'}));this.current(g);
  const v=viewInput(await this.request({op:'forwarding_rules'}),s);this.current(g);
  this.sample=s;this.view=v;this.ready=this.manageable(s);
  this.loadView(v);return s;
 }
 async read(g){
  this.current(g);
  if(this.dirty&&!this.confirm('重新读取会清空未保存的目标和规则输入，继续？'))return;
  this.clearSecrets();this.ready=false;this.sample=null;
  const pending=pendingInput(await this.loadPending());this.current(g);this.pending=pending;
  const s=await this.load(g);this.message(processText(s)+(this.ready?'。'+privacyText(this.view.rules)+'；凭据不回显。':'。当前安装尚未启用完整转发服务管理。'));
 }
 target(t=null){
  const row={id:t?.id||this.crypto.randomUUID(),existing:!!t,kind:t?.kind||'webhook',keep:!!t,endpoint:'',bearer:'',key:'',chat:''};return row;
 }
 rule(r=null){return {...ruleRow(r),id:r?.id||this.crypto.randomUUID(),targets:r?.targets.slice()||this.targets.slice(0,1).map(t=>t.id)};}
 loadView(v){
  this.clearSecrets();this.secretInputs.clear();this.targets=v.targets.map(t=>this.target(t));this.rules=v.rules.map(r=>this.rule(r));
  if(!this.targets.length&&!this.rules.length){this.targets=[this.target()];this.rules=[this.rule()];}
  this.dirty=false;this.render();
  const select=this.$('forwarding-test-target');select.replaceChildren();
  for(const [i,t] of v.targets.entries()){const option=this.element('option',`目标 ${i+1} · ${kinds[t.kind]} · ${t.id.slice(0,8)}`);option.value=t.id;select.append(option);}
  select.value=v.targets[0]?.id||'';
 }
 element(tag,text){const e=this.doc.createElement(tag);if(text!==undefined)e.textContent=text;return e;}
 label(text,input){const label=this.element('label',text);label.append(input);return label;}
 field(row,name,label,{secret=false,max=256}={}){
  const input=secret&&row[name+'Input']||this.element('input');input.type=secret?'password':'text';input.autocomplete='off';input.maxLength=max;
  if(secret){this.secretInputs.add(input);input.oninput=()=>{this.dirty=true;};}else {input.value=row[name]??'';input.oninput=()=>{row[name]=input.value;this.dirty=true;};}
  if(secret)row[name+'Input']=input;
  return this.label(label,input);
 }
 capture(){
  for(const r of this.rules)if(r.select)r.targets=Array.from(r.select.options).filter(o=>o.selected).map(o=>o.value);
 }
 clearTarget(t){for(const name of ['endpoint','bearer','key','chat']){if(t[name+'Input'])t[name+'Input'].value='';delete t[name+'Input'];}}
 render(){
  this.capture();const oldSecrets=this.secretInputs;this.secretInputs=new Set();const targets=[],rules=[];
  for(const [i,t] of this.targets.entries()){
   const card=this.element('fieldset');card.className='forwarding-card';card.append(this.element('legend',`目标 ${i+1} · ${t.id.slice(0,8)}`));
   const kind=this.element('select');for(const [value,label] of Object.entries(kinds)){const option=this.element('option',label);option.value=value;option.selected=t.kind===value;kind.append(option);}kind.value=t.kind;kind.disabled=t.existing&&t.keep;
   kind.onchange=()=>{this.clearTarget(t);t.kind=kind.value;this.dirty=true;this.render();};card.append(this.label('发送器',kind));
   if(t.existing){const replace=this.element('input');replace.type='checkbox';replace.checked=!t.keep;replace.onchange=()=>{this.clearTarget(t);t.keep=!replace.checked;this.dirty=true;this.render();};card.append(this.label('替换此目标的地址和凭据（不勾选则保留原值）',replace));}
   if(!t.keep){
    if(t.kind!=='telegram')card.append(this.field(t,'endpoint',t.kind==='bark'?'Bark 服务地址（HTTPS 根地址）':'Webhook HTTPS 地址（不回显）',{secret:true,max:2048}));
    if(t.kind==='webhook')card.append(this.field(t,'bearer','Bearer（可选）',{secret:true,max:2048}));
    else card.append(this.field(t,'key',t.kind==='bark'?'Bark key':'Telegram bot token',{secret:true,max:256}));
    if(t.kind==='telegram')card.append(this.field(t,'chat','Telegram chat ID',{secret:true,max:128}));
   }
   const remove=this.element('button','移除此目标');remove.onclick=()=>{if(this.busy)return;this.capture();this.targets=this.targets.filter(x=>x!==t);this.rules.forEach(r=>{r.targets=r.targets.filter(id=>id!==t.id);r.select=null;});this.dirty=true;this.render();};card.append(remove);targets.push(card);
  }
  for(const [i,r] of this.rules.entries()){
   const card=this.element('fieldset');card.className='forwarding-card';card.append(this.element('legend',`规则 ${i+1}`));
   const select=this.element('select');select.multiple=true;select.size=Math.max(1,Math.min(4,this.targets.length));
   for(const [j,t] of this.targets.entries()){const option=this.element('option',`目标 ${j+1} · ${kinds[t.kind]} · ${t.id.slice(0,8)}`);option.value=t.id;option.selected=r.targets.includes(t.id);select.append(option);}r.select=select;select.onchange=()=>{r.targets=Array.from(select.options).filter(o=>o.selected).map(o=>o.value);this.dirty=true;};card.append(this.label('发送到（可多选）',select));
   card.append(this.field(r,'sender','发送方完全等于（留空不限）'),this.field(r,'contains','正文包含（留空不限）'),this.field(r,'line','SIM 代次（留空不限）',{max:16}));
   const share=this.element('select');share.ariaLabel='内容隐私';for(const [value,label] of [['notice','仅通知（默认，不发送正文）'],['masked_text','正文隐藏英数字（不发送原始发送方）'],['full_text','明确允许发送完整正文和发送方']]){const o=this.element('option',label);o.value=value;share.append(o);}share.value=r.full?'full_text':r.masked?'masked_text':'notice';share.onchange=()=>{r.full=share.value==='full_text';r.masked=share.value==='masked_text';this.dirty=true;};card.append(this.label('内容隐私',share));
   const advanced=this.element('details');advanced.append(this.element('summary','高级筛选 · 正则与时间段'));advanced.append(this.field(r,'regex','正文正则（留空不限，最多256字节）'));
   const window=this.element('input');window.type='checkbox';window.checked=r.window;window.onchange=()=>{r.window=window.checked;this.dirty=true;};advanced.append(this.label('按短信标注时间筛选；无有效时间的短信不匹配',window));
   advanced.append(this.field(r,'start','开始 HH:MM（含）',{max:5}),this.field(r,'end','结束 HH:MM（不含，可跨午夜）',{max:5}),this.field(r,'offset','固定 UTC 偏移分钟（中国480，无自动夏令时）',{max:4}));
   advanced.append(this.element('p','小型Rust正则不支持环视、反向引用或Unicode属性。隐藏英数字不识别所有敏感内容；敏感短信请用“仅通知”。'));card.append(advanced);
   const remove=this.element('button','移除此规则');remove.onclick=()=>{if(this.busy)return;this.capture();this.rules=this.rules.filter(x=>x!==r);this.dirty=true;this.render();};card.append(remove);rules.push(card);
  }
  for(const input of oldSecrets)if(!this.secretInputs.has(input))input.value='';
  this.cards=[...targets,...rules];this.$('forwarding-targets').replaceChildren(...targets);this.$('forwarding-rules').replaceChildren(...rules);this.update();
 }
 async archive(record,applied,g){
  await this.archivePending({...record,applied_generation:applied,uncertain:applied===null});this.current(g);
  requireValue(this.pending?.id===record.id,'保存记录已变化，请重新读取');
  await this.savePending(null,record.id);this.current(g);this.pending=null;
 }
 async mutate(g,kind,fields){
  this.current(g);requireValue(this.ready&&this.sample&&!this.pending,'请先读取状态并核对上次保存');
  requireValue(this.sample.generation<Number.MAX_SAFE_INTEGER,'配置代次超出网页可编辑范围，请使用维护入口');
  const record={format:1,id:this.crypto.randomUUID(),kind,expected_generation:this.sample.generation};
  this.clearSecrets();for(const t of this.targets)for(const name of ['endpoint','bearer','key','chat'])t[name]='';
  try{await this.savePending(record,null);}catch{throw new FormError('本机回执未能保存或另一页面已有保存，请读取状态；本次未提交到设备');}this.current(g);this.pending=record;this.update();
  const result=await this.request({...fields,expected_generation:record.expected_generation,request_id:record.id});this.current(g);
  requireValue(result.configuration_applied===true&&result.request_id===record.id&&result.generation===record.expected_generation+1,'保存回执不匹配，请核对上次保存');
  this.status(result);await this.archive(record,result.generation,g);
  this.ready=false;this.sample=result;this.view=null;const view=viewInput(await this.request({op:'forwarding_rules'}),result);this.current(g);this.view=view;this.ready=this.manageable(result);this.loadView(view);
  this.message('开关和规则意图已保存；'+processText(result)+'。');
 }
 async save(g){
  this.current(g);requireValue(this.ready&&!this.pending,'请先读取状态并核对上次保存');
  this.capture();const targets=this.targets.map(t=>({id:t.id,kind:t.kind,existing:t.existing,keep:t.keep,endpoint:t.endpointInput?.value||'',bearer:t.bearerInput?.value||'',key:t.keyInput?.value||'',chat:t.chatInput?.value||''}));const policy=policyInput(targets,this.rules,this.sample?.enabled===true,this.sample?.rules_revision??null);
  if(!this.confirm('保存这些目标和规则？'+(policy.enabled?'转发已开启，新规则将立即生效。':'保存后仍保持关闭。')+(policy.rules.some(r=>r.share==='full_text')?'已选择转发完整短信正文。':privacyText(policy.rules)+'。')))return;
  await this.mutate(g,'configure',{op:'forwarding_configure',policy});
 }
 async toggle(g){
  this.current(g);requireValue(this.ready&&!this.pending,'请先读取状态并核对上次保存');
  requireValue(this.sample&&this.view,'请先读取状态');const enabled=!this.sample.enabled;
  requireValue(!enabled||this.view.targets.length>0&&this.view.rules.length>0,'请先保存至少一个目标和一条规则');
  if(!this.confirm((enabled?'开启短信转发？默认只处理之后的新短信。':'关闭短信转发并停止转发进程？')+(enabled&&this.view.rules.some(r=>r.share==='full_text')?'现有规则会向目标发送完整短信正文。':'')+(this.dirty?'未保存的表单输入将清空，不会提交。':'')))return;
  await this.mutate(g,enabled?'enable':'disable',{op:'set_forwarding_enabled',enabled});
 }
 async query(g){
  this.current(g);
  requireValue(!!this.pending,'没有待核对的保存');const record=this.pending;
  if(testKind(record.kind)){await this.inspectTest(g,record,await this.request({op:'forwarding_test_request',request_id:record.id}));return;}
  const result=await this.request({op:'forwarding_request',request_id:record.id});this.current(g);
  requireValue(result.request_id===record.id&&typeof result.known==='boolean'&&result.not_found_is_not_proof===true&&generation(result.current_generation));
  if(!result.known){this.message('没有找到这笔保存回执；记录可能未受理、已清理或来自恢复前。不会重提，记录继续保留。');return;}
  requireValue(generation(result.applied_generation)&&result.applied_generation===record.expected_generation+1&&result.applied_generation<=result.current_generation);
  await this.archive(record,result.applied_generation,g);const sample=await this.load(g);this.message('已核对保存并归档回执；'+processText(sample)+'。');
 }
 async inspectTest(g,record,result){
  this.current(g);requireValue(result.request_id===record.id&&(result.accepted===true||typeof result.known==='boolean'));
  if(result.known===false){requireValue(result.not_found_is_not_proof===true);this.message('没有找到这笔合成测试回执；不代表从未外发，不会重新提交。');return true;}
  const r=result.receipt;requireValue(r&&r.id===record.id&&r.target_id===record.target_id&&r.generation===record.expected_generation&&r.synthetic===true&&Object.hasOwn(states,r.state));
  if(['confirmed','failed','cancelled'].includes(r.state)){
   await this.archive(record,r.generation,g);await this.load(g);this.message(r.state==='confirmed'?'合成测试已获目标确认；没有读取或发送真实短信。':'合成测试'+states[r.state]+'；不会自动重发。');return true;
  }
  if(r.state==='unknown'){this.message('合成测试结果不确定，回执已保留；不会自动重发。');return true;}
  this.message('合成测试'+states[r.state]+'，正在核对回执；只提交一次。');return false;
 }
 async test(g){
  this.current(g);requireValue(this.ready&&!this.pending&&this.sample?.synthetic_test_api&&this.sample.enabled&&this.sample.process_state_verified&&this.sample.state==='running'&&this.sample.delivery_api,'请先保存目标、开启转发并读取确认运行状态');
  const target=this.$('forwarding-test-target').value;requireValue(this.view.targets.some(t=>t.id===target),'请选择已保存的测试目标');
  if(!this.confirm('向所选已保存目标发送一条固定合成通知？不读取真实短信。'+(this.dirty?'未保存的输入会清空，不会提交。':'')))return;
  const record={format:1,id:this.crypto.randomUUID(),kind:'test',expected_generation:this.sample.generation,target_id:target};
  this.clearSecrets();
  try{await this.savePending(record,null);}catch{throw new FormError('本机回执未能保存或另一页面已有请求；本次未提交测试');}this.current(g);this.pending=record;this.update();
  const deadline=Date.now()+15000;
  const result=await this.request({op:'forwarding_test',expected_generation:record.expected_generation,request_id:record.id,target_id:target});
  if(await this.inspectTest(g,record,result))return;
  for(let i=0;i<10&&Date.now()<deadline;i++){
   await this.delay();this.current(g);if(Date.now()>=deadline)break;
   if(await this.inspectTest(g,record,await this.request({op:'forwarding_test_request',request_id:record.id})))return;
  }
  this.message('合成测试仍在处理，回执已保留；可点击核对上次请求，不会重新提交。');
 }
 async archiveUnknown(g){
  this.current(g);
  requireValue(!!this.pending,'没有待核对的保存');
  if(!this.confirm('将这笔未确认保存保留在本机回执中，再读取当前配置？这不会撤回、重发或证明旧请求未执行。'))return;
  await this.archive(this.pending,null,g);const s=await this.load(g);this.message('未确认记录已保留；'+processText(s)+'。后续改动使用当前配置代次。');
 }
 async receipts(g,reset){
  requireValue(!!this.getToken(),'请先使用所有者浏览器登录');
  const result=await this.request({op:'forwarding_receipts',after:reset?'0':this.cursor,limit:16});this.current(g);
  requireValue(Array.isArray(result.receipts)&&result.receipts.length<=16);const rows=[];let cursor=reset?'0':this.cursor;
  for(const r of result.receipts){requireValue(typeof r.seq==='string'&&/^(0|[1-9][0-9]{0,18})$/.test(r.seq)&&BigInt(r.seq)<=9223372036854775807n&&BigInt(r.seq)>BigInt(cursor)&&uuid(r.id)&&uuid(r.message_id)&&uuid(r.target_id)&&Object.hasOwn(states,r.state));cursor=r.seq;rows.push(this.element('p',`${r.synthetic===true?'合成测试':'短信'} ${r.message_id.slice(0,8)} → 目标 ${r.target_id.slice(0,8)} · ${states[r.state]}`));}
  if(reset)this.$('forwarding-receipts').replaceChildren();this.$('forwarding-receipts').append(...rows);this.cursor=cursor;this.more=result.receipts.length===16;this.message(rows.length?'已读取递送回执；不包含短信正文或目标凭据。':'暂时没有新的递送回执。');
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
    const r=await this.request({op:'forwarding_test_request',request_id:record.id});
    requireValue(r.request_id===record.id&&typeof r.known==='boolean','测试回执无效；不会重新发送。');
    requireValue(r.known===true,'没有找到这笔测试回执；不代表未外发，记录已保留，不会重新发送。');
    const receipt=r.receipt;
    requireValue(receipt&&receipt.id===record.id&&receipt.target_id===record.target_id&&receipt.generation===record.expected_generation&&receipt.synthetic===true&&Object.hasOwn(states,receipt.state)&&typeof r.quick_active==='boolean','测试回执不匹配；不会重新发送。');
    if(r.quick_active===false){
     let closed=r.automatic_stop_verified===true;
     if(!closed){const s=await this.request({op:'forwarding_status'});closed=s.quick_test_active===false&&s.enabled===false&&s.state==='disabled'&&s.process_state_verified===true;}
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
