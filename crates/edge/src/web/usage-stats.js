// SPDX-License-Identifier: AGPL-3.0-only
// Candidate projection of member A PR #41 QueryOutcome at 295c47655af1d69d2eb0568eda09023c4599c16b.
const windows={'24h':'最近24小时','30d':'最近30天','90d':'最近90天'};
const reasons=['offline','timeout','rejected','unavailable','clock_unavailable','clock_rollback','throttled','oversized','invalid_response','wrong_version','wrong_topic','wrong_window','invalid_page','inconsistent_pages','expired_response','invalid_time','count_overflow','count_out_of_range','invalid_cache'];
const stamp=n=>Number.isSafeInteger(n)&&n>=0&&n<=8640000000000&&Number.isFinite(new Date(n*1000).getTime())?new Date(n*1000).toLocaleString('zh-CN',{timeZone:'Asia/Shanghai'})+' UTC+8':'未知';
export function usageRows(value,expectedWindow=null){
 const unavailable=reason=>[['统计状态',reason],['近期活跃安装估计','未知'],['说明','这是安装活跃估计，不是人数、在线用户数或连接节点数']];
 if(value?.status==='disabled')return [['统计状态','已关闭'],['说明','不产生新的统计意图；已有公开订阅可能保留到链上到期，与转发、远程接入和蜂窝数据开关分别管理']];
 if(value?.status==='unknown')return unavailable(reasons.includes(value.reason)?`未知（${value.reason}）`:'未知');
 if(!['success','cached'].includes(value?.status))return unavailable('统计后端尚未接入或返回未知状态');
 const e=value.entry;if(!e||!Object.hasOwn(windows,e.window)||expectedWindow&&e.window!==expectedWindow||e.source!=='read_only_transport'||e.freshness_revoked!==false)return unavailable('来源、窗口或时间证据未确认');
 if(![e.observed_at_secs,e.succeeded_at_secs,e.expires_at_secs].every(n=>Number.isSafeInteger(n)&&n>=0)||e.observed_at_secs>e.succeeded_at_secs||e.expires_at_secs<=e.succeeded_at_secs||e.expires_at_secs-e.succeeded_at_secs>600)return unavailable('时间证据无效');
 const cached=value.status==='cached',freshness={within_ttl:'缓存仍在原期限内',expired:'缓存已过期',clock_rollback:'时钟回拨，缓存不能当作新鲜数据'};
 if(cached&&!Object.hasOwn(freshness,value.freshness))return unavailable('缓存新鲜度未知');
 const count=Number.isSafeInteger(e.count)&&e.count>=0?String(e.count):'未知（原始 u64 超出前端精确数值范围或格式无效）';
 return [['统计状态',cached?`缓存估计：${freshness[value.freshness]}`:'原统计模块返回查询成功'],['统计窗口',windows[e.window]],['近期活跃安装估计',count],['原始观察时间',stamp(e.observed_at_secs)],['上次成功读取',stamp(e.succeeded_at_secs)],['缓存到期',stamp(e.expires_at_secs)],['说明','仅统计独立授权的近期活跃安装估计；不是人数、在线用户数或连接节点数。仅在单独授权的引擎已运行时查询汇总计数；不参与安装上报，不续订、不启用转发或蜂窝数据。']];
}
export class UsageStatsPanel {
 constructor({document,root,read=null,clock=()=>performance.now()}){this.doc=document;this.read=read;this.clock=clock;this.epoch=0;this.lastRead=-Infinity;this.busy=false;this.select=document.createElement('select');this.select.setAttribute('aria-label','统计窗口');for(const [value,label] of Object.entries(windows)){const o=document.createElement('option');o.value=value;o.textContent=label;this.select.append(o);}this.select.value='24h';this.button=document.createElement('button');this.button.type='button';this.button.textContent='只读刷新统计';this.button.onclick=()=>this.refresh();this.state=document.createElement('p');this.state.setAttribute('role','status');this.rows=document.createElement('dl');this.rows.style.overflowWrap='anywhere';root.replaceChildren(this.select,this.button,this.state,this.rows);this.reset();}
 display(rows){this.rows.replaceChildren();for(const [label,value] of rows){const dt=this.doc.createElement('dt'),dd=this.doc.createElement('dd');dt.textContent=label;dd.textContent=value;this.rows.append(dt,dd);}}
 reset(){this.epoch++;this.controller?.abort();this.busy=false;this.button.disabled=typeof this.read!=='function';this.display(usageRows(null));this.state.textContent=typeof this.read==='function'?'仅点击时读取已授权的统计结果':'统计读取端口尚未安装';}
 async refresh(){
  if(this.busy||typeof this.read!=='function')return;const started=this.clock();
  if(!Number.isFinite(started)||started-this.lastRead<5000){this.state.textContent='请至少间隔5秒后再手动读取；没有自动请求';return;}
  const window=this.select.value;if(!Object.hasOwn(windows,window))return;this.lastRead=started;
  const epoch=this.epoch;this.busy=true;this.button.disabled=true;this.controller=new AbortController();const controller=this.controller;let timer;
  try{
   const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(Error('deadline'));controller.abort();},10000);});
   const value=await Promise.race([this.read({window,signal:controller.signal}),timeout]);if(epoch!==this.epoch)return;
   const elapsed=this.clock()-started;if(!Number.isFinite(elapsed)||elapsed<0||elapsed>=10000){controller.abort();throw Error('deadline');}
   this.display(usageRows(value,window));this.state.textContent=value?.status==='disabled'?'统计查询已关闭；可在 NKN 区单独请求启用。参与安装上报尚未开放。':'本次只读查询已结束；没有上报、续订或自动重试。参与安装上报尚未开放。';
  }catch{if(epoch===this.epoch){this.display(usageRows(null));this.state.textContent='读取未确认，当前计数未知；没有自动重试';}}
  finally{clearTimeout(timer);if(epoch===this.epoch){this.busy=false;this.button.disabled=false;}}
 }
}

