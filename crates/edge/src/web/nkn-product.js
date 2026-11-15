// SPDX-License-Identifier: AGPL-3.0-only
import {parseNknStatus,nknStatusText} from './nkn-status.js';
const uuid=s=>typeof s==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(s);
const context=s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s);
const revision=s=>typeof s==='string'&&/^(0|[1-9][0-9]{0,18})$/.test(s)&&BigInt(s)<9223372036854775807n;
const owners=['manual_engine','remote','statistics'];
const requireValue=(v,message='状态或回执未确认，请保留记录并重新核对')=>{if(!v)throw Error(message);};
const exact=(v,keys)=>v&&typeof v==='object'&&!Array.isArray(v)&&Object.keys(v).length===keys.length&&Object.keys(v).every(k=>keys.includes(k));
export function pendingInput(v){
 if(v===null||v===undefined)return null;
 requireValue(exact(v,['format','id','owner','enabled','expected_revision','core_instance'])&&v.format===1&&uuid(v.id)&&owners.includes(v.owner)&&typeof v.enabled==='boolean'&&revision(v.expected_revision)&&context(v.core_instance),'本机引擎回执损坏，请保留记录并通过维护入口核对');
 return {format:1,id:v.id,owner:v.owner,enabled:v.enabled,expected_revision:v.expected_revision,core_instance:v.core_instance};
}
export function reportInput(v){
 requireValue(exact(v,['available','configuration_api','revision','provider','remote_control_supported'])&&v.available===true&&v.configuration_api===true&&revision(v.revision)&&typeof v.remote_control_supported==='boolean');
 const p=v.provider;
 requireValue(exact(p,['contract','core_instance','owners','supervisor_state','root_generation','original_process_exit_verified','status'])&&p.contract===1&&context(p.core_instance)&&exact(p.owners,['manual','remote','statistics'])&&Object.values(p.owners).every(x=>typeof x==='boolean')&&['off','starting','ready','waiting_retry','stopping','unknown'].includes(p.supervisor_state)&&(p.root_generation===null||Number.isInteger(p.root_generation)&&p.root_generation>0&&p.root_generation<=0xffffffff)&&typeof p.original_process_exit_verified==='boolean');
 const status=parseNknStatus(p.status);requireValue(status.source!=='mock');
 if(status.source==='unavailable')requireValue(status.age_ms===null&&['enabled','service','path','dht'].every(k=>status[k]==='unknown')&&['rx_bytes','tx_bytes','rtt_ms','peer_count'].every(k=>status[k]===null));
 else requireValue(status.generation>0&&status.sequence>0&&status.age_ms!==null&&status.age_ms<=5000);
 requireValue(!p.original_process_exit_verified||p.supervisor_state==='off'&&!Object.values(p.owners).some(Boolean));
 return {...v,provider:{...p,owners:{...p.owners},status}};
}
export function receiptInput(v,record){
 requireValue(v&&v.request_id===record.id&&v.known===true&&['accepted','unknown'].includes(v.state)&&v.core_instance===record.core_instance&&v.owner===record.owner&&v.enabled===record.enabled&&v.expected_revision===record.expected_revision&&revision(v.admitted_revision)&&BigInt(v.admitted_revision)===BigInt(record.expected_revision)+1n&&v.not_found_is_not_proof===true);
 return {state:v.state,admitted_revision:v.admitted_revision};
}
export function archiveInput(v){
 requireValue(exact(v,['record','state','admitted_revision']));const record=pendingInput(v.record);requireValue(record);
 requireValue(v.state==='unknown'&&v.admitted_revision===null||v.state==='accepted'&&revision(v.admitted_revision)&&BigInt(v.admitted_revision)===BigInt(record.expected_revision)+1n);
 return {record,state:v.state,admitted_revision:v.admitted_revision};
}
const phase={off:'没有本轮运行请求',starting:'正在启动',ready:'已收到就绪快照',waiting_retry:'等待有限重连',stopping:'正在关闭',unknown:'运行结果未确认'};
export class NknProductPanel {
 constructor({document,root,request,getToken,login=null,confirm,loadPending,savePending,archivePending,crypto=globalThis.crypto,clock=()=>performance.now(),wallClock=()=>Date.now()}){
  Object.assign(this,{doc:document,request,getToken,login,confirm,loadPending,savePending,archivePending,crypto,clock,wallClock});this.epoch=0;
  this.state=document.createElement('p');this.state.setAttribute('role','status');this.state.style.whiteSpace='pre-wrap';this.output=document.createElement('p');this.output.style.whiteSpace='pre-wrap';
  this.buttons={};const action=(key,text,fn)=>{const b=document.createElement('button');b.type='button';b.textContent=text;b.onclick=()=>this.action(fn);this.buttons[key]=b;return b;};
  root.replaceChildren(this.state,action('read','只读核对引擎状态',g=>this.read(g)),action('manual_on','请求运行引擎',g=>this.set(g,'manual_engine',true)),action('manual_off','释放手动运行请求',g=>this.set(g,'manual_engine',false)),action('remote_on','请求开启远程功能',g=>this.set(g,'remote',true)),action('remote_off','关闭远程功能请求',g=>this.set(g,'remote',false)),action('statistics_on','启用统计只读查询',g=>this.set(g,'statistics',true)),action('statistics_off','关闭统计查询请求',g=>this.set(g,'statistics',false)),action('query','核对上次回执',g=>this.query(g)),action('archive','保留不确定记录并解除本页阻止',g=>this.archive(g)),this.output);
  if(login){this.loginButton=document.createElement('button');this.loginButton.type='button';this.loginButton.textContent='管理登录后读取';this.loginButton.onclick=async()=>{if(this.busy)return;try{await login();await this.action(g=>this.read(g));}catch{this.state.textContent='登录或读取未确认；原回执保留。';}};root.append(this.loginButton);}
  this.reset();
 }
 reset(){this.epoch++;clearTimeout(this.timer);this.busy=false;this.sample=null;this.order=null;this.pending=null;this.state.textContent='默认不自动启停。请先使用所有者登录，再手动读取状态。';this.output.textContent='引擎状态未知；关闭运行请求不表示进程已经退出。';this.update();}
 current(g){requireValue(g===this.epoch&&!!this.getToken(),'会话已变化，请重新读取；原操作回执保留');}
 age(){if(!this.sample)return NaN;const a=this.clock()-this.sample.received,b=this.wallClock()-this.sample.wallReceived;return [a,b].every(n=>Number.isFinite(n)&&n>=0)?this.sample.elapsed+Math.max(a,b):NaN;}
 fresh(){const age=this.age();return Number.isFinite(age)&&age<=5000;}
 update(){
  const blocked=this.busy||!this.getToken(),ready=!blocked&&!this.pending&&this.fresh(),p=this.sample?.data.provider;
  this.buttons.read.disabled=blocked;this.buttons.query.disabled=blocked||!this.pending;this.buttons.archive.disabled=blocked||!this.pending;
  this.buttons.manual_on.disabled=!ready||p.owners.manual||p.supervisor_state==='unknown';this.buttons.manual_off.disabled=!ready||!p.owners.manual;
  this.buttons.remote_on.disabled=!ready||!this.sample.data.remote_control_supported||p.owners.remote||p.supervisor_state==='unknown';this.buttons.remote_off.disabled=!ready||!p.owners.remote;
  this.buttons.statistics_on.disabled=!ready||p.owners.statistics||p.supervisor_state==='unknown';this.buttons.statistics_off.disabled=!ready||!p.owners.statistics;
  if(this.loginButton)this.loginButton.disabled=this.busy;
 }
 render(){
  const p=this.sample?.data.provider;if(!p){this.update();return;}
  const fresh=this.fresh(),age=this.age();
  const ownership=fresh?`手动运行：${p.owners.manual?'持有':'未持有'}；远程功能：${p.owners.remote?'持有':'未持有'}；统计使用：${p.owners.statistics?'持有':'未持有'}`:'快照已陈旧，请手动重新读取。';
  this.output.textContent=[fresh?phase[p.supervisor_state]:'引擎管理状态未知',ownership,fresh&&p.original_process_exit_verified?'原受管实例退出已确认':'原受管实例退出尚未确认',this.sample.data.remote_control_supported?'远程入口可请求启用':'此候选入口尚未提供完整远程连接装配',nknStatusText(p.status,p.status.age_ms===null?NaN:p.status.age_ms+age),'手动、远程和统计共享引擎；释放一项不会停止另外两项的运行请求。读取不会续期授权。'].join('\n');this.update();
 }
 async action(fn){if(this.busy)return;const g=this.epoch;this.busy=true;this.update();try{await fn(g);}catch{if(g===this.epoch)this.state.textContent='操作或读取未获确认，原回执保留；不会自动再次提交。';}finally{if(g===this.epoch){this.busy=false;this.render();this.update();}}}
 async read(g){
  this.current(g);clearTimeout(this.timer);this.sample=null;this.output.textContent='引擎状态未知；正在只读核对。';this.update();this.pending=pendingInput(await this.loadPending());this.current(g);
  const start=this.clock(),wallStart=this.wallClock();const data=reportInput(await this.request({op:'nkn_status'}));this.current(g);
  const received=this.clock(),wallReceived=this.wallClock(),a=received-start,b=wallReceived-wallStart;
  requireValue([a,b].every(n=>Number.isFinite(n)&&n>=0&&n<3000),'读取超时或时钟变化，状态保持未知');
  const s=data.provider.status,old=this.order;
  if(old){requireValue(old.instance===data.provider.core_instance,'运行上下文已变化，请重新登录核对');if(s.source==='engine')requireValue(s.generation>old.generation||s.generation===old.generation&&s.sequence>old.sequence);}
  if(s.source==='engine')this.order={instance:data.provider.core_instance,generation:s.generation,sequence:s.sequence};
  this.sample={data,received,wallReceived,elapsed:Math.max(a,b)};clearTimeout(this.timer);this.timer=setTimeout(()=>this.render(),5001-this.sample.elapsed);
  this.state.textContent=this.pending?'上次请求待核对：'+this.pending.id+'。不会自动重做。':'已只读核对；运行请求和实际进程状态分别显示。';this.render();
 }
 async set(g,owner,enabled){
  this.current(g);requireValue(this.fresh()&&!this.pending,'请先读取状态并核对上次回执');
  requireValue(owner==='statistics'||owner==='manual_engine'||owner==='remote'&&this.sample.data.remote_control_supported||owner==='remote'&&!enabled,'此功能入口尚不可用');
  if(!this.confirm(enabled?(owner==='statistics'?'允许统计只读查询使用共享 NKN 引擎及已配置的 Wi-Fi/USB 网络？授权有时限；不会参与安装上报、生成统计身份或发送链上订阅。':'请求启用此项引擎使用？授权有效期有限，不会自动续期。'):'释放此项运行请求？其他功能仍可能使用共享引擎。'))return;this.current(g);
  const record=pendingInput({format:1,id:this.crypto.randomUUID(),owner,enabled,expected_revision:this.sample.data.revision,core_instance:this.sample.data.provider.core_instance});
  await this.savePending(record,null);this.current(g);
  requireValue(JSON.stringify(pendingInput(await this.loadPending()))===JSON.stringify(record),'本机回执未能读回；没有向设备提交');this.current(g);this.pending=record;this.update();
  const result=await this.request({op:'set_nkn_enabled',request_id:record.id,owner,enabled,expected_revision:record.expected_revision,expected_core_instance:record.core_instance});this.current(g);
  await this.accept(g,record,result);
 }
 async accept(g,record,value){
  const receipt=receiptInput(value,record);
  if(receipt.state==='unknown'){this.state.textContent='运行请求结果不确定；回执保留，不会自动重发。';return;}
  await this.archivePending({record,state:'accepted',admitted_revision:receipt.admitted_revision});this.current(g);
  await this.savePending(null,record.id);this.current(g);requireValue(pendingInput(await this.loadPending())===null,'回执清理尚未确认；请重新读取');this.pending=null;
  await this.read(g);this.state.textContent='此项运行请求已核对；实际进程是否退出请看状态证据。';
 }
 async query(g){
  this.current(g);const record=pendingInput(await this.loadPending());this.current(g);requireValue(record,'没有待核对回执');this.pending=record;
  const result=await this.request({op:'nkn_request',request_id:record.id,expected_core_instance:record.core_instance});this.current(g);
  requireValue(result?.request_id===record.id&&typeof result.known==='boolean'&&result.not_found_is_not_proof===true);
  if(!result.known){this.state.textContent='未找到原请求不证明未执行；原记录保留，不会重发。';return;}
  await this.accept(g,record,result);
 }
 async archive(g){
  this.current(g);const record=pendingInput(await this.loadPending());this.current(g);requireValue(record);
  if(!this.confirm('保留本次不确定记录并解除本页阻止？这不撤销设备操作，也不证明进程已停止。'))return;
  await this.archivePending({record,state:'unknown',admitted_revision:null});this.current(g);await this.savePending(null,record.id);this.current(g);requireValue(pendingInput(await this.loadPending())===null);this.pending=null;await this.read(g);
 }
}
