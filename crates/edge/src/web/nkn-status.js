// SPDX-License-Identifier: AGPL-3.0-only
// Read-only T5 NknStatusProviderV1 projection. The host owns authentication,
// engine lifecycle and trusted context changes; this module never starts either.
export const NKN_FIELDS=['contract','source','generation','sequence','age_ms','enabled','service','path','dht','rx_bytes','tx_bytes','rtt_ms','peer_count','last_error'];
export const NKN_ENUMS={source:['engine','mock','unavailable'],enabled:['on','off','unknown'],service:['stopped','starting','running','stopping','failed','unknown'],path:['direct','relay','offline','unknown'],dht:['connected','disconnected','unknown'],last_error:[null,'engine_unavailable','deadline','protocol_error','stale','permission_denied']};
Object.freeze(NKN_FIELDS);for(const values of Object.values(NKN_ENUMS))Object.freeze(values);Object.freeze(NKN_ENUMS);
const error=code=>Object.assign(new Error('NKN status unconfirmed'),{code});
const uint=(v,max)=>Number.isSafeInteger(v)&&v>=0&&v<=max;
const u64=v=>v===null||typeof v==='string'&&/^(0|[1-9][0-9]{0,19})$/.test(v)&&BigInt(v)<=18446744073709551615n;
export function parseNknStatus(value){
 try{
  if(!value||typeof value!=='object'||Array.isArray(value))throw error('protocol_error');
  const keys=Object.keys(value);if(keys.length!==NKN_FIELDS.length||keys.some(k=>!NKN_FIELDS.includes(k)))throw error('protocol_error');
  const v=Object.fromEntries(NKN_FIELDS.map(k=>[k,value[k]]));
  if(v.contract!==1||Object.entries(NKN_ENUMS).some(([k,allowed])=>!allowed.includes(v[k]))||!uint(v.generation,0xffffffff)||!uint(v.sequence,0xffffffff)||!(v.age_ms===null||uint(v.age_ms,Number.MAX_SAFE_INTEGER))||!u64(v.rx_bytes)||!u64(v.tx_bytes)||!(v.rtt_ms===null||typeof v.rtt_ms==='number'&&Number.isFinite(v.rtt_ms)&&v.rtt_ms>=0&&v.rtt_ms<=60000)||!(v.peer_count===null||uint(v.peer_count,100000)))throw error('protocol_error');
  if(new TextEncoder().encode(JSON.stringify(v)).length>4096)throw error('protocol_error');
  return v;
 }catch{throw error('protocol_error');}
}
const labels={enabled:{on:'开启',off:'关闭',unknown:'未知'},service:{stopped:'已停止',starting:'启动中',running:'运行中',stopping:'停止中',failed:'失败',unknown:'未知'},path:{direct:'直连',relay:'中继',offline:'离线',unknown:'未知'},dht:{connected:'已连接',disconnected:'未连接',unknown:'未知'}};
const errors={engine_unavailable:'引擎不可用',deadline:'读取超时',protocol_error:'状态格式或上下文未确认',stale:'证据已陈旧',permission_denied:'读取权限被拒绝'};
export function nknStatusText(value,age){
 let v;try{v=parseNknStatus(value);}catch{return 'NKN 状态未知；没有有效的状态证据';}
 const origin={engine:'引擎状态源',mock:'离线模拟（不代表真实设备）',unavailable:'状态源不可用'}[v.source];
 const fresh=v.source!=='unavailable'&&Number.isFinite(age)&&age>=0&&age<=5000;
 const field=k=>fresh?v[k]:null;
 return [`来源：${origin}`,fresh?`证据年龄至少 ${Math.ceil(age)} ms`:'证据陈旧或年龄未知；动态字段均为未知',
  ...['enabled','service','path','dht'].map(k=>`${{enabled:'开关',service:'服务',path:'链路',dht:'DHT'}[k]}：${labels[k][field(k)]||'未知'}`),
  `接收字节：${field('rx_bytes')===null?'未知':field('rx_bytes')+' B'}`,
  `发送字节：${field('tx_bytes')===null?'未知':field('tx_bytes')+' B'}`,
  `往返延时：${field('rtt_ms')===null?'未知':field('rtt_ms')+' ms'}`,
  `连接节点数：${field('peer_count')===null?'未知':field('peer_count')}（不是用户数或安装数）`,
  `错误：${!fresh?'未知':v.last_error===null?'无已提供的错误':errors[v.last_error]}`,
  '开关、服务、DHT和直连/中继分别核对；关闭开关不证明进程已退出。'].join('\n');
}