export function bindUsageCounts(panel, read) {
 if (!panel) return;
 const button = panel.querySelector('[data-refresh]');
 const status = panel.querySelector('[data-status]');
 const cells = panel.querySelectorAll('[data-count]');
 let lastSuccess;
 button.addEventListener('click', async () => {
  if (button.disabled) return;
  button.disabled = true;
  status.textContent = '正在查询 NKN';
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10500);
  try {
   const response = await read(controller.signal);
   if (!response.ok) throw Error('count unavailable');
   const {source, counts} = await response.json();
   if (!['domestic', 'official'].includes(source) || !Array.isArray(counts)
       || counts.length !== cells.length || !counts.every(n => Number.isSafeInteger(n) && n >= 0)) {
    throw Error('invalid counts');
   }
   counts.forEach((value, index) => { cells[index].textContent = value.toLocaleString('zh-CN'); });
   const label = source === 'domestic' ? '国内节点' : '官方主网兜底';
   const time = new Date().toLocaleTimeString('zh-CN', {timeZone: 'Asia/Shanghai'});
   lastSuccess = label + ' · ' + time;
   status.textContent = lastSuccess;
  } catch {
   status.textContent = lastSuccess ? '刷新失败 · 上次 ' + lastSuccess : '暂未取到统计';
  } finally {
   clearTimeout(timer);
   button.disabled = false;
  }
 });
}

const permissionExact=(v,keys)=>v&&typeof v==='object'&&!Array.isArray(v)&&Object.keys(v).length===keys.length&&keys.every(k=>Object.hasOwn(v,k));
const permissionContext=s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s);
const permissionGeneration=s=>typeof s==='string'&&/^(0|[1-9][0-9]{0,18})$/.test(s)&&BigInt(s)<9223372036854775807n;
const permissionId=s=>typeof s==='string'&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(s);
const permissionRequire=v=>{if(!v)throw Error('授权记录未确认');};
export function participationPending(v){
 if(v==null)return null;
 permissionRequire(permissionExact(v,['format','id','enabled','disclosure_version','expected_generation','core_instance'])&&v.format===1&&permissionId(v.id)&&typeof v.enabled==='boolean'&&v.disclosure_version===(v.enabled?1:null)&&permissionGeneration(v.expected_generation)&&permissionContext(v.core_instance));
 return {...v};
}
export function participationStatus(v){
 permissionRequire(permissionExact(v,['format','available','core_instance','generation','enabled','disclosure_version','disabled_generation','owner_current','record_exists','reporting_available','runtime'])&&v.format===1&&v.available===true&&permissionContext(v.core_instance)&&permissionGeneration(v.generation)&&permissionGeneration(v.disabled_generation)&&BigInt(v.disabled_generation)<=BigInt(v.generation)&&typeof v.enabled==='boolean'&&v.disclosure_version===(v.enabled?1:null)&&typeof v.owner_current==='boolean'&&typeof v.record_exists==='boolean'&&v.record_exists===(v.generation!=='0')&&typeof v.reporting_available==='boolean');
 permissionRequire(!v.reporting_available||v.enabled&&v.owner_current);
 permissionRequire(v.runtime===(!v.enabled?'disabled':v.owner_current?(v.reporting_available?'ready_for_check_in':'authorized_not_connected'):'reauthorization_required'));
 return {...v};
}
export function participationReceipt(v,record){
 permissionRequire(v?.known===true&&v.state==='saved'&&v.request_id===record.id&&v.enabled===record.enabled&&v.disclosure_version===record.disclosure_version&&v.expected_generation===record.expected_generation&&v.core_instance===record.core_instance&&permissionContext(v.current_core_instance)&&permissionGeneration(v.generation)&&BigInt(v.generation)===BigInt(record.expected_generation)+1n&&v.not_found_is_not_proof===true&&v.reporting_available===false);
 return {state:'saved',generation:v.generation};
}
export function participationArchive(v){
 permissionRequire(permissionExact(v,['record','state','generation']));const record=participationPending(v.record);permissionRequire(record);
 permissionRequire(v.state==='unknown'&&v.generation===null||v.state==='saved'&&permissionGeneration(v.generation)&&BigInt(v.generation)===BigInt(record.expected_generation)+1n);
 return {record,state:v.state,generation:v.generation};
}

