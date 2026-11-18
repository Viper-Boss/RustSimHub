// SPDX-License-Identifier: AGPL-3.0-only
// 控制台外壳与仪表盘。短信管理写操作均由用户确认，
// Core 仍要求所有者最近登录。开关和配置都跳到原业务区，保留那里的最近登录、确认和回执。
// 转发和 NKN 状态不在这里自动读取：forwarding_status 可能触发核对，这两项按原约定只由用户手动读取。
const record=v=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v:{};
const text=(v,max=64)=>typeof v==='string'&&v.trim()?v.trim().slice(0,max):null;
const finite=v=>typeof v==='number'&&Number.isFinite(v)?v:null;
const count=v=>Number.isSafeInteger(v)&&v>=0?v:null;
const p2=n=>String(n).padStart(2,'0');
export function stamp(d){return `${d.getFullYear()}-${p2(d.getMonth()+1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}`;}
export function bytesText(n){n=finite(n);if(n===null||n<0)return '—';if(n<1e6)return `${(n/1e3).toFixed(0)} KB`;if(n<1e9)return `${(n/1e6).toFixed(1)} MB`;return `${(n/1e9).toFixed(n<1e10?2:1)} GB`;}
export function uptimeText(s){s=count(s);if(s===null)return '—';return `${Math.floor(s/86400)} 天 ${p2(Math.floor(s/3600)%24)}:${p2(Math.floor(s/60)%60)}:${p2(s%60)}`;}
// CPU 占用必须来自两次采样之差；单次采样或计数回退都算未知，不显示 0。
export function cpuPercent(prev,next){const a=record(prev),b=record(next),[ab,at,bb,bt]=[a.cpu_busy_ticks,a.cpu_total_ticks,b.cpu_busy_ticks,b.cpu_total_ticks].map(count);if([ab,at,bb,bt].includes(null)||bt<=at||bb<ab)return null;return Math.min(100,Math.round((bb-ab)/(bt-at)*100));}
export function rateText(n){n=finite(n);if(n===null||n<0)return '—';const bits=n*8;return bits<1e6?`${(bits/1e3).toFixed(0)} Kbps`:`${(bits/1e6).toFixed(1)} Mbps`;}
export function usedPercent(total,free){total=count(total);free=count(free);if(!total||free===null||free>total)return null;return Math.round((total-free)/total*100);}
// 信号分级，四格；缺值或超范围一律 0 格。
const level=(v,steps)=>{const n=finite(v);if(n===null)return 0;return steps.reduce((sum,s)=>sum+(n>=s?1:0),0);};
export const rsrpLevel=v=>finite(v)!==null&&v<0&&v>-160?level(v,[-115,-105,-95,-85]):0;
export const rsrqLevel=v=>finite(v)!==null&&v<=0&&v>-40?level(v,[-19,-15,-12,-9]):0;
export const sinrLevel=v=>finite(v)!==null&&v>-30&&v<60?level(v,[0,5,13,20]):0;
export function signalLabel(percent){const p=finite(percent);if(p===null||p<0||p>100)return {bars:0,label:'未知',tone:'off'};const bars=p>=75?4:p>=50?3:p>=25?2:p>0?1:0;return {bars,label:['无信号','较弱','一般','良好','优秀'][bars],tone:bars>=2?'':'off'};}
const connected=d=>String(d?.state).startsWith('100 ');
const firstAddress=d=>Array.isArray(d?.ipv4)?text(String(d.ipv4[0]??'').split('/')[0],64):null;
export function networkView(sample){
 const s=record(sample),c=record(s.cellular),cell=record(c.serving_cell),band=record(cell.channel_derived).band,devices=Array.isArray(s.devices)?s.devices.map(record):[];
 const wifi=devices.find(d=>d.kind==='wifi'),modem=devices.find(d=>d.kind==='cellular');
 const tech=Array.isArray(c.access_technologies)?c.access_technologies.filter(t=>typeof t==='string').slice(0,3).map(t=>t.toUpperCase()==='LTE'?'4G (LTE)':t.toUpperCase()).join(' / '):'';
 return {operator:text(c.operator,32),tech:tech||null,registration:({home:'已注册',roaming:'漫游',searching:'正在搜索',denied:'注册被拒绝'})[c.registration]||null,band:Number.isInteger(band)&&band>0?`Band ${band}`:null,signal:signalLabel(c.signal_percent),rsrp:finite(cell.rsrp),rsrq:finite(cell.rsrq),sinr:finite(cell.sinr),
  wifi:{online:connected(wifi),present:!!wifi,name:text(wifi?.interface,32),ip:firstAddress(wifi)},data:{online:connected(modem),ip:connected(modem)?firstAddress(modem):null}};
}
// 设备时间：采样时间加上收到后经过的时间；与本机差超过60秒提示未同步。
export function deviceClock(sampleSeconds,receivedAt,now){const t=finite(sampleSeconds);if(t===null)return null;return {time:new Date(t*1000+(now-receivedAt)),synced:Math.abs(t*1000-receivedAt)<=60000};}
// 验证码识别：正文含验证码类关键词时，取第一个独立的4–8位数字。只用于展示和复制。
export function otpOf(preview){const s=text(preview,384);if(!s||!/验证码|校验码|动态码|动态密码|确认码|verification code|\bcode\b/i.test(s))return null;const m=s.match(/(?<![0-9])[0-9]{4,8}(?![0-9])/);return m?m[0]:null;}
export function maskNumber(value){const s=text(value,40);if(!s)return null;const m=s.match(/^(\+?86)?(1[3-9][0-9])([0-9]{4})([0-9]{4})$/);return m?`${m[1]?'+86 ':''}${m[2]}****${m[4]}`:s;}
// 通话列表没有“是否接通”和时间，所以这里只说方向和状态，不声称未接。
const CALL_STATES={outgoing_pending:'准备拨出',dialing:'正在拨号',ringing_out:'对方振铃',ringing_in:'来电振铃',active:'通话中',held:'保持中',waiting:'等待中',ended:'已结束',reconciling:'状态核对中'};
export function callView(call){
 const c=record(call),kind=c.direction===1?'in':c.direction===2?'out':'unknown';
 const state=typeof c.state==='string'&&Object.hasOwn(CALL_STATES,c.state)?c.state:null;
 return {id:typeof c.id==='string'?c.id:null,number:maskNumber(c.number)||'号码未知',kind,
  label:`${kind==='in'?'来电':kind==='out'?'去电':'方向未知'} · ${state?CALL_STATES[state]:'状态未知'}`,
  ended:state==='ended',live:['ringing_in','active','held','waiting','dialing','ringing_out'].includes(state)};
}
// Zoned timestamps are shown in this browser's local zone. Unzoned timestamps
// keep their wall-clock fields and are explicitly marked unknown-zone. Never
// turn invalid dates (including calendar overflow) into a current timestamp.
export function smsTime(value,now=new Date()){
 const raw=typeof value==='string'?value:'',s=raw.trim();
 const unknown={short:'时间未知',full:s?`时间无效（原文：${raw.slice(0,128)}）`:'时间未知',raw,zone:'unknown'};
 const m=/^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}(?::?\d{2})?)?$/.exec(s);
 if(!m)return unknown;
 const [y,mo,day,h,min,sec]=m.slice(1,7).map(Number);
 if(y<1||mo<1||mo>12||day<1||day>31||h>23||min>59||sec>59)return unknown;
 const wall=new Date(0);wall.setUTCFullYear(y,mo-1,day);wall.setUTCHours(h,min,sec,Number((m[7]||'').padEnd(3,'0').slice(0,3)));
 if(wall.getUTCFullYear()!==y||wall.getUTCMonth()!==mo-1||wall.getUTCDate()!==day)return unknown;
 const zone=m[8];let time=wall;
 if(zone&&zone!=='Z'){
  const digits=zone.slice(1).replace(':',''),hours=Number(digits.slice(0,2)),minutes=Number(digits.slice(2)||0);
  if(hours>23||minutes>59)return unknown;
  time=new Date(wall.getTime()-(zone[0]==='-'?-1:1)*(hours*60+minutes)*60000);
 }
 const fields=zone?[time.getFullYear(),time.getMonth()+1,time.getDate(),time.getHours(),time.getMinutes(),time.getSeconds()]:[y,mo,day,h,min,sec];
 const [year,month,date,hour,minute,second]=fields,same=year===now.getFullYear()&&month===now.getMonth()+1&&date===now.getDate();
 return {short:same?`${p2(hour)}:${p2(minute)}`:`${month}月${date}日`,full:`${year}-${p2(month)}-${p2(date)} ${p2(hour)}:${p2(minute)}:${p2(second)}（${zone?'本地时间':'时区未知'}）`,raw,zone:zone?'local':'unspecified'};
}
export function shortTime(value,now=new Date()){return smsTime(value,now).short;}
const LINE={ready:'SIM 就绪',locked:'SIM 已锁定',absent:'未插 SIM',unavailable:'SIM 不可用'};
// 导航：每页若干标签。view 是新页面区块；debug 显示全部原业务区（设置 → 设备调试），其逻辑和 ID 不变。
const DEBUG_TAB=3;
export const PAGES={
 dashboard:{title:'RustSimHub 控制台',desc:'设备管理平台',phone:['v-mhome']},
 hardware:{title:'硬件状态',desc:'处理器、内存、存储和温度',tabs:[{label:'概览',view:'v-hardware'}]},
 network:{title:'网络管理',desc:'联网状态、流量和网络接口',tabs:[{label:'概览',view:'v-network'}]},
 sim:{title:'SIM与调制解调器',desc:'SIM 状态、信号质量和当前小区',tabs:[{label:'概览',view:'v-sim'}]},
 comms:{title:'通信中心',desc:'短信、验证码、写短信和电话',tabs:[{label:'收件箱',view:'v-sms',phone:'v-mcomm'},{label:'写短信',view:'v-compose'},{label:'电话',view:'v-phone'},{label:'验证码',view:'v-sms'},{label:'回收站',view:'v-sms'}]},
 forwarding:{title:'短信转发',desc:'',tabs:[{label:'短信转发',show:['forwarding-panel']}]},
 gnss:{title:'GNSS定位',desc:'卫星定位状态',tabs:[{label:'定位',view:'v-gnss'}]},
 ble:{title:'蓝牙',desc:'通信模式与 Wi-Fi 配网',tabs:[{label:'蓝牙',view:'v-ble'}]},
 security:{title:'安全策略',desc:'当前身份、已授权设备和外部接入',tabs:[{label:'概览',view:'v-security'}]},
 logs:{title:'系统日志',desc:'本次打开页面以来的事件',tabs:[{label:'最近事件',view:'v-events'}]},
 settings:{title:'设置',desc:'账户、外观和设备调试',tabs:[{label:'账户',view:'v-account'},{label:'外观',view:'v-appearance'},{label:'关于',view:'v-about'},{label:'设备调试',debug:true}]},
 login:{title:'登录',desc:'',nav:'settings',tabs:[{label:'登录',show:['login-section']}]},
 more:{title:'更多',desc:'',tabs:[{label:'更多',view:'v-more'}]},
};
function transientListError(error){
 return error?.coreCode==='busy'||[429,502,503,504].includes(error?.httpStatus)||
  error?.name==='AbortError'||error instanceof TypeError;
}
const VIEWS=['v-mhome','v-hardware','v-network','v-sim','v-sms','v-mcomm','v-compose','v-phone','v-gnss','v-ble','v-security','v-events','v-account','v-appearance','v-about','v-more'];
const STRICT_BLOCKS=['login-section','session-section'];
export const scalePercent=(v,lo,hi)=>{const n=finite(v);return n===null?null:Math.max(0,Math.min(100,(n-lo)/(hi-lo)*100));};
const SPARK=30,POLL_SYSTEM=15000,POLL_LISTS=60000;
// 设备给的总数不可信时退回已读到的数量，不猜。
const countOf=(value,loaded)=>Number.isSafeInteger(value)&&value>=loaded?value:null;
const countText=(value,loaded)=>value===null?(loaded?`${loaded}+`:'—'):String(value);
const cursorOf=value=>{const c=record(value);return typeof c.db_epoch==='string'&&c.db_epoch&&typeof c.seq==='string'&&/^(0|[1-9][0-9]*)$/.test(c.seq)?c:null;};
const visibilityOf=value=>typeof value==='string'&&/^(0|[1-9][0-9]*)$/.test(value)?value:null;
const uniqueRows=rows=>{const seen=new Set();return rows.filter(row=>{if(!row.id)return true;if(seen.has(row.id))return false;seen.add(row.id);return true;});};
const VIEW_FIELDS=['c-mh-cpu','c-mh-mem','c-mh-disk','c-mh-temp','c-hw-model','c-hw-os','c-hw-kernel','c-hw-version','c-hw-cores','c-hw-uptime','c-hw-cpu','c-hw-load','c-hw-mem','c-hw-mem-d','c-hw-disk','c-hw-disk-d','c-hw-temp','c-ab-version','c-ab-model','c-ab-os','c-ab-kernel','c-sm-line','c-sm-op','c-sm-tech','c-sm-reg'];
export class ConsoleDashboard{
 constructor({document,request,copy,detailText=v=>String(v?.text??''),confirm=()=>false,storage=null,clock=()=>Date.now(),matchDark=()=>false,onLeave=()=>{},readDelay=ms=>new Promise(resolve=>setTimeout(resolve,ms))}){this.doc=document;this.request=request;this.copy=copy;this.storage=storage;this.clock=clock;this.onLeave=onLeave;this.readDelay=readDelay;this.matchDark=matchDark;this.epoch=0;this.active=false;this.sample=null;this.receivedAt=0;this.system=null;this.systemAt=0;this.temps=[];this.tempTimes=[];this.otps=[];this.events=[];this.seen=new Set();this.lastLine=null;this.page='dashboard';this.tab=0;this.timers=[];
  this.$=id=>document.getElementById(id);this.detailText=detailText;this.confirm=confirm;this.mail=[];this.recentMail=[];this.older=[];this.mailTotal=0;this.callRows=[];this.recentCalls=[];this.olderCalls=[];this.callTotal=0;this.expanded=new Set();this.selectedMail=new Set();this.selectedCalls=new Set();this.trash=[];
  let theme=null;try{theme=storage?.getItem('rustsimhub-theme');}catch{}
  const saved=theme==='dark'||theme==='light';this.setTheme(saved?theme:matchDark()?'dark':'light');this.mode=saved?theme:'system';for(const b of document.querySelectorAll('[data-mode]')){b.setAttribute('aria-pressed',String(b.dataset.mode===this.mode));b.addEventListener('click',()=>this.setMode(b.dataset.mode));}
  this.$('c-theme').addEventListener('click',()=>{this.setMode(this.doc.documentElement.dataset.theme==='dark'?'light':'dark');});
  for(const b of document.querySelectorAll('[data-go]'))b.addEventListener('click',()=>this.go(b.dataset.go));
  for(const b of document.querySelectorAll('[data-debug]'))b.addEventListener('click',()=>this.debug(b.dataset.debug));
  for(const b of document.querySelectorAll('[data-comm]'))b.addEventListener('click',()=>this.commSection(b.dataset.comm));
  for(const b of document.querySelectorAll('[data-mc]'))b.addEventListener('click',()=>this.commSection(b.dataset.mc));
  for(const b of document.querySelectorAll('[data-all]'))b.addEventListener('click',()=>{const k=b.dataset.all;if(this.expanded.has(k))this.expanded.delete(k);else this.expanded.add(k);this.renderComm();});
  for(const b of document.querySelectorAll('[data-tab]'))b.addEventListener('click',()=>this.go('comms',Number(b.dataset.tab)));
  this.$('c-back').addEventListener('click',()=>{const root=this.doc.documentElement;if(root.dataset.reading){this.clearMailDetail();return;}if(root.dataset.mailManage||this.page==='comms'&&(this.tab===3||this.tab===4)){this.go('comms');return;}this.go('dashboard');});
  this.$('c-mh-user').addEventListener('click',()=>this.go(this.active?'settings':'login'));
  this.$('c-mc-q').addEventListener('input',()=>this.renderComm());this.$('c-mc-f').addEventListener('change',()=>this.renderComm());
  this.$('c-mail-refresh').addEventListener('click',()=>void this.refreshSmsFolder());
  this.$('c-hw-temp-point').addEventListener('input',()=>this.showTempPoint());
  this.$('c-log-clear').addEventListener('click',()=>{if(this.history.length&&this.confirm('清空本页系统日志？')){this.history=[];this.renderLog();this.$('c-log-state').textContent='日志已清空。';}});
  this.$('c-log-save').addEventListener('click',()=>this.saveLogLimit());
  let capacity;
  try{capacity=Number(this.storage?.getItem('rustsimhub-log-limit'));}catch{}
  this.logLimit=Number.isInteger(capacity)&&capacity>=1&&capacity<=500?capacity:50;
  this.$('c-log-capacity').value=String(this.logLimit);
  this.$('c-mc-more').addEventListener('click',()=>void this.moreComm());
  this.$('c-mail-del').addEventListener('click',()=>void this.deleteMail());
  this.$('c-mail-select').addEventListener('click',()=>{this.mailSelecting=!this.mailSelecting;this.selectedMail.clear();this.renderMail();});
  this.$('c-mail-select-all').addEventListener('click',()=>{const rows=this.folderRows(),all=rows.every(row=>this.selectedMail.has(row.id));this.selectedMail=new Set(all?[]:rows.map(row=>row.id));this.renderMail();});
  this.$('c-mail-apply-selection').addEventListener('click',()=>void this.applySelection());
  this.$('c-mail-apply-folder').addEventListener('click',()=>void this.applyFolder());
  this.$('c-mc-manage').addEventListener('click',()=>{this.go('comms');this.doc.documentElement.dataset.mailManage='1';});
  this.$('c-mail-reply').addEventListener('click',()=>{if(!this.opened)return;this.$('c-cp-to').value=this.opened.raw||'';this.go('comms',1);});
  this.$('c-mail-copy').addEventListener('click',()=>{if(this.opened?.code)void this.copy(this.opened.code);});
  this.$('c-cp-text').addEventListener('input',()=>{this.$('c-cp-count').textContent=`${new TextEncoder().encode(this.$('c-cp-text').value).length} / 4096 字节`;});
  this.$('c-cp-send').addEventListener('click',()=>this.sendSms());
  this.$('c-call-select').addEventListener('click',()=>{this.callSelecting=!this.callSelecting;this.selectedCalls.clear();this.renderPhoneCalls();});
  this.$('c-call-select-all').addEventListener('click',()=>{
   const rows=this.callRows.filter(call=>call.ended&&call.id);
   const all=rows.every(call=>this.selectedCalls.has(call.id));
   this.selectedCalls=new Set(all?[]:rows.map(call=>call.id));this.renderPhoneCalls();
  });
  this.$('c-call-delete').addEventListener('click',()=>void this.removeCallHistory(false));
  this.$('c-call-clear').addEventListener('click',()=>void this.removeCallHistory(true));
  this.$('c-ph-keys').addEventListener('click',e=>{const key=e.target?.dataset?.key,input=this.$('c-ph-num');if(key&&input.value.length<32)input.value+=key;});
  this.$('c-ph-back').addEventListener('click',()=>{const input=this.$('c-ph-num');input.value=input.value.slice(0,-1);});
  this.$('c-ph-dial').addEventListener('click',()=>this.dial());this.$('c-ph-hang').addEventListener('click',()=>this.press('phone-hangup','c-ph-state','现在没有可挂断的通话'));
  this.$('c-ac-out').addEventListener('click',()=>this.$('daily-logout').click());this.$('c-ac-done').addEventListener('click',()=>this.$('configuration-done').click());
  this.$('c-sc-load').addEventListener('click',()=>void this.loadClients());
  this.$('c-as-load').addEventListener('click',()=>{this.watchAirsim();void this.airsim({op:'airsim_status'});});
  this.$('c-as-on').addEventListener('click',()=>void this.airsim({op:'airsim_configure',enabled:true,pcm_rate:Number(this.$('c-as-rate').value)}));
  this.$('c-as-save').addEventListener('click',()=>void this.airsim({op:'airsim_configure',enabled:true,pcm_rate:Number(this.$('c-as-rate').value)}));
  this.$('c-as-off').addEventListener('click',()=>{if(this.confirm('停用 AirSIM 直连？\n\n所有已配对的 iPhone 会立即失效，重新启用后它们可以继续使用。'))void this.airsim({op:'airsim_configure',enabled:false,pcm_rate:Number(this.$('c-as-rate').value)});});
  this.$('c-as-pair').addEventListener('click',()=>void this.airsimPair());
  this.$('c-ph-live').append(this.$('phone-live'));
  this.mirror('sms-send-state','c-cp-state');this.mirror('phone-state','c-ph-state');
  this.$('c-user').addEventListener('click',()=>this.go(this.active?'settings':'login'));
  this.$('c-bell').addEventListener('click',()=>this.go('logs',0));
  const dialog=this.$('c-otp-dialog');this.$('c-otp-clear').addEventListener('click',()=>{if(!this.otps.length||this.clearing)return;this.$('c-otp-dialog-title').textContent=`清理 ${this.otps.length} 条验证码短信？`;dialog.showModal();});
  dialog.addEventListener('close',()=>{const clear=dialog.returnValue==='clear';dialog.returnValue='';if(clear)void this.clearOtps();});
  for(const list of [this.$('c-mail-list'),this.$('c-mc-g-sms')])if(list.parentElement){const note=this.doc.createElement('p');note.className='console-order-note';if(list.id==='c-mail-list')this.mailOrderNote=note;note.textContent='按设备收到顺序（新→旧），不是短信自带时间排序。时间按浏览器本地时区显示；无时区会明确标注。计数为设备本地记录，含历史线路、不含短信回收站。';list.parentElement.insertBefore(note,list);}
  this.go('dashboard');this.reset();this.tick();}
 setTheme(theme){this.doc.documentElement.dataset.theme=theme;this.$('c-theme-icon').setAttribute('href',theme==='dark'?'#i-sun':'#i-moon');}
 go(target,tab=0,openId=null){const page=PAGES[target];if(!page)return;const tabs=page.tabs||[];const index=tabs[tab]?tab:0;const changed=target!==this.page||index!==this.tab;if(changed)this.onLeave();if(this.page==='security'&&target!=='security')this.hideAirsimCode();this.page=target;this.tab=index;this.watchAirsim();
  const root=this.doc.documentElement;root.dataset.page=target;root.dataset.tab=String(index);delete root.dataset.reading;delete root.dataset.mailManage;
  if(changed){this.mailSelecting=false;this.selectedMail.clear();this.clearMailDetail();this.$('c-mail-operation').textContent='';this.renderMail();if(target==='comms'&&this.active){if(index===4)void this.loadTrash();else if(index===0||index===2||index===3)void this.readLists();}}
  const navTarget=page.nav||target;for(const b of this.doc.querySelectorAll('.nav[data-go]'))if(b.dataset.go===navTarget)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');
  const mobile={dashboard:'dashboard',comms:'comms',hardware:'hardware',network:'network'}[navTarget]||'more';for(const b of this.doc.querySelectorAll('[data-mnav]'))if(b.dataset.mnav===mobile)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');
  this.$('c-title').textContent=page.title;const sub=this.$('c-subtitle');sub.textContent=page.desc||'';sub.hidden=!page.desc;const dashboard=target==='dashboard';this.$('c-dash').hidden=!dashboard;this.$('c-back').hidden=dashboard;
  const bar=this.$('c-tabs');bar.replaceChildren();bar.hidden=tabs.length<2;if(tabs.length>1)tabs.forEach((t,i)=>{const b=this.doc.createElement('button');b.type='button';b.textContent=t.label;b.setAttribute('aria-pressed',String(i===index));b.addEventListener('click',()=>this.go(target,i));bar.append(b);});
  const current=tabs[index]||{},visible=new Set([...(page.phone||[]),current.view,current.phone].filter(Boolean));for(const id of VIEWS)this.$(id).hidden=!visible.has(id);
  const legacy=this.$('c-legacy');legacy.classList.toggle('debug',current.debug===true);const show=current.show||[];legacy.hidden=!show.length&&!current.debug;
  if(current.debug){for(const child of legacy.children)child.classList.remove('page-off');for(const id of STRICT_BLOCKS)this.$(id).classList.remove('page-off');this.$('c-empty').classList.add('page-off');
   for(const child of legacy.children)if(child.tagName==='DETAILS')child.open=false;
   if(openId){const node=this.$(openId);for(let n=node;n&&n!==legacy;n=n.parentElement)if(n.tagName==='DETAILS')n.open=true;node?.scrollIntoView?.({block:'start'});}return;}
  if(!show.length)return;const keep=new Set();for(const id of show){for(let n=this.$(id);n&&n!==legacy;n=n.parentElement){keep.add(n);if(n.tagName==='DETAILS')n.open=true;}}
  for(const child of legacy.children)child.classList.toggle('page-off',!keep.has(child)&&child.id!=='notice');for(const id of STRICT_BLOCKS)this.$(id).classList.toggle('page-off',!keep.has(this.$(id)));this.$('c-empty').classList.add('page-off');}
 debug(id){this.go('settings',DEBUG_TAB,id);}
 mirror(from,to){const source=this.$(from),target=this.$(to),copy=()=>{if(source.textContent)target.textContent=source.textContent;};copy();if(globalThis.MutationObserver)new MutationObserver(copy).observe(source,{childList:true,characterData:true,subtree:true});}
 press(id,stateId,reason){const button=this.$(id);if(button.disabled){this.$(stateId).textContent=this.active?reason:'请先登录';return false;}button.click();return true;}
 sendSms(){const to=this.$('recipient'),body=this.$('text');to.value=this.$('c-cp-to').value.trim();body.value=this.$('c-cp-text').value;body.dispatchEvent(new Event('input',{bubbles:true}));this.press('send-sms','c-cp-state','现在不能发送：有未完成的操作，请到“设备调试”查询');}
 dial(){const input=this.$('phone-number');input.value=this.$('c-ph-num').value.trim();input.dispatchEvent(new Event('input',{bubbles:true}));this.press('phone-dial','c-ph-state','现在不能拨号');}
 commSection(kind){this.go('comms',kind==='otp'?3:0);for(const b of this.doc.querySelectorAll('[data-mc]'))b.setAttribute('aria-pressed',String(b.dataset.mc===kind));this.$(`c-mc-g-${kind}`)?.scrollIntoView?.({behavior:'smooth',block:'start'});}
 commMatch(value){const q=this.$('c-mc-q').value.trim();return !q||String(value).includes(q);}
 renderComm(){const filter=this.$('c-mc-f').value;for(const k of ['sms','otp','call'])this.$(`c-mc-g-${k}`).hidden=filter!=='all'&&filter!==k;
  const moreMail=this.canMoreMail(),moreCalls=this.canMoreCalls(),searching=!!this.$('c-mc-q').value.trim();
  this.$('c-mc-more').hidden=!(moreCalls&&this.expanded.has('call'));
  const fill=(key,rows,draw)=>{const box=this.$(`c-mc-${key}`);box.replaceChildren();const shown=key!=='call'||this.expanded.has(key)?rows:rows.slice(0,3);this.$(`c-mc-n-${key}`).textContent=key==='call'?(searching?String(rows.length):countText(this.callTotal,this.callRows.length)):`${rows.length}${moreMail?'+':''}`;const empty=this.$(`c-mc-e-${key}`);empty.hidden=rows.length>0;if(this.active&&!rows.length)empty.textContent='没有内容';for(const r of shown)box.append(draw(r));};
  const avatar=(m,mark)=>{const a=this.doc.createElement('span');a.className=`brand t-${mark?'gray':'blue'}`;a.textContent=mark||[...m].find(ch=>/[^\d\s*+]/.test(ch))||'信';return a;};
  const shell=(m,sub,time,onClick,mark)=>{const b=this.doc.createElement('button');b.type='button';b.className='mc-it';const main=this.doc.createElement('span');main.className='m';const t=this.doc.createElement('b');t.textContent=m;const s=this.doc.createElement('small');s.textContent=sub;main.append(t,s);const w=this.doc.createElement('time');w.textContent=time||'';b.append(avatar(m,mark),main);if(time)b.append(w);if(onClick)b.addEventListener('click',onClick);return b;};
  const mail=this.mail.filter(m=>this.commMatch(m.sender+m.preview));
  fill('sms',mail.filter(m=>!m.code),m=>shell(m.sender,m.preview,m.time,()=>void this.openMail(m,true)));
  fill('otp',mail.filter(m=>m.code),m=>{const row=shell(m.sender,m.preview,m.time,()=>void this.openMail(m,true));const code=this.doc.createElement('span');code.className='mc-code num';code.textContent=m.code;code.setAttribute('role','button');code.addEventListener('click',async e=>{e.stopPropagation();try{await this.copy(m.code);code.textContent='已复制';code.classList.add('done');}catch{code.textContent='复制失败';}setTimeout(()=>{code.textContent=m.code;code.classList.remove('done');},1200);});row.insertBefore(code,row.lastChild);return row;});
  fill('call',this.callRows.filter(c=>this.commMatch(c.number)),c=>shell(c.number,c.label,'',null,c.kind==='in'?'↙':c.kind==='out'?'↗':'?'));}
 // 短信和通话的总数来自设备；验证码由页面从预览识别，没读完时只说已读范围内的数量。
 renderMobileCounts(){const otp=this.mail.filter(m=>m.code).length;this.$('c-mh-q-sms').textContent=this.active?countText(this.mailTotal,this.mail.length):'—';this.$('c-mh-q-otp').textContent=this.active?(this.canMoreMail()?`${otp}+`:String(otp)):'—';this.$('c-mh-q-call').textContent=this.active?countText(this.callTotal,this.callRows.length):'—';}
 smsFolder(){return this.page==='comms'?(this.tab===4?'trash':this.tab===3?'otp':'inbox'):'inbox';}
 folderRows(){
  const folder=this.smsFolder();
  return folder==='trash'?this.trash:folder==='otp'?this.mail.filter(mail=>mail.code):this.mail;
 }
 clearMailDetail(){
  this.opened=null;delete this.doc.documentElement.dataset.reading;
  this.$('c-mail-open').hidden=true;this.$('c-mail-blank').hidden=false;
  this.$('c-mail-body').textContent='';this.$('c-mail-state').textContent='';
 }
 mailButtons(){
  const folder=this.smsFolder(),trash=folder==='trash',rows=this.folderRows(),busy=!!this.mailAction;
  this.$('c-mail-title').textContent=trash?'回收站':folder==='otp'?'验证码':'收件箱';
  this.$('c-mail-select').textContent=this.mailSelecting?'取消选择':'选择';
  this.$('c-mail-select').disabled=!this.active||!rows.length||busy;
  this.$('c-mail-batch').hidden=!this.mailSelecting;
  this.$('c-mail-selection-count').textContent=`已选 ${this.selectedMail.size} 条`;
  this.$('c-mail-select-all').textContent=rows.length&&rows.every(row=>this.selectedMail.has(row.id))?'取消全选':'全选';
  this.$('c-mail-select-all').disabled=!rows.length||busy;
  this.$('c-mail-apply-selection').textContent=trash?'恢复所选':'移到回收站';
  this.$('c-mail-apply-selection').disabled=busy||!this.selectedMail.size;
  this.$('c-mail-folder-actions').hidden=folder==='otp'||!this.active;
  this.$('c-mail-apply-folder').textContent=trash?'清空回收站':'全选整箱并移到回收站';
  this.$('c-mail-apply-folder').disabled=busy||!(trash?this.trash.length:this.mail.length);
  this.$('c-mail-del').disabled=busy;
  this.$('c-mail-refresh').disabled=busy;
  if(this.mailOrderNote)this.mailOrderNote.hidden=trash;
 }
 renderMail(){
  const folder=this.smsFolder(),rows=this.folderRows(),box=this.$('c-mail-list'),ids=new Set(rows.map(m=>m.id));
  for(const id of this.selectedMail)if(!ids.has(id))this.selectedMail.delete(id);
  box.replaceChildren();
  const empty=this.$('c-mail-empty');
  empty.hidden=rows.length>0;
  empty.textContent=!this.active?'登录后显示':folder==='trash'?'回收站为空':folder==='otp'?
   (this.canMoreMail()?'正在读取全部短信…':'没有验证码短信'):'没有短信';
  for(const m of rows){
   const b=this.doc.createElement('button');b.type='button';b.className='mail-it';
   const a=this.doc.createElement('span');a.className='brand t-blue';a.textContent=[...m.sender].find(ch=>/[^\d\s*+]/.test(ch))||'信';
   const t=this.doc.createElement('b');t.textContent=m.sender;
   const w=this.doc.createElement('time');w.textContent=m.time||'';
   const p=this.doc.createElement('span');p.className='p';
   if(folder==='otp'){
    const code=this.doc.createElement('strong');code.className='mail-code';code.textContent=m.code;p.append(code);
   }
   p.append(m.preview);b.append(a,t,w,p);
   if(this.opened?.id===m.id)b.setAttribute('aria-current','true');
   b.addEventListener('click',()=>void this.openMail(m,this.doc.defaultView?.matchMedia?.('(max-width: 767px)').matches));
   if(this.mailSelecting){
    const row=this.doc.createElement('div'),check=this.doc.createElement('input');
    row.className='mail-choice';check.type='checkbox';check.checked=this.selectedMail.has(m.id);
    check.setAttribute('aria-label',`选择来自 ${m.sender} 的短信`);check.disabled=!!this.mailAction;
    check.addEventListener('change',()=>{if(check.checked)this.selectedMail.add(m.id);else this.selectedMail.delete(m.id);this.mailButtons();});
    row.append(check,b);box.append(row);
   }else box.append(b);
  }
  this.mailButtons();
 }
 async openMail(m,phone=false){
  const epoch=this.epoch,trash=this.smsFolder()==='trash';
  this.opened=m;if(phone)this.doc.documentElement.dataset.reading='1';
  this.$('c-mail-blank').hidden=true;this.$('c-mail-open').hidden=false;this.$('c-mail-from').textContent=m.sender;
  this.$('c-mail-time').textContent=m.fullTime||m.time||'';this.$('c-mail-av').textContent=[...m.sender].find(ch=>/[^\d\s*+]/.test(ch))||'信';
  this.$('c-mail-body').textContent=m.preview;this.$('c-mail-copy').hidden=trash||!m.code;this.$('c-mail-reply').hidden=trash;
  this.$('c-mail-del').textContent=trash?'恢复到收件箱':'移到回收站';this.$('c-mail-state').textContent='';this.renderMail();
  try{
   const detail=await this.request({op:trash?'deleted_sms_detail':'sms_detail',sms_id:m.id});
   if(epoch!==this.epoch||this.opened!==m)return;
   this.$('c-mail-time').textContent=smsTime(record(detail).timestamp??m.rawTimestamp,new Date(this.clock())).full;
   const full=String(this.detailText(detail)),parts=full.split('\n\n');
   this.$('c-mail-body').textContent=parts.length>1?parts.slice(1).join('\n\n'):full;
  }catch{
   if(epoch===this.epoch&&this.opened===m)this.$('c-mail-state').textContent='完整正文暂时读不到，上面是列表预览';
   return;
  }
  if(!trash)void this.optional({op:'notification_read',entity:'sms',record_id:m.id});
 }
 async refreshSmsFolder(){
  if(this.smsFolder()==='trash')await this.loadTrash();
  else await this.readLists(true);
 }
 async loadTrash(){
  if(!this.active||this.trashLoad)return;
  const epoch=this.epoch,flight={};this.trashLoad=flight;
  const records=[],ids=new Set();let after=null;
  try{
   while(this.active&&epoch===this.epoch){
    const page=record(await this.readListPage({op:'deleted_inbox',after,limit:32},()=>this.active&&epoch===this.epoch));
    if(epoch!==this.epoch)return;
    const next=cursorOf(page.cursor),rows=page.messages;
    if(!next||!Array.isArray(rows)||rows.length>32||
       after&&(next.db_epoch!==after.db_epoch||rows.length&&BigInt(next.seq)<=BigInt(after.seq)))throw Error('回收站列表已变化');
    for(const row of rows){
     if(typeof row?.id!=='string'||ids.has(row.id))throw Error('回收站列表已变化');
     ids.add(row.id);records.push(this.mailView(row));
    }
    if(rows.length<32){this.trash=records.reverse();return;}
    after=next;
   }
  }catch{
   if(epoch===this.epoch&&this.smsFolder()==='trash')this.$('c-mail-operation').textContent='读取回收站失败，请刷新重试。';
  }finally{
   if(this.trashLoad===flight){this.trashLoad=null;if(this.smsFolder()==='trash')this.renderMail();}
  }
 }
 async performSmsAction(action){
  if(!this.active||this.mailAction)return;
  const epoch=this.epoch,folder=this.smsFolder(),marker={},state=this.$('c-mail-operation');
  this.mailAction=marker;this.renderMail();
  const current=()=>{if(epoch!==this.epoch||this.page!=='comms'||this.smsFolder()!==folder)throw Error('会话或短信页面已变化');};
  try{
   const result=await action(current);
   current();if(result===null)return;
   const {changed,label}=result;
   this.selectedMail.clear();this.mailSelecting=false;this.clearMailDetail();
   await this.refreshSmsFolder();current();
   if(this.smsFolder()==='trash')void this.readLists(true);
   state.textContent=`已${label} ${changed} 条短信`;
  }catch{
   if(epoch===this.epoch&&this.page==='comms')state.textContent='操作未全部确认，请刷新核对后再试。';
  }finally{if(this.mailAction===marker){this.mailAction=null;this.renderMail();}}
 }
 async applySelection(){
  const ids=[...this.selectedMail],trash=this.smsFolder()==='trash',label=trash?'恢复':'移到回收站';
  if(!ids.length||!this.confirm(`将所选 ${ids.length} 条短信${trash?'恢复到收件箱':'移到本地回收站'}？`))return;
  await this.performSmsAction(async current=>{
   let changed=0;
   for(let i=0;i<ids.length;i+=32){
    current();const sms_ids=ids.slice(i,i+32);
    const result=record(await this.request({op:'set_sms_deleted_batch',sms_ids,deleted:!trash}));
    if(count(result.changed)===null||result.changed>sms_ids.length)throw Error('回执无效');
    changed+=result.changed;
   }
   return {changed,label};
  });
 }
 async applyFolder(){
  const folder=this.smsFolder(),purge=folder==='trash';
  if(folder==='otp')return;
  await this.performSmsAction(async current=>{
   const info=record(await this.request({op:'sms_folder_status'}));current();
   if(this.smsFolder()!==folder)return null;
   const total=count(purge?info.trash:info.inbox),version=cursorOf(info.version),head_seq=visibilityOf(info.head_seq);
   if(total===null||!version||!head_seq)throw Error('文件夹状态无效');
   if(!total){this.$('c-mail-operation').textContent=purge?'回收站已空。':'收件箱已空。';return null;}
   if(!this.confirm(purge?`永久删除回收站中的 ${total} 条短信？清空后无法从回收站恢复。`:
     `把收件箱的全部 ${total} 条短信移到回收站？验证码也会移入回收站。`))return null;
   current();
   const result=record(await this.request({op:'apply_sms_folder',version,head_seq,purge}));
   if(result.changed!==total)throw Error('回执无效');
   return {changed:total,label:purge?'永久删除':'移到回收站'};
  });
 }
 canMoreMail(){return !!this.mailCursor&&!this.mailExhausted&&(this.mailTotal===null||this.mail.length<this.mailTotal);}
 canMoreCalls(){return !!this.callCursor&&!this.callExhausted&&(this.callTotal===null||this.callRows.length<this.callTotal);}
 async readAllMail(head,valid){
  const rows=Array.isArray(head.messages)?head.messages.slice():[];
  let cursor=cursorOf(head.cursor);
  const visibility=visibilityOf(head.visibility_seq),total=count(head.total);
  if(!cursor||visibility===null||rows.length>32)throw Error('短信列表响应无效');
  const ids=new Set(rows.map(row=>row?.id));
  if(ids.has(undefined)||ids.size!==rows.length)throw Error('短信列表响应无效');
  let full=rows.length===32;
  while(total===null?full:rows.length<total){
   const page=record(await this.readListPage({op:'inbox_recent',before:cursor,limit:32},valid));
   if(!valid())return null;
   const next=cursorOf(page.cursor),batch=page.messages;
   if(!next||next.db_epoch!==cursor.db_epoch||visibilityOf(page.visibility_seq)!==visibility||
      count(page.total)!==total||!Array.isArray(batch)||batch.length>32||
      batch.length&&BigInt(next.seq)>=BigInt(cursor.seq))throw Error('收件箱已变化');
   for(const row of batch){
    if(typeof row?.id!=='string'||ids.has(row.id))throw Error('短信列表已变化');
    ids.add(row.id);rows.push(row);
   }
   cursor=next;full=batch.length===32;
   if(!full)break;
  }
  if(total!==null&&total!==rows.length)throw Error('短信未全部读取');
  return {rows,cursor,visibility,total};
 }
 async moreCalls(limit=16){
  if(!this.active||!this.canMoreCalls()||this.callLoading)return;
  const epoch=this.epoch,run=this.listRun,cursor=this.callCursor,flight={};this.callLoading=flight;
  try{
   const page=await this.optional({op:'calls_recent',before:cursor,limit});
   if(epoch!==this.epoch||run!==this.listRun||this.callCursor!==cursor||!this.active)return;
   if(!page){this.forgetCalls('通话记录暂不可用，请检查登录或权限');return;}
   const next=cursorOf(record(page).cursor),rows=Array.isArray(page.calls)?page.calls:[];
   if(!next||next.db_epoch!==cursor.db_epoch||rows.length>limit||rows.length&&BigInt(next.seq)>=BigInt(cursor.seq)){this.forgetCalls('通话记录已变化，请刷新后继续');return;}
   this.callCursor=next;this.callExhausted=!rows.length||(count(record(page).total)===null&&rows.length<limit);this.olderCalls=uniqueRows([...this.olderCalls,...rows.map(callView)]);this.applyCalls(record(page).total);
  }finally{if(this.callLoading===flight)this.callLoading=null;}
 }
 async moreComm(){if(this.canMoreCalls()&&this.expanded.has('call'))await this.moreCalls();}
 async deleteMail(){
  const m=this.opened,trash=this.smsFolder()==='trash';
  if(!m||!this.confirm(trash?'恢复这条短信到收件箱？':'把这条短信移到本地回收站？'))return;
  await this.performSmsAction(async current=>{
   const result=record(await this.request({op:trash?'restore_sms':'delete_sms',sms_id:m.id}));
   current();
   if(result.sms_id!==m.id||result.deleted!==!trash)throw Error('回执无效');
   return {changed:1,label:trash?'恢复':'移到回收站'};
  });
 }
 renderPhoneCalls(){
  const rows=this.callRows,eligible=rows.filter(call=>call.ended&&call.id);
  const ids=new Set(eligible.map(call=>call.id));
  for(const id of this.selectedCalls)if(!ids.has(id))this.selectedCalls.delete(id);
  const ready=this.active&&!this.callAction;
  this.$('c-call-select').disabled=!ready||!eligible.length;
  this.$('c-call-select').textContent=this.callSelecting?'取消选择':'选择';
  this.$('c-call-select-all').hidden=!this.callSelecting;
  this.$('c-call-select-all').disabled=!ready||!eligible.length||this.canMoreCalls();
  this.$('c-call-select-all').textContent=eligible.length&&eligible.every(call=>this.selectedCalls.has(call.id))?'取消全选':'全选';
  this.$('c-call-delete').hidden=!this.callSelecting;
  this.$('c-call-delete').disabled=!ready||!this.selectedCalls.size;
  this.$('c-call-delete').textContent=this.selectedCalls.size?'删除所选（'+this.selectedCalls.size+'）':'删除所选';
  this.$('c-call-clear').disabled=!ready||!this.haveCalls;
  const box=this.$('c-ph-calls');box.replaceChildren();
  this.$('c-ph-calls-empty').hidden=rows.length>0;
  this.$('c-ph-calls-empty').textContent=this.active?'没有通话记录':'登录后显示';
  for(const call of rows){
   const row=this.doc.createElement('div');row.className='call-history-row';
   if(this.callSelecting&&call.ended&&call.id){
    const check=this.doc.createElement('input');check.type='checkbox';check.checked=this.selectedCalls.has(call.id);check.disabled=!ready;
    check.setAttribute('aria-label','选择 '+call.label);
    check.addEventListener('change',()=>{if(check.checked)this.selectedCalls.add(call.id);else this.selectedCalls.delete(call.id);this.renderPhoneCalls();});
    row.append(check);
   }
   this.item(row,{tone:call.live?'green':call.kind==='in'?'blue':'gray',title:call.number,body:call.label});box.append(row);
  }
 }
 async removeCallHistory(all){
  if(!this.active||this.callAction)return;
  const epoch=this.epoch,marker={},status=this.$('c-call-operation');
  const valid=()=>epoch===this.epoch&&this.callAction===marker;
  this.callAction=marker;this.renderPhoneCalls();
  try{
   let changed=0;
   if(all){
    const info=record(await this.request({op:'call_history_status'}));
    if(!valid())return;
    const through=cursorOf(info.through),expected=count(info.ended);
    if(!through||expected===null)throw Error('通话记录范围无效');
    if(!expected){status.textContent='没有已结束的通话记录。';return;}
    if(!this.confirm('删除全部 '+expected+' 条已结束的通话记录？'))return;
    const result=record(await this.request({op:'clear_call_history',through,expected}));
    if(result.changed!==expected)throw Error('回执无效');
    changed=expected;
   }else{
    const ids=[...this.selectedCalls];
    if(!ids.length||!this.confirm('删除所选 '+ids.length+' 条通话记录？'))return;
    for(let offset=0;offset<ids.length;offset+=32){
     if(!valid())return;
     const call_ids=ids.slice(offset,offset+32);
     const result=record(await this.request({op:'delete_call_history',call_ids}));
     if(!Number.isSafeInteger(result.changed)||result.changed<0||result.changed>call_ids.length)throw Error('回执无效');
     changed+=result.changed;
    }
   }
   if(!valid())return;
   this.callSelecting=false;this.selectedCalls.clear();
   await this.readLists(true);
   if(valid())status.textContent='已删除 '+changed+' 条通话记录。';
  }catch{
   if(valid()){status.textContent='操作未全部确认，正在核对记录。';await this.readLists(true);}
  }finally{
   if(valid()){this.callAction=null;this.renderPhoneCalls();}
  }
 }