export class NknStatusPanel{
 constructor({document,root,readStatus=null,clock=()=>performance.now(),wallClock=()=>Date.now()}){
  this.document=document;this.readStatus=readStatus;this.clock=clock;this.wallClock=wallClock;this.epoch=0;this.order=null;
  this.output=document.createElement('p');this.output.style.whiteSpace='pre-wrap';this.output.style.overflowWrap='anywhere';this.output.setAttribute('role','status');this.output.setAttribute('aria-live','polite');
  this.button=document.createElement('button');this.button.type='button';this.button.textContent='只读查询 NKN 状态';this.button.style.minHeight='44px';this.button.onclick=()=>this.refresh();root.replaceChildren(this.output,this.button);
  this.visibility=()=>{if(document.hidden)this.clear(false);else this.controls();};document.addEventListener('visibilitychange',this.visibility);this.clear(true);
 }
 controls(){this.button.disabled=this.busy||this.contextInvalid||this.document.hidden||typeof this.readStatus!=='function';}
 clear(resetOrder){this.epoch++;clearTimeout(this.staleTimer);this.controller?.abort();this.busy=false;this.snapshot=null;if(resetOrder){this.order=null;this.contextInvalid=false;}this.output.textContent=typeof this.readStatus==='function'?'NKN 状态未知；等待手动只读查询':'尚未接入经授权的 NKN 状态源；状态未知';this.controls();}
 reset(){this.clear(true);}
 render(){
  const s=this.snapshot;if(!s){this.output.textContent='NKN 状态未知';return;}
  let age=NaN;try{const elapsed=this.clock()-s.received,wallElapsed=this.wallClock()-s.wallReceived;if(s.data.age_ms!==null&&[elapsed,wallElapsed].every(v=>Number.isFinite(v)&&v>=0))age=s.age+Math.max(elapsed,wallElapsed);}catch{ /* A failed clock cannot establish fresh evidence. */ }
  this.output.textContent=nknStatusText(s.data,age);
 }
 async refresh(){
  if(this.busy||this.contextInvalid||this.document.hidden||typeof this.readStatus!=='function')return;
  const epoch=++this.epoch;this.busy=true;this.controls();clearTimeout(this.staleTimer);const controller=new AbortController();this.controller=controller;let timer;
  try{
   const started=this.clock(),wallStarted=this.wallClock();if(![started,wallStarted].every(Number.isFinite))throw error('protocol_error');
   const timeout=new Promise((_,reject)=>{timer=setTimeout(()=>{reject(error('deadline'));controller.abort();},3000);});
   const raw=await Promise.race([this.readStatus({signal:controller.signal}),timeout]);if(epoch!==this.epoch)return;
   const received=this.clock(),wallReceived=this.wallClock(),elapsed=received-started,wallElapsed=wallReceived-wallStarted;
   if(![elapsed,wallElapsed].every(v=>Number.isFinite(v)&&v>=0&&v<3000)){controller.abort();throw error('deadline');}
   const data=parseNknStatus(raw);
   if(data.source!=='unavailable'){
    if(this.order&&(data.source!==this.order.source||data.generation!==this.order.generation)){this.contextInvalid=true;throw error('protocol_error');}
    if(this.order&&data.sequence<=this.order.sequence)throw error('protocol_error');
    this.order={source:data.source,generation:data.generation,sequence:data.sequence};
   }
   this.snapshot={data,age:data.age_ms===null?NaN:data.age_ms+Math.max(elapsed,wallElapsed),received,wallReceived};this.render();
   if(Number.isFinite(this.snapshot.age)&&this.snapshot.age<=5000)this.staleTimer=setTimeout(()=>this.render(),5001-this.snapshot.age);
  }catch(failure){if(epoch===this.epoch){this.snapshot=null;this.output.textContent=`NKN 状态未知；${errors[failure?.code]||'读取未确认'}。没有启停引擎、测试网络或自动重试。`;}}
  finally{clearTimeout(timer);if(epoch===this.epoch){this.busy=false;this.controls();}}
 }
 dispose(){this.clear(true);this.readStatus=null;this.controls();this.document.removeEventListener('visibilitychange',this.visibility);}
}
