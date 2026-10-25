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
export function policyInput(targets,rules,enabled){
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
  return {id:r.id,targets:r.targets.slice(),sender_equals:r.sender||null,body_contains:r.contains||null,line,share:r.full?'full_text':'notice'};
 });
 requireValue(!enabled||selected.length>0&&matched.length>0,'请先保存至少一个目标和一条规则');
 return {enabled:enabled===true,targets:selected,rules:matched};
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
 for(const r of v.rules){requireValue(uuid(r.id)&&!ruleIds.has(r.id)&&Array.isArray(r.targets)&&r.targets.length>0&&r.targets.length<=8&&r.targets.every(id=>ids.has(id))&&new Set(r.targets).size===r.targets.length&&text(r.sender_equals??'',256)&&text(r.body_contains??'',256));ruleIds.add(r.id);}
 // Reuse production input limits without requesting any retained secret.
 policyInput(v.targets.map(t=>({id:t.id,kind:t.kind,existing:true,keep:true})),v.rules.map(r=>({id:r.id,targets:r.targets,sender:r.sender_equals||'',contains:r.body_contains||'',line:r.line??'',full:r.share==='full_text'})),v.enabled);
 for(const r of v.rules)requireValue(['notice','full_text'].includes(r.share));
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
  const s=await this.load(g);this.message(processText(s)+(this.ready?(this.view.rules.some(r=>r.share==='full_text')?'。现有规则包含完整正文转发；凭据不回显。':'。现有规则仅发送新短信通知；凭据不回显。'):'。当前安装尚未启用完整转发服务管理。'));
 }
 target(t=null){
  const row={id:t?.id||this.crypto.randomUUID(),existing:!!t,kind:t?.kind||'webhook',keep:!!t,endpoint:'',bearer:'',key:'',chat:''};return row;
 }
 rule(r=null){return {id:r?.id||this.crypto.randomUUID(),targets:r?.targets.slice()||this.targets.slice(0,1).map(t=>t.id),sender:r?.sender_equals||'',contains:r?.body_contains||'',line:r?.line??'',full:r?.share==='full_text'};}
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
   const full=this.element('input');full.type='checkbox';full.checked=r.full;full.onchange=()=>{r.full=full.checked;this.dirty=true;};card.append(this.label('明确允许向目标发送短信发送方和完整正文',full));
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
  this.capture();const targets=this.targets.map(t=>({id:t.id,kind:t.kind,existing:t.existing,keep:t.keep,endpoint:t.endpointInput?.value||'',bearer:t.bearerInput?.value||'',key:t.keyInput?.value||'',chat:t.chatInput?.value||''}));const policy=policyInput(targets,this.rules,this.sample?.enabled===true);
  if(!this.confirm('保存这些目标和规则？'+(policy.enabled?'转发已开启，新规则将立即生效。':'保存后仍保持关闭。')+(policy.rules.some(r=>r.share==='full_text')?'已选择转发完整短信正文。':'仅发送新短信通知。')))return;
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
 async run(){
  if(this.busy)return;this.busy=true;
  try{
   this.notify('正在自动登录并检查 Bark 目标…');await this.login();
   let record=pendingInput(await this.loadPending());
   if(record){requireValue(record.kind==='quick_test','有旧转发操作待核对；本次没有发送通知。');this.notify('正在核对上一次测试，不会重新发送…');}
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
      this.notify(receipt.state==='confirmed'?'测试完成：Bark 已确认，转发已自动关闭。请看手机是否收到“RustSimHub 合成测试”。':`测试${states[receipt.state]}；转发已自动关闭，不会自动重发。`);return;
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