 // AirSIM 直连：页面不保存任何状态，全部以设备返回为准；配对码只在本页显示。
 hideAirsimCode(){clearInterval(this.asTimer);this.asTimer=null;this.$('c-as-code').hidden=true;this.$('c-as-digits').textContent='';this.$('c-as-hint').textContent='';}
 watchAirsim(){this.asView=(this.asView||0)+1;this.asReadFailures=0;this.asReadsLeft=30;this.asNextRead=this.clock();this.asWatchUntil=this.active&&this.page==='security'?this.clock()+150000:0;}
 resetAirsim(){this.asWatchUntil=0;this.asReadsLeft=0;this.asRate=null;this.asView=(this.asView||0)+1;this.asFlight=null;this.asBusy=false;this.hideAirsimCode();const s=this.$('c-as-state');s.textContent='未读取';s.classList.add('off');this.$('c-as-list').replaceChildren();for(const id of ['c-as-save','c-as-pair','c-as-off'])this.$(id).hidden=true;this.$('c-as-on').hidden=false;const e=this.$('c-as-empty');e.hidden=false;e.textContent='请用所有者浏览器登录；修改设置需要最近 5 分钟内登录';this.$('c-as-msg').textContent='';}
 renderAirsim(value,keepRate=false){const v=record(value);if(typeof v.enabled!=='boolean'||![8000,16000].includes(v.pcm_rate)||!Array.isArray(v.pairings)||v.pairings.length>16||v.pairings.some(p=>!p||typeof p!=='object'||!/^[0-9a-f]{24}$/.test(p.id)))throw Error('配对状态响应无效');const on=v.enabled,rate=String(v.pcm_rate),list=v.pairings.map(record);
  const s=this.$('c-as-state');s.textContent=on?'已启用':'未启用';s.classList.toggle('off',!on);if(!keepRate||!this.asRate||this.$('c-as-rate').value===this.asRate)this.$('c-as-rate').value=rate;this.asRate=rate;this.$('c-as-on').hidden=on;for(const id of ['c-as-save','c-as-pair','c-as-off'])this.$(id).hidden=!on;if(!on||!(count(v.window_seconds)>0))this.hideAirsimCode();
  const box=this.$('c-as-list'),empty=this.$('c-as-empty');box.replaceChildren();empty.hidden=list.length>0;empty.textContent=on?'还没有配对的 iPhone':'启用后可以配对 iPhone';
  for(const p of list){const row=this.doc.createElement('div'),id=text(p.id,24);for(const cell of [`iPhone ${text(p.device_hint,8)?'…'+text(p.device_hint,8):''}`.trim(),text(p.last_peer,45)||'—',count(p.last_seen_ms)?`最近 ${stamp(new Date(p.last_seen_ms))}`:'—']){const c=this.doc.createElement('span');c.textContent=cell;row.append(c);}
   const b=this.doc.createElement('button');b.type='button';b.className='pg-link';b.textContent='解除配对';b.addEventListener('click',()=>{if(/^[0-9a-f]{24}$/.test(id)&&this.confirm('解除这台 iPhone 的配对？\n\n它会立即无法使用，需要重新配对。'))void this.airsim({op:'airsim_revoke',pairing_id:id},true);});row.append(b);box.append(row);}}
 async airsim(command,reload=false,automatic=false){if(this.asBusy||!this.active)return null;const epoch=this.epoch,view=this.asView,flight={},msg=this.$('c-as-msg'),reading=command.op==='airsim_status',current=()=>epoch===this.epoch&&view===this.asView;this.asBusy=true;this.asFlight=flight;if(!automatic)msg.textContent=reading?'正在读取配对列表…':'';
  try{let r=await this.request(command);if(!current())return null;if(reload){r=await this.request({op:'airsim_status'});if(!current())return null;}this.renderAirsim(r,reading);this.asReadFailures=0;if(reading||reload)msg.textContent=`已读取设备状态：${r.pairings.length} 条 iPhone 配对记录。`;return r;}
  catch(error){if(current()){if(reading){this.asReadFailures=(this.asReadFailures||0)+1;if(this.asReadFailures>=3)this.asWatchUntil=0;msg.textContent=`配对列表暂未更新：${text(error?.message,120)||'请检查登录和设备连接'}。保留上次显示的记录，不会撤销手机配对；可重新登录后点“读取状态”。`;}else msg.textContent=`未完成：${text(error?.message,120)||'需要所有者最近 5 分钟内登录'}。设备上的设置没有被这次操作改变或结果未知，请点“读取状态”核对。`;}return null;}
  finally{if(this.asFlight===flight){this.asFlight=null;this.asBusy=false;}}}
 async airsimPair(){if(this.asBusy||!this.active)return;const epoch=this.epoch,view=this.asView,flight={},msg=this.$('c-as-msg');this.asBusy=true;this.asFlight=flight;msg.textContent='';
  try{const r=record(await this.request({op:'airsim_open_pairing'}));if(epoch!==this.epoch||view!==this.asView)return;const code=text(r.code,6),until=count(r.expires_at),port=count(r.pairing_port);if(!/^[0-9]{6}$/.test(code)||!until||!port)throw Error('设备响应无效');
   this.hideAirsimCode();this.$('c-as-digits').textContent=code;this.$('c-as-code').hidden=false;const host=text(this.doc.location?.hostname,64)||'棒子的 IP';
   const tick=()=>{const left=until-Math.floor(this.clock()/1000);if(left<=0||epoch!==this.epoch){this.hideAirsimCode();return;}this.$('c-as-hint').textContent=`在 iPhone 的 AirSIM 里选“配对安卓手机 → 私网 IP”，地址填 ${host}，端口填 ${port}，再输入这个验证码。剩余 ${left} 秒。`;};
   tick();if(!this.$('c-as-code').hidden)this.asTimer=setInterval(tick,1000);this.watchAirsim();}
  catch(error){if(epoch===this.epoch&&view===this.asView)msg.textContent=`没有打开配对窗口：${text(error?.message,120)||'需要所有者最近 5 分钟内登录'}`;}
  finally{if(this.asFlight===flight){this.asFlight=null;this.asBusy=false;}}}
 async loadClients(){const epoch=this.epoch,empty=this.$('c-sc-empty'),box=this.$('c-sc-list');try{const r=await this.request({op:'clients'});if(epoch!==this.epoch)return;const list=Array.isArray(record(r).clients)?r.clients.map(record).slice(0,32):[];box.replaceChildren();empty.hidden=list.length>0;empty.textContent='没有已授权设备';for(const c of list){const row=this.doc.createElement('div'),key=Array.isArray(c.public_key)?c.public_key.slice(0,6).map(v=>Number(v).toString(16).padStart(2,'0')).join(''):text(c.hex||c.id,12);for(const v of [text(c.kind,16)||'客户端',key?`${key}…`:'—',c.active===false?'已撤销':'有效',Array.isArray(c.scopes)?`${c.scopes.length} 项权限`:'—']){const s=this.doc.createElement('span');s.textContent=v;row.append(s);}box.append(row);}}catch{if(epoch===this.epoch){empty.hidden=false;empty.textContent='读取失败：需要所有者最近5分钟内登录';}}}
 setMode(mode){try{if(mode==='system')this.storage?.removeItem?.('rustsimhub-theme');else this.storage?.setItem('rustsimhub-theme',mode);}catch{}this.mode=mode;this.setTheme(mode==='system'?(this.matchDark()?'dark':'light'):mode);for(const b of this.doc.querySelectorAll('[data-mode]'))b.setAttribute('aria-pressed',String(b.dataset.mode===mode));}
 tick(){clearTimeout(this.ticker);const now=this.clock(),clock=this.sample?deviceClock(this.sample.sample_unix_seconds,this.receivedAt,now):null;
  if(this.active&&this.page==='security'&&!this.doc.hidden&&this.asReadsLeft>0&&now<this.asWatchUntil&&now>=this.asNextRead&&!this.asBusy){this.asReadsLeft--;this.asNextRead=now+5000;void this.airsim({op:'airsim_status'},false,true);}
  this.$('c-now').textContent=stamp(clock?clock.time:new Date(now));const sync=this.$('c-sync');sync.hidden=!clock||clock.synced;
  if(this.system&&count(this.system.uptime_seconds)!==null)this.$('c-uptime').textContent=uptimeText(this.system.uptime_seconds+Math.floor((now-this.systemAt)/1000));
  this.ticker=setTimeout(()=>this.tick(),1000-(now%1000)+20);}
 dots(id,n){[...this.$(id).children].forEach((d,i)=>d.classList.toggle('on',i<n));}
 bar(id,percent){this.$(id).style.width=`${percent===null?0:Math.max(0,Math.min(100,percent))}%`;}
 status(online){const o=this.$('c-online');o.classList.toggle('off',!online);o.lastChild.textContent=online?'设备在线':this.active?'暂时读不到设备':'未登录';this.$('c-user-name').textContent=this.active?'所有者':'登录';this.$('c-signin').hidden=this.active;this.$('c-mh-signin').hidden=this.active;this.$('login-section').hidden=this.active;this.$('c-ac-login').hidden=this.active;this.$('c-ac-out').hidden=!this.active;for(const id of ['c-sc-state','c-ac-state']){const p=this.$(id);p.textContent=this.active?'所有者':'未登录';p.classList.toggle('off',!this.active);}const m=this.$('c-mh-online');m.textContent=online?'在线':'离线';m.classList.toggle('off',!online);}
 reset(){this.epoch++;this.resetAirsim();this.callAction=null;this.callSelecting=false;this.selectedCalls.clear();this.$('c-call-operation').textContent='';this.mailAction=null;this.mailSelecting=false;this.selectedMail.clear();this.trash=[];this.trashLoad=null;this.clearMailDetail();this.$('c-mail-operation').textContent='';this.active=false;this.listRun=(this.listRun||0)+1;this.listLoading=null;this.callLoading=null;this.lineEpoch=null;this.haveInbox=false;this.haveCalls=false;this.mailVisibility=null;this.mailExhausted=true;this.callExhausted=true;for(const t of this.timers)clearInterval(t);this.timers=[];this.sample=null;this.system=null;this.prevTicks=null;this.temps=[];this.tempTimes=[];this.otps=[];this.events=[];this.history=[];this.seen=new Set();this.lastLine=null;
  this.status(false);for(const id of ['c-sms-list','c-call-list','c-otp-list','c-event-list','c-mail-list','c-ph-calls','c-nw-devices','c-sc-list','c-mc-sms','c-mc-otp','c-mc-call'])this.$(id).replaceChildren();this.mail=[];this.recentMail=[];this.older=[];this.mailTotal=0;this.callRows=[];this.recentCalls=[];this.olderCalls=[];this.callTotal=0;this.opened=null;this.mailCursor=null;this.callCursor=null;this.$('c-mc-more').hidden=true;this.$('c-mail-open').hidden=true;this.$('c-mail-blank').hidden=false;for(const id of ['c-mail-empty','c-ph-calls-empty','c-nw-devices-empty','c-mc-e-sms','c-mc-e-otp','c-mc-e-call']){this.$(id).hidden=false;this.$(id).textContent='登录后显示';}this.renderPhoneCalls();this.renderMobileCounts();for(const id of ['c-sms-empty','c-call-empty','c-otp-empty','c-event-empty']){this.$(id).hidden=false;this.$(id).textContent='登录后显示';}
  for(const id of ['c-cpu','c-mem','c-disk','c-temp','c-uptime','c-kernel','c-os','c-version','c-cores','c-mem-total','c-disk-total','c-wifi-name','c-wifi-ip','c-wifi-speed','c-cell-op','c-cell-ip','c-cell-usage','c-rsrp','c-rsrq','c-sinr',...VIEW_FIELDS])this.$(id).textContent='—';for(const id of ['c-hw-cpu-ring','c-hw-mem-ring','c-hw-disk-ring','c-hw-temp-ring'])this.ring(id,0);this.drawTemp();this.$('c-log-state').textContent='';this.renderLog();this.$('c-model').textContent='未登录';this.$('c-mh-model').textContent='未登录';this.$('c-mh-sub').textContent='登录后显示设备信息';this.$('c-sim').textContent='登录后显示设备信息';
  this.$('c-otp-state').textContent='';for(const id of ['c-cpu-bar','c-mem-bar','c-disk-bar','c-mh-cpu-b','c-mh-mem-b','c-mh-disk-b'])this.bar(id,0);this.$('c-mh-trend').setAttribute('d','');this.$('c-temp-line').setAttribute('d','');this.network(null);this.mailButtons();this.tick();}
 async signedIn(){this.active=true;this.status(true);this.log('登录成功','g');await this.refresh();if(this.smsFolder()==='trash')await this.loadTrash();const epoch=this.epoch;
  this.timers.push(setInterval(()=>{if(epoch===this.epoch&&!this.doc.hidden)void this.readSystem();},POLL_SYSTEM),setInterval(()=>{if(epoch===this.epoch&&!this.doc.hidden)void this.readLists();},POLL_LISTS));}
 async optional(command){try{return await this.request(command);}catch{return null;}}
 async refresh(){await this.readSystem();await this.readLists();}
 async readSystem(){const epoch=this.epoch,value=await this.optional({op:'system_status'});if(epoch!==this.epoch)return;if(value)this.renderSystem(value);this.status(!!value||!!this.sample);}
 renderSystem(value){const s=record(value),cpu=cpuPercent(this.prevTicks,s);this.prevTicks=s;this.system=s;this.systemAt=this.clock();
  this.$('c-model').textContent=text(s.model,48)||'型号未知';this.$('c-kernel').textContent=text(s.kernel,32)?`Linux ${text(s.kernel,32)}`:'—';this.$('c-os').textContent=text(s.os,40)||'—';this.$('c-version').textContent=text(s.firmware_version,24)?`RustSimHub v${text(s.firmware_version,24)}`:'—';
  this.$('c-cores').textContent=count(s.cpu_cores)?`${s.cpu_cores} 核`:'—';this.$('c-mem-total').textContent=bytesText(s.memory_total_bytes);this.$('c-disk-total').textContent=bytesText(s.storage_total_bytes);
  this.$('c-cpu').textContent=cpu===null?'采样中':`${cpu}%`;this.bar('c-cpu-bar',cpu);
  const memUsed=usedPercent(s.memory_total_bytes,s.memory_available_bytes);this.$('c-mem').textContent=memUsed===null?'—':`${bytesText(s.memory_total_bytes-s.memory_available_bytes)} / ${bytesText(s.memory_total_bytes)}  (${memUsed}%)`;this.bar('c-mem-bar',memUsed);
  const diskUsed=usedPercent(s.storage_total_bytes,s.storage_free_bytes);this.$('c-disk').textContent=diskUsed===null?'—':`${bytesText(s.storage_total_bytes-s.storage_free_bytes)} / ${bytesText(s.storage_total_bytes)}  (${diskUsed}%)`;this.bar('c-disk-bar',diskUsed);
  const temp=finite(s.temperature_c);this.$('c-temp').textContent=temp===null?'—':`${Math.round(temp)}°C`;if(temp!==null){this.temps.push(temp);this.temps=this.temps.slice(-SPARK);this.tempTimes.push(this.systemAt);this.tempTimes=this.tempTimes.slice(-SPARK);}this.drawTemp(temp);
  this.$('c-uptime').textContent=uptimeText(s.uptime_seconds);
  const os=text(s.os,40)||'—',kernel=text(s.kernel,32)?`Linux ${text(s.kernel,32)}`:'—',version=text(s.firmware_version,24)?`v${text(s.firmware_version,24)}`:'—',model=text(s.model,48)||'—';
  for(const [id,v] of [['c-hw-model',model],['c-hw-os',os],['c-hw-kernel',kernel],['c-hw-version',version],['c-hw-cores',count(s.cpu_cores)?`${s.cpu_cores} 核`:'—'],['c-ab-version',version],['c-ab-model',model],['c-ab-os',os],['c-ab-kernel',kernel],['c-hw-uptime',uptimeText(s.uptime_seconds)],['c-hw-cpu',cpu===null?'—':`${cpu}%`],['c-hw-load',finite(s.load_1m)===null?(count(s.cpu_cores)?`${s.cpu_cores} 核`:'—'):`负载 ${s.load_1m.toFixed(2)}`],['c-hw-mem',memUsed===null?'—':`${memUsed}%`],['c-hw-mem-d',memUsed===null?'—':`${bytesText(s.memory_total_bytes-s.memory_available_bytes)} / ${bytesText(s.memory_total_bytes)}`],['c-hw-disk',diskUsed===null?'—':`${diskUsed}%`],['c-hw-disk-d',diskUsed===null?'—':`${bytesText(s.storage_total_bytes-s.storage_free_bytes)} / ${bytesText(s.storage_total_bytes)}`],['c-hw-temp',temp===null?'—':`${Math.round(temp)}°C`]])this.$(id).textContent=v;
  this.$('c-mh-model').textContent=text(s.model,32)||'型号未知';this.$('c-mh-sub').textContent=[os,version].filter(v=>v!=='—').join(' / ')||'—';for(const [id,v,bar,p] of [['c-mh-cpu',cpu===null?'—':`${cpu}%`,'c-mh-cpu-b',cpu],['c-mh-mem',memUsed===null?'—':`${memUsed}%`,'c-mh-mem-b',memUsed],['c-mh-disk',diskUsed===null?'—':`${diskUsed}%`,'c-mh-disk-b',diskUsed]]){this.$(id).textContent=v;this.bar(bar,p);}this.$('c-mh-temp').textContent=temp===null?'—':`${Math.round(temp)}°C`;
  this.ring('c-hw-cpu-ring',cpu);this.ring('c-hw-mem-ring',memUsed);this.ring('c-hw-disk-ring',diskUsed);this.ring('c-hw-temp-ring',temp===null?null:temp/85*100);}
 ring(id,percent){this.$(id).setAttribute('stroke-dashoffset',String(326.7*(1-Math.max(0,Math.min(100,percent??0))/100)));}
 drawTemp(current=null){
  const temps=this.temps,last=temps.length-1,slider=this.$('c-hw-temp-point');
  this.$('c-hw-temp-current').textContent=current===null?'—':Number(current.toFixed(1))+' °C';
  this.$('c-hw-temp-sample').hidden=last<0;
  slider.max=String(Math.max(0,last));slider.value=slider.max;slider.disabled=last<=0;
  const low=last<0?0:Math.floor(Math.min(...temps))-1;
  const high=last<0?1:Math.ceil(Math.max(...temps))+1;
  for(const [id,value] of [['c-hw-temp-low',low],['c-hw-temp-mid',(low+high)/2],['c-hw-temp-high',high]])this.$(id).textContent=last<0?'':value+' °C';
  const line=x=>last>0?temps.map((value,i)=>(i?'L':'M')+x(i).toFixed(1)+' '+(26-(value-low)/(high-low)*24).toFixed(1)).join(' '):'';
  const small=line(i=>(i+SPARK-temps.length)/(SPARK-1)*150);
  this.$('c-temp-line').setAttribute('d',small);this.$('c-mh-trend').setAttribute('d',small);
  this.$('c-hw-trend').setAttribute('d',line(i=>i/last*150));this.showTempPoint();
 }
 showTempPoint(){
  const last=this.temps.length-1,point=Number(this.$('c-hw-temp-point').value),marker=this.$('c-hw-temp-marker');
  marker.setAttribute('visibility',last>0?'visible':'hidden');
  const x=String(last>0?point/last*150:0);
  marker.setAttribute('x1',x);marker.setAttribute('x2',x);
  const value=this.temps[point];
  this.$('c-hw-temp-reading').textContent=value===undefined?'':Number(value.toFixed(1))+' °C · '+new Date(this.tempTimes[point]).toLocaleTimeString();
 }