// Saves permission only. Querying counts, signing, submitting and renewing are
// separate ports; none of them is called or inferred from this component.
export class UsageParticipationPanel {
 constructor({document,root,request,getToken,login=null,confirm,loadPending,savePending,archivePending,crypto=globalThis.crypto}){
  Object.assign(this,{doc:document,request,getToken,login,confirm,loadPending,savePending,archivePending,crypto});this.epoch=0;
  const heading=document.createElement('h3');heading.textContent='是否参与安装统计';
  const label=document.createElement('label');this.consent=document.createElement('input');this.consent.type='checkbox';this.consent.setAttribute('aria-label','允许本设备参与活跃安装统计');
  const description=document.createElement('span');description.textContent='允许使用独立统计身份发布本产品的活跃安装订阅。不包含号码、短信或通话；公开订阅可能在关闭后保留到链上到期。';label.append(this.consent,description);
  this.state=document.createElement('p');this.state.setAttribute('role','status');this.output=document.createElement('p');this.output.style.whiteSpace='pre-wrap';this.output.style.overflowWrap='anywhere';this.buttons={};
  const button=(key,text,fn)=>{const b=document.createElement('button');b.type='button';b.textContent=text;b.onclick=fn;this.buttons[key]=b;return b;};
  root.replaceChildren(heading,label,button('read','登录并读取上报授权',()=>this.loginRead()),button('save','保存参与授权',()=>this.save()),button('checkIn','签到或核对',()=>this.action(g=>this.checkIn(g))),button('query','核对上次授权保存',()=>this.action(g=>this.query(g))),button('archive','保留不确定记录并解除阻止',()=>this.action(g=>this.archive(g))),this.state,this.output);this.reset();
 }
 reset(){this.epoch++;this.busy=false;this.starting=false;this.pending=null;this.sample=null;this.consent.checked=false;this.state.textContent='尚未读取设备授权；新设备默认不参与。保存授权后可查看本机签到状态。';this.output.textContent='当前授权未知；退出不会修改设备已保存的授权。读取不会开启引擎或上报。';this.update();}
 update(){const working=this.busy||this.starting,blocked=working||!this.getToken();this.consent.disabled=working;this.buttons.read.disabled=working;this.buttons.save.disabled=working||(!this.getToken()&&!this.login);this.buttons.checkIn.hidden=!this.sample?.reporting_available;this.buttons.checkIn.disabled=blocked||!!this.pending;this.buttons.query.disabled=blocked||!this.pending;this.buttons.archive.disabled=blocked||!this.pending;}
 current(g){permissionRequire(g===this.epoch&&!!this.getToken());}
 async action(fn){if(this.busy)return;const g=this.epoch;this.busy=true;this.update();try{await fn(g);}catch{if(g===this.epoch)this.state.textContent=this.pending?'授权保存结果未确认，原记录保留；请核对，不会自动重发。':'授权或读取未确认；没有上报、启停引擎或自动重试。';}finally{if(g===this.epoch){this.busy=false;this.update();}}}
 async loginRead(){if(this.busy||this.starting)return;const epoch=this.epoch;this.starting=true;this.update();try{if(this.login)await this.login();if(epoch!==this.epoch)return;this.starting=false;await this.action(g=>this.read(g));}catch{if(epoch===this.epoch)this.state.textContent='登录未确认，原记录保留。';}finally{if(epoch===this.epoch){this.starting=false;this.update();}}}
 async read(g){
  this.current(g);this.sample=null;this.pending=participationPending(await this.loadPending());this.current(g);
  const value=participationStatus(await this.request({op:'usage_participation_status'}));this.current(g);this.sample=value;this.consent.checked=value.enabled;
  this.output.textContent=[value.enabled?(value.owner_current?(value.reporting_available?'已保存参与授权，可以签到或核对上一笔。':'已保存参与授权，设备签到服务尚未就绪。'):'旧所有者授权不能作为当前授权，请重新确认。'):'当前不参与安装上报。','授权代次：'+value.generation,'保存授权和读取状态不会发送交易；每次签到需手动确认。','查询开关、远程功能和短信转发分别管理。'].join('\n');
  this.state.textContent=this.pending?'有一笔上次授权保存待核对：'+this.pending.id:'已读取设备上的授权记录；未上报。';this.update();
 }
 async save(){
  if(this.busy||this.starting)return;const enabled=this.consent.checked===true,epoch=this.epoch;this.starting=true;this.update();
  try{if(this.login)await this.login();}catch{if(epoch===this.epoch){this.starting=false;this.state.textContent='登录未确认，没有提交授权。';this.update();}return;}
  if(epoch!==this.epoch)return;this.starting=false;
  await this.action(async g=>{
   await this.read(g);this.current(g);permissionRequire(!this.pending);
   if(!this.confirm(enabled?'保存参与活跃安装统计的授权？未来的独立统计公钥、产品主题和到期窗口会公开可见，可能产生链上费用；关闭不会立即抹除已公开订阅。本次只保存授权。':'关闭参与安装上报的授权？已公开订阅可能保留到链上到期；查询、远程和短信转发保持独立。'))return;
   this.current(g);const record=participationPending({format:1,id:this.crypto.randomUUID(),enabled,disclosure_version:enabled?1:null,expected_generation:this.sample.generation,core_instance:this.sample.core_instance});
   await this.savePending(record,null);this.current(g);permissionRequire(JSON.stringify(participationPending(await this.loadPending()))===JSON.stringify(record));this.current(g);this.pending=record;this.update();
   const value=await this.request({op:'usage_participation_set',enabled,disclosure_version:record.disclosure_version,expected_generation:record.expected_generation,expected_core_instance:record.core_instance,request_id:record.id});this.current(g);await this.accept(g,record,value);
  });
 }
 async accept(g,record,value){
  const receipt=participationReceipt(value,record);await this.archivePending({record,...receipt});this.current(g);
  await this.savePending(null,record.id);this.current(g);permissionRequire(participationPending(await this.loadPending())===null);this.pending=null;await this.read(g);
  this.state.textContent=this.sample.generation!==receipt.generation||this.sample.enabled!==record.enabled||this.sample.enabled&&!this.sample.owner_current?'原请求已保存；当前授权已有后续变更，请以最新读取为准。没有重新应用旧请求。':record.enabled?'参与授权已保存，未开始上报。':'不参与的授权已保存；没有启动上报任务。';
 }
 async checkIn(g){
   this.current(g);
   permissionRequire(this.sample?.reporting_available&&!this.pending);
   if(!this.confirm('进行一次安装统计签到或核对上一笔？若有未确认的交易，只会核对原交易。统计公钥、主题及订阅期限在链上公开。'))return;
   this.current(g);
   try {
    const value=await this.request({op:'usage_check_in'});this.current(g);
    if(value?.state==='all_confirmed'){this.state.textContent='当前周期的三个窗口均已确认。';return;}
    const label=windows[value?.window];
    const messages={pending:'已提交，等待链上确认',unknown:'待核对原交易，暂不能提交下一笔',confirmed:'已核对链上确认',due:'当前还未到续订时间',unavailable:'签到暂不可用'};
    permissionRequire(!!label&&Object.hasOwn(messages,value?.state));
    this.state.textContent=label+' · '+messages[value.state];
   }catch{
    if(g===this.epoch)this.state.textContent='本次结果未确认；请重新登录后核对。';
   }
  }
 async query(g){
  this.current(g);const record=participationPending(await this.loadPending());this.current(g);permissionRequire(record);this.pending=record;
  const value=await this.request({op:'usage_participation_request',request_id:record.id});this.current(g);
  permissionRequire(value?.request_id===record.id&&typeof value.known==='boolean'&&value.not_found_is_not_proof===true);
  if(!value.known){this.state.textContent='未找到原保存记录不证明未执行；记录保留，不会重发。';return;}
  await this.accept(g,record,value);
 }
 async archive(g){
  this.current(g);const record=participationPending(await this.loadPending());this.current(g);permissionRequire(record);
  if(!this.confirm('保留原不确定回执并解除本页阻止？这不会撤销或再次提交设备上的授权，不能证明保存失败。'))return;
  this.current(g);await this.archivePending({record,state:'unknown',generation:null});this.current(g);await this.savePending(null,record.id);this.current(g);permissionRequire(participationPending(await this.loadPending())===null);this.pending=null;await this.read(g);
 }
}