 // 网络页已经在轮询诊断状态；这里复用同一份采样，不额外读取。
 network(sample){if(!sample){this.sample=null;this.renderNetwork(networkView(null),null);this.tick();return;}const hadSample=!!this.sample;this.sample=sample;this.receivedAt=this.clock();const v=networkView(sample);this.renderNetwork(v,sample);if(!hadSample&&this.active)this.log(`网络已读取：${v.operator||'运营商未知'}`,'b');this.tick();}
 renderNetwork(v,sample){const pill=(id,on,yes,no)=>{const p=this.$(id);p.classList.toggle('off',!on);p.firstChild.textContent=on?yes:no;};
  pill('c-wifi-pill',v.wifi.online,'已连接','未连接');pill('c-data-pill',v.data.online,'已连接','未连接');this.$('c-wifi-icon').classList.toggle('off',!v.wifi.online);
  this.$('c-wifi-name').textContent=v.wifi.name||'—';this.$('c-wifi-ip').textContent=v.wifi.ip||'—';this.$('c-wifi-speed').textContent=v.wifi.online?'已联网':'未联网';this.$('c-wifi-state').textContent=v.wifi.online?'已连接':'未连接';
  this.$('c-cell-op').textContent=sample?[v.operator||'运营商未知',v.tech].filter(Boolean).join(' '):'—';this.$('c-cell-ip').textContent=v.data.ip||'—';
  const u=record(sample?.usage);this.$('c-cell-usage').textContent=!sample?'—':u.uncertain===true?'计量不确定':`↓ ${bytesText(u.rx_bytes)} ↑ ${bytesText(u.tx_bytes)} (本月)`;
  [...this.$('c-cell-bars').children].forEach((b,i)=>b.classList.toggle('on',i<v.signal.bars));this.$('c-cell-bars').setAttribute('aria-label',`信号 ${v.signal.bars}/4 格`);this.$('c-cell-state').textContent=`信号：${v.signal.label}`;
  const q=this.$('c-quality');q.textContent=v.signal.label;q.classList.toggle('off',v.signal.tone==='off');
  this.$('c-rsrp').textContent=v.rsrp===null?'—':`${String(v.rsrp).replace('-','−')} dBm`;this.$('c-rsrq').textContent=v.rsrq===null?'—':`${String(v.rsrq).replace('-','−')} dB`;this.$('c-sinr').textContent=v.sinr===null?'—':`${String(v.sinr).replace('-','−')} dB`;
  this.dots('c-rsrp-dots',rsrpLevel(v.rsrp));this.dots('c-rsrq-dots',rsrqLevel(v.rsrq));this.dots('c-sinr-dots',sinrLevel(v.sinr));
  const c=record(sample?.cellular),cell=record(c.serving_cell),unit=(n,u)=>n===null?'—':`${String(n).replace('-','−')} ${u}`,list=Array.isArray(c.configured_bands)?c.configured_bands.filter(b=>typeof b==='string').slice(0,8).join(', '):'';
  for(const [id,value] of [['c-sm-op',v.operator||'—'],['c-sm-tech',v.tech||'—'],['c-sm-reg',v.registration||'—'],['c-sm-rsrp',unit(v.rsrp,'dBm')],['c-sm-rsrq',unit(v.rsrq,'dB')],['c-sm-sinr',unit(v.sinr,'dB')],['c-sm-band',v.band||'—'],['c-sm-earfcn',Number.isInteger(cell.earfcn)&&cell.earfcn>=0?String(cell.earfcn):'—'],['c-sm-pci',cell.physical_cell_id==null?'—':text(String(cell.physical_cell_id),16)||'—'],['c-sm-cell',/^[0-9a-f]+$/i.test(String(cell.cell_id??''))?`0x${cell.cell_id}`:'—'],['c-sm-tac',/^[0-9a-f]+$/i.test(String(cell.tracking_area_code??''))?`0x${cell.tracking_area_code}`:'—'],['c-sm-modem',text(c.state,24)||'—'],['c-sm-plmn',text(c.operator_code,8)||'—'],['c-sm-pct',finite(c.signal_percent)===null?'—':`${c.signal_percent}%`],['c-sm-bands',list||'—'],['c-sm-data',sample?(v.data.online?'已连接':'未连接'):'—']])this.$(id).textContent=value;
  const on=(id,yes,y,n)=>{const p=this.$(id);p.textContent=yes?y:n;p.classList.toggle('off',!yes);};on('c-nw-wifi',v.wifi.online,'已连接','未连接');on('c-nw-data',v.data.online,'已连接','未连接');on('c-mh-wifi-p',v.wifi.online,'已连接','未连接');on('c-mh-sim-p',!!sample&&!!v.operator,'已连接','未就绪');
  const usage=record(sample?.usage),policy=record(sample?.policy),total=finite(usage.total_bytes),limit=policy.enabled===true?finite(policy.limit_bytes):null;
  for(const [id,value] of [['c-nw-wifi-name',v.wifi.name||'—'],['c-nw-wifi-ip',v.wifi.ip||'—'],['c-nw-op',[v.operator,v.tech].filter(Boolean).join(' ')||'—'],['c-nw-cell-ip',v.data.ip||'—'],['c-nw-total',usage.uncertain===true?'计量不确定':bytesText(total)],['c-nw-limit',limit?`/ ${bytesText(limit)}`:'未设限额'],['c-nw-rx',bytesText(usage.rx_bytes)],['c-nw-tx',bytesText(usage.tx_bytes)],['c-nw-rate-rx',rateText(sample?.rx_bytes_per_second)],['c-nw-rate-tx',rateText(sample?.tx_bytes_per_second)],['c-nw-period',text(usage.period_end,16)?`${usage.period_end} 重置`:''],['c-mh-wifi-n',v.wifi.name||'—'],['c-mh-wifi-s',v.wifi.online?'已联网':'未连接'],['c-mh-sim-n',[v.operator,v.tech].filter(Boolean).join(' ')||'—'],['c-mh-sim-s',`信号${v.signal.label}`]])this.$(id).textContent=value;
  this.bar('c-nw-bar',limit&&total!==null?total/limit*100:0);
  const table=this.$('c-nw-devices');table.replaceChildren();const devices=Array.isArray(sample?.devices)?sample.devices.map(record).slice(0,16):[];this.$('c-nw-devices-empty').hidden=devices.length>0;this.$('c-nw-devices-empty').textContent=sample?'没有网络接口':'登录后显示';
  for(const d of devices){const row=this.doc.createElement('div');for(const cell of [({cellular:'蜂窝数据',wifi:'Wi-Fi',ethernet:'有线 / USB',loopback:'本机'})[d.kind]||text(d.kind,16)||'未知',text(d.interface||d.device,24)||'—',text(String(d.state??''),32)||'—',firstAddress(d)||'—']){const c=this.doc.createElement('span');c.textContent=cell;row.append(c);}table.append(row);}
  const sq=this.$('c-sm-quality');sq.textContent=v.signal.label;sq.classList.toggle('off',v.signal.tone==='off');
  for(const [id,value] of [['c-sm-rsrp-m',scalePercent(v.rsrp,-120,-70)],['c-sm-rsrq-m',scalePercent(v.rsrq,-20,-5)],['c-sm-sinr-m',scalePercent(v.sinr,0,25)]]){const m=this.$(id);m.classList.toggle('unset',value===null);m.style.left=`${value??0}%`;}}
 async readListPage(command,valid){
  for(const delay of [0,180,500,1200]){
   if(delay)await this.readDelay(delay);
   if(!valid())return null;
   try{return await this.request(command);}
   catch(error){
    if(!transientListError(error)||delay===1200)throw error;
   }
  }
 }
 // Inbox and verification codes are projections of the same list. Queue only
 // the latest change of mode or explicit refresh behind the current read.
 async readLists(fresh=false){
  if(!this.active)return;
  const mode=this.page!=='comms'?'preview':(this.tab===0||this.tab===3)?'inbox':this.tab===2?'calls':'preview';
  if(this.listLoading&&!fresh&&this.listMode===mode)return this.listLoading;
  const previous=this.listLoading,run=++this.listRun,epoch=this.epoch;
  const valid=()=>this.active&&epoch===this.epoch&&run===this.listRun;
  const task=(async()=>{
   if(previous)await previous;
   if(valid())await this.readListsAt(mode,valid);
  })();
  this.listMode=mode;this.listLoading=task;
  try{await task;}finally{if(this.listLoading===task)this.listLoading=null;}
 }
 async readListsAt(mode,valid){
  const line=await this.optional({op:'line_status'});if(!valid())return;
  const identity=record(line).line;
  if(identity!==undefined&&this.lineEpoch!==null&&identity!==this.lineEpoch){this.forgetMail('线路已变化，重新读取');this.forgetCalls('线路已变化，重新读取');}
  if(identity!==undefined)this.lineEpoch=identity;
  const label=LINE[record(line).status]||(line?'SIM 状态未知':null);
  if(label){this.$('c-sim').textContent=label;this.$('c-sm-line').textContent=label;if(label!==this.lastLine){this.log(label,label==='SIM 就绪'?'g':'n');this.lastLine=label;}}
  const fullInbox=mode==='inbox',fullCalls=mode==='calls';
  let inbox=null,mailError=null,calls=null,callsError=null;
  try{inbox=await this.readListPage({op:'inbox_recent',before:null,limit:fullInbox?32:16},valid);}
  catch(error){mailError=error;}
  if(!valid())return;
  try{calls=await this.readListPage({op:'calls_recent',before:null,limit:fullCalls?32:16},valid);}
  catch(error){callsError=error;}
  if(!valid())return;
  const mailCursor=cursorOf(record(inbox).cursor),callCursor=cursorOf(record(calls).cursor);
  if(mailCursor&&callCursor&&mailCursor.db_epoch!==callCursor.db_epoch){this.forgetMail('记录代次已变化，请刷新');this.forgetCalls('记录代次已变化，请刷新');return;}
  if(inbox){
   if(this.mailCursor&&mailCursor&&this.mailCursor.db_epoch!==mailCursor.db_epoch){this.opened=null;this.$('c-mail-open').hidden=true;this.$('c-mail-blank').hidden=false;}
   if(fullInbox){
    if(!this.mail.length)this.$('c-mail-empty').textContent='正在读取全部短信…';
    try{
     const all=await this.readAllMail(inbox,valid);
     if(!all||!valid())return;
     this.older=[];this.recentMail=[];this.mailCursor=all.cursor;this.mailVisibility=all.visibility;
     this.mailExhausted=true;this.messages(all.rows,all.total,16);
     const notice=this.$('c-mail-operation');
     if(/^短信(?:未全部读取|读取暂忙)/.test(notice.textContent))notice.textContent='';
    }catch(error){
     if(!valid())return;
     if(transientListError(error))this.mailReadBusy();
     else{
      this.forgetMail('短信未全部读取，请刷新重试。');
      this.$('c-mail-operation').textContent='短信未全部读取，请刷新重试。';
     }
    }
   }else{
    this.older=[];this.recentMail=[];this.mailCursor=mailCursor;this.mailVisibility=visibilityOf(record(inbox).visibility_seq);
    this.mailExhausted=!Array.isArray(inbox.messages)||!inbox.messages.length||(count(inbox.total)===null&&inbox.messages.length<16);
    this.messages(record(inbox).messages,record(inbox).total);
   }
  }else if(transientListError(mailError))this.mailReadBusy();
  else this.forgetMail('短信暂不可用，请检查登录或权限');
  if(calls){
   this.olderCalls=[];this.recentCalls=[];this.callCursor=callCursor;
   this.callExhausted=!Array.isArray(calls.calls)||!calls.calls.length||(count(calls.total)===null&&calls.calls.length<(fullCalls?32:16));
   this.calls(record(calls).calls,record(calls).total);
   if(fullCalls)while(valid()&&this.canMoreCalls()&&!this.callLoading)await this.moreCalls(32);
  }
  else if(transientListError(callsError)){
   if(!this.haveCalls)this.$('c-ph-calls-empty').textContent='通话记录读取暂忙，请稍后刷新。';
  }else this.forgetCalls('通话记录暂不可用，请检查登录或权限');
 }
 mailReadBusy(){
  const message='短信读取暂忙，请稍后刷新。';
  this.$('c-mail-operation').textContent=message;
  if(!this.haveInbox)this.$('c-mail-empty').textContent=message;
 }
 forgetMail(message){this.older=[];this.recentMail=[];this.mailCursor=null;this.mailVisibility=null;this.mailExhausted=true;this.opened=null;this.haveInbox=false;this.applyMail(null);this.$('c-mail-open').hidden=true;this.$('c-mail-blank').hidden=false;for(const id of ['c-mail-empty','c-sms-empty','c-otp-empty','c-mc-e-sms','c-mc-e-otp']){this.$(id).hidden=false;this.$(id).textContent=message;}}
 forgetCalls(message){this.olderCalls=[];this.recentCalls=[];this.callCursor=null;this.callExhausted=true;this.haveCalls=false;this.applyCalls(null);for(const id of ['c-call-empty','c-ph-calls-empty','c-mc-e-call']){this.$(id).hidden=false;this.$(id).textContent=message;}}
 item(parent,{icon,tone,title,time,body,code}){const doc=this.doc,row=doc.createElement('div'),a=doc.createElement(icon?'span':'i'),main=doc.createElement('div'),top=doc.createElement('div'),t=doc.createElement('b'),when=doc.createElement('time'),p=doc.createElement('p');
  row.className='it';a.className=icon?`brand t-${tone}`:`pt t-${tone}`;if(icon)a.textContent=icon;top.className='it-l';t.textContent=title;when.textContent=time||'';p.textContent=body;top.append(t,when);main.className='it-m';main.append(top,p);row.append(a,main);
  if(code){const c=doc.createElement('button');c.type='button';c.className='codeb num';c.textContent=code;c.setAttribute('aria-label',`复制验证码 ${code}`);c.addEventListener('click',async()=>{try{await this.copy(code);c.textContent='已复制';c.classList.add('done');}catch{c.textContent='复制失败';}setTimeout(()=>{c.textContent=code;c.classList.remove('done');},1200);});p.textContent='';p.append('验证码 ',c,` · ${body}`);}
  parent.append(row);}
 mailView(m){const d=record(m.display),code=otpOf(d.preview),time=smsTime(d.timestamp,new Date(this.clock()));return {id:m.id,raw:text(d.sender,32),sender:maskNumber(d.sender)||'未知发件人',time:time.short,fullTime:time.full,rawTimestamp:time.raw,preview:text(d.preview,384)||(m.binary?'二进制短信':'（正文为空）'),code};}
 // 已加载过更早记录时，被新记录挤出最新一页的那几条并回“更早”，否则列表中间会缺一段。
 keepDropped(previous,next,older){const known=new Set(next.map(v=>v.id).filter(Boolean));return older.length?[...previous.filter(v=>v.id&&!known.has(v.id)),...older]:older;}
 messages(list,total,notificationLimit=16){this.haveInbox=true;const previous=this.recentMail;this.recentMail=Array.isArray(list)?list.map(record).filter(m=>typeof m.id==='string').map(m=>this.mailView(m)):[];this.older=this.keepDropped(previous,this.recentMail,this.older);
  for(const view of this.recentMail.slice(0,notificationLimit))if(this.seen.size&&!this.seen.has(view.id))this.log(`收到${view.code?'验证码':'新短信'}：${view.sender}`,'g');
  for(const view of this.recentMail)this.seen.add(view.id);this.applyMail(total);}
 applyMail(total){const known=new Set(this.recentMail.map(m=>m.id));this.older=this.older.filter(m=>!known.has(m.id));this.mail=uniqueRows([...this.recentMail,...this.older]);this.mailTotal=countOf(total,this.mail.length);const otps=this.mail.filter(m=>m.code),plain=this.mail.filter(m=>!m.code),more=this.canMoreMail();this.otps=otps;
  this.renderMail();this.renderComm();this.renderMobileCounts();const tones=['blue','purple','red','teal'];
  const box=this.$('c-otp-list');box.replaceChildren();this.$('c-otp-empty').hidden=otps.length>0;this.$('c-otp-empty').textContent=more?`最近 ${this.mail.length} 条里没有验证码`:'没有验证码短信';this.$('c-otp-clear').disabled=!otps.length;
  otps.slice(0,3).forEach((o,i)=>this.item(box,{icon:[...o.sender].find(ch=>/[^\d\s*+]/.test(ch))||'#',tone:tones[i%tones.length],title:o.sender,time:o.time,body:o.preview.replace(o.code,'').replace(/^[^\p{L}\p{N}]+/u,'').slice(0,40),code:o.code}));
  const sms=this.$('c-sms-list');sms.replaceChildren();this.$('c-sms-empty').hidden=plain.length>0;this.$('c-sms-empty').textContent='没有短信';
  plain.slice(0,3).forEach(m=>this.item(sms,{tone:'blue',title:m.sender,time:m.time,body:m.preview}));}
 calls(list,total){this.haveCalls=true;const previous=this.recentCalls;this.recentCalls=Array.isArray(list)?list.map(callView):[];this.olderCalls=this.keepDropped(previous,this.recentCalls,this.olderCalls);this.applyCalls(total);}
 applyCalls(total){const known=new Set(this.recentCalls.map(c=>c.id).filter(Boolean));this.olderCalls=this.olderCalls.filter(c=>!c.id||!known.has(c.id));const calls=uniqueRows([...this.recentCalls,...this.olderCalls]);this.callRows=calls;this.callTotal=countOf(total,calls.length);const box=this.$('c-call-list');box.replaceChildren();this.renderPhoneCalls();this.renderComm();this.renderMobileCounts();this.$('c-call-empty').hidden=calls.length>0;this.$('c-call-empty').textContent='没有通话记录';
  calls.slice(0,3).forEach(c=>this.item(box,{tone:c.live?'green':c.kind==='in'?'blue':'gray',title:c.number,body:c.label}));}
 saveLogLimit(){
  const capacity=Number(this.$('c-log-capacity').value),status=this.$('c-log-state');
  if(!Number.isInteger(capacity)||capacity<1||capacity>500){status.textContent='请输入 1 至 500 之间的整数。';return;}
  if(capacity<this.history.length&&!this.confirm('降低上限会移除较早的日志，继续保存？'))return;
  try{this.storage?.setItem('rustsimhub-log-limit',String(capacity));}catch{status.textContent='浏览器无法保存容量设置。';return;}
  this.logLimit=capacity;this.history=this.history.slice(0,capacity);this.renderLog();
  status.textContent='容量上限已保存。';
 }
 log(message,tone){
  this.history=[{at:new Date(this.clock()),message,tone},...this.history].slice(0,this.logLimit);this.renderLog();
 }
 renderLog(){
  this.events=this.history.slice(0,5);
  const render=(entry,editable)=>{
   const row=this.doc.createElement('li'),when=this.doc.createElement('time'),message=this.doc.createElement('span');
   row.className='t-'+entry.tone;when.textContent=stamp(entry.at);message.textContent=entry.message;row.append(when,message);
   if(editable){
    const button=this.doc.createElement('button');button.type='button';button.className='pg-link';button.textContent='删除';
    button.setAttribute('aria-label','删除日志 '+entry.message);
    button.addEventListener('click',()=>{this.history=this.history.filter(item=>item!==entry);this.renderLog();});row.append(button);
   }
   return row;
  };
  this.$('c-event-list').replaceChildren(...this.events.map(entry=>render(entry,false)));
  this.$('c-event-empty').hidden=this.events.length>0;
  this.$('c-ev-all').replaceChildren(...this.history.map(entry=>render(entry,true)));
  this.$('c-ev-all-empty').hidden=this.history.length>0;
  this.$('c-ev-all-empty').textContent=this.active?'暂无系统日志':'登录后显示';
  this.$('c-log-clear').disabled=!this.history.length;
 }
 // 逐条移到本地回收站；任何一条失败即停止并保留剩余，不自动重试。
 async clearOtps(){if(this.clearing)return;const epoch=this.epoch,ids=this.otps.map(o=>o.id),state=this.$('c-otp-state');this.clearing=true;let done=0;
  try{for(const id of ids){const r=await this.request({op:'delete_sms',sms_id:id});if(epoch!==this.epoch)return;if(record(r).sms_id!==id||record(r).deleted!==true)throw Error('设备响应不匹配');done++;}state.textContent=`已移到回收站 ${done} 条，所有者可恢复`;this.log(`清理验证码 ${done} 条`,'b');}
  catch(error){if(epoch!==this.epoch)return;state.textContent=`已移到回收站 ${done} 条，其余未处理：${error?.message||'需要所有者最近5分钟内登录'}；未自动重试`;}
  finally{this.clearing=false;if(epoch===this.epoch)await this.readLists(true);}}
}
