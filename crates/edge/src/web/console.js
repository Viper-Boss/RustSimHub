// SPDX-License-Identifier: AGPL-3.0-only
// 控制台外壳与仪表盘。仪表盘只读：唯一的写操作是用户确认后的“清理验证码”，逐条调用原有的移到回收站接口，
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
export function callView(call){const c=record(call),incoming=c.direction===1;return {number:maskNumber(c.number)||'号码未知',kind:incoming?'in':'out',label:`${incoming?'来电':'去电'} · ${CALL_STATES[c.state]||'状态未知'}`,live:['ringing_in','active','held','waiting','dialing','ringing_out'].includes(c.state)};}
export function shortTime(value,now=new Date()){const s=text(value,40);if(!s)return '';const d=new Date(s);if(Number.isNaN(d.getTime()))return s.slice(0,16);return d.toDateString()===now.toDateString()?`${p2(d.getHours())}:${p2(d.getMinutes())}`:`${d.getMonth()+1}月${d.getDate()}日`;}
const LINE={ready:'SIM 就绪',locked:'SIM 已锁定',absent:'未插 SIM',unavailable:'SIM 不可用'};
// 导航目标：legacy 中要显示的区块。嵌套目标会连同外层一起打开。
export const PAGES={dashboard:{title:'RustSimHub 控制台'},hardware:{title:'硬件状态',show:['debug-panel']},network:{title:'网络管理',show:['network-panel','ap-wifi-panel']},sim:{title:'SIM与调制解调器',show:['debug-panel']},comms:{title:'通话短信',show:['phone-panel','sms-panel']},gnss:{title:'GNSS定位',empty:'设备暂未提供定位接口，这里先不显示数据。'},ble:{title:'蓝牙配网',empty:'蓝牙配网在手机配置工具中完成，网页暂无蓝牙开关。'},security:{title:'安全策略',show:['ap-identity-panel']},logs:{title:'系统日志',show:['test-center','support-panel']},settings:{title:'设置',show:['account-panel']},forwarding:{title:'短信转发',show:['forwarding-panel']},nas:{title:'NasSimHub 连接',show:['nas-bridge-panel']},clients:{title:'手机与设备授权',show:['clients-section']},nkn:{title:'NKN 远程',show:['support-panel']},login:{title:'登录',show:['account-panel','ap-identity-panel']}};
const SPARK=30,POLL_SYSTEM=15000,POLL_LISTS=60000;
export class ConsoleDashboard{
 constructor({document,request,copy,storage=null,clock=()=>Date.now(),matchDark=()=>false,onLeave=()=>{}}){this.doc=document;this.request=request;this.copy=copy;this.storage=storage;this.clock=clock;this.onLeave=onLeave;this.epoch=0;this.active=false;this.sample=null;this.receivedAt=0;this.system=null;this.systemAt=0;this.temps=[];this.otps=[];this.events=[];this.seen=new Set();this.lastLine=null;this.page='dashboard';this.timers=[];
  this.$=id=>document.getElementById(id);
  let theme=null;try{theme=storage?.getItem('rustsimhub-theme');}catch{}
  this.setTheme(theme==='dark'||theme==='light'?theme:matchDark()?'dark':'light');
  this.$('c-theme').addEventListener('click',()=>{const next=this.doc.documentElement.dataset.theme==='dark'?'light':'dark';this.setTheme(next);try{this.storage?.setItem('rustsimhub-theme',next);}catch{}});
  for(const b of document.querySelectorAll('[data-go]'))b.addEventListener('click',()=>this.go(b.dataset.go));
  this.$('c-user').addEventListener('click',()=>this.go(this.active?'settings':'login'));
  this.$('c-bell').addEventListener('click',()=>{this.go('dashboard');this.$('c-events').scrollIntoView?.({behavior:'smooth',block:'center'});});
  const dialog=this.$('c-otp-dialog');this.$('c-otp-clear').addEventListener('click',()=>{if(!this.otps.length||this.clearing)return;this.$('c-otp-dialog-title').textContent=`清理 ${this.otps.length} 条验证码短信？`;dialog.showModal();});
  dialog.addEventListener('close',()=>{const clear=dialog.returnValue==='clear';dialog.returnValue='';if(clear)void this.clearOtps();});
  this.go('dashboard');this.reset();this.tick();}
 setTheme(theme){this.doc.documentElement.dataset.theme=theme;this.$('c-theme-icon').setAttribute('href',theme==='dark'?'#i-sun':'#i-moon');}
 go(target){const page=PAGES[target];if(!page)return;if(target!==this.page)this.onLeave();this.page=target;
  for(const b of this.doc.querySelectorAll('.nav[data-go]'))if(b.dataset.go===target)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');
  this.$('c-title').textContent=page.title;const dashboard=target==='dashboard';this.$('c-subtitle').hidden=!dashboard;this.$('c-dash').hidden=!dashboard;const legacy=this.$('c-legacy');legacy.hidden=dashboard;
  if(dashboard)return;const shown=new Set(),targets=(page.show||[]).map(id=>this.$(id)).filter(Boolean);
  for(const node of targets){for(let n=node;n&&n!==legacy;n=n.parentElement){if(n.parentElement===legacy)shown.add(n);if(n.tagName==='DETAILS')n.open=true;}}
  for(const child of legacy.children)child.classList.toggle('page-off',!shown.has(child)&&child.id!=='notice');
  const empty=this.$('c-empty');empty.classList.toggle('page-off',!page.empty);empty.textContent=page.empty||'';
  targets[0]?.scrollIntoView?.({block:'start'});}
 tick(){clearTimeout(this.ticker);const now=this.clock(),clock=this.sample?deviceClock(this.sample.sample_unix_seconds,this.receivedAt,now):null;
  this.$('c-now').textContent=stamp(clock?clock.time:new Date(now));const sync=this.$('c-sync');sync.hidden=!clock||clock.synced;
  if(this.system&&count(this.system.uptime_seconds)!==null)this.$('c-uptime').textContent=uptimeText(this.system.uptime_seconds+Math.floor((now-this.systemAt)/1000));
  this.ticker=setTimeout(()=>this.tick(),1000-(now%1000)+20);}
 dots(id,n){[...this.$(id).children].forEach((d,i)=>d.classList.toggle('on',i<n));}
 bar(id,percent){this.$(id).style.width=`${percent===null?0:Math.max(0,Math.min(100,percent))}%`;}
 status(online){const o=this.$('c-online');o.classList.toggle('off',!online);o.lastChild.textContent=online?'设备在线':this.active?'暂时读不到设备':'未登录';this.$('c-user-name').textContent=this.active?'所有者':'登录';}
 reset(){this.epoch++;this.active=false;for(const t of this.timers)clearInterval(t);this.timers=[];this.sample=null;this.system=null;this.prevTicks=null;this.temps=[];this.otps=[];this.events=[];this.seen=new Set();this.lastLine=null;
  this.status(false);for(const id of ['c-sms-list','c-call-list','c-otp-list','c-event-list'])this.$(id).replaceChildren();for(const id of ['c-sms-empty','c-call-empty','c-otp-empty','c-event-empty']){this.$(id).hidden=false;this.$(id).textContent='登录后显示';}
  for(const id of ['c-cpu','c-mem','c-disk','c-temp','c-uptime','c-kernel','c-os','c-version','c-cores','c-mem-total','c-disk-total','c-wifi-name','c-wifi-ip','c-wifi-speed','c-cell-op','c-cell-ip','c-cell-usage','c-rsrp','c-rsrq','c-sinr'])this.$(id).textContent='—';this.$('c-model').textContent='未登录';this.$('c-sim').textContent='登录后显示设备信息';
  this.$('c-otp-state').textContent='';for(const id of ['c-cpu-bar','c-mem-bar','c-disk-bar'])this.bar(id,0);this.$('c-temp-line').setAttribute('d','');this.network(null);this.tick();}
 async signedIn(){this.active=true;this.status(true);this.log('登录成功','g');await this.refresh();const epoch=this.epoch;
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
  const temp=finite(s.temperature_c);this.$('c-temp').textContent=temp===null?'—':`${Math.round(temp)}°C`;if(temp!==null){this.temps.push(temp);this.temps=this.temps.slice(-SPARK);this.drawTemp();}
  this.$('c-uptime').textContent=uptimeText(s.uptime_seconds);}
 drawTemp(){const t=this.temps;if(t.length<2){this.$('c-temp-line').setAttribute('d','');return;}const lo=Math.min(...t)-1,hi=Math.max(...t)+1,x=i=>i/(SPARK-1)*150,y=v=>26-(v-lo)/(hi-lo)*24;this.$('c-temp-line').setAttribute('d',t.map((v,i)=>`${i?'L':'M'}${x(i+SPARK-t.length).toFixed(1)} ${y(v).toFixed(1)}`).join(' '));}
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
  this.dots('c-rsrp-dots',rsrpLevel(v.rsrp));this.dots('c-rsrq-dots',rsrqLevel(v.rsrq));this.dots('c-sinr-dots',sinrLevel(v.sinr));}
 async readLists(){const epoch=this.epoch,line=await this.optional({op:'line_status'});if(epoch!==this.epoch)return;const label=LINE[record(line).status]||(line?'SIM 状态未知':null);if(label){this.$('c-sim').textContent=label;if(label!==this.lastLine){this.log(label,label==='SIM 就绪'?'g':'n');this.lastLine=label;}}
  const inbox=await this.optional({op:'inbox',after:null,limit:16});if(epoch!==this.epoch)return;const calls=await this.optional({op:'calls',after:null,limit:16});if(epoch!==this.epoch)return;
  // 读失败时保留上一次的内容；从未读到时才说暂不可用，不误报为空。
  if(inbox)this.messages(record(inbox).messages);else if(!this.haveInbox){this.$('c-sms-empty').textContent='短信暂不可用';this.$('c-otp-empty').textContent='短信暂不可用';}
  if(calls)this.calls(record(calls).calls);else if(!this.haveCalls)this.$('c-call-empty').textContent='通话记录暂不可用';}
 item(parent,{icon,tone,title,time,body,code}){const doc=this.doc,row=doc.createElement('div'),a=doc.createElement(icon?'span':'i'),main=doc.createElement('div'),top=doc.createElement('div'),t=doc.createElement('b'),when=doc.createElement('time'),p=doc.createElement('p');
  row.className='it';a.className=icon?`brand t-${tone}`:`pt t-${tone}`;if(icon)a.textContent=icon;top.className='it-l';t.textContent=title;when.textContent=time||'';p.textContent=body;top.append(t,when);main.className='it-m';main.append(top,p);row.append(a,main);
  if(code){const c=doc.createElement('button');c.type='button';c.className='codeb num';c.textContent=code;c.setAttribute('aria-label',`复制验证码 ${code}`);c.addEventListener('click',async()=>{try{await this.copy(code);c.textContent='已复制';c.classList.add('done');}catch{c.textContent='复制失败';}setTimeout(()=>{c.textContent=code;c.classList.remove('done');},1200);});p.textContent='';p.append('验证码 ',c,` · ${body}`);}
  parent.append(row);}
 messages(list){this.haveInbox=true;const all=Array.isArray(list)?list.map(record).filter(m=>typeof m.id==='string'):[],otps=[],plain=[];
  for(const m of all){const d=record(m.display),code=otpOf(d.preview),view={id:m.id,sender:maskNumber(d.sender)||'未知发件人',time:shortTime(d.timestamp),preview:text(d.preview,384)||(m.binary?'二进制短信':'（正文为空）'),code};(code?otps:plain).push(view);if(this.seen.size&&!this.seen.has(m.id))this.log(`收到${code?'验证码':'新短信'}：${view.sender}`,'g');}
  this.seen=new Set(all.map(m=>m.id));this.otps=otps;const tones=['blue','purple','red','teal'];
  const box=this.$('c-otp-list');box.replaceChildren();this.$('c-otp-empty').hidden=otps.length>0;this.$('c-otp-empty').textContent='没有验证码短信';this.$('c-otp-clear').disabled=!otps.length;
  otps.slice(0,3).forEach((o,i)=>this.item(box,{icon:[...o.sender].find(ch=>/[^\d\s*+]/.test(ch))||'#',tone:tones[i%tones.length],title:o.sender,time:o.time,body:o.preview.replace(o.code,'').replace(/^[^\p{L}\p{N}]+/u,'').slice(0,40),code:o.code}));
  const sms=this.$('c-sms-list');sms.replaceChildren();this.$('c-sms-empty').hidden=plain.length>0;this.$('c-sms-empty').textContent='没有短信';
  plain.slice(0,3).forEach(m=>this.item(sms,{tone:'blue',title:m.sender,time:m.time,body:m.preview}));}
 calls(list){this.haveCalls=true;const box=this.$('c-call-list');box.replaceChildren();const calls=Array.isArray(list)?list.map(callView):[];this.$('c-call-empty').hidden=calls.length>0;this.$('c-call-empty').textContent='没有通话记录';
  calls.slice(0,3).forEach(c=>this.item(box,{tone:c.live?'green':c.kind==='in'?'blue':'gray',title:c.number,body:c.label}));}
 log(message,tone){this.events.unshift({at:new Date(this.clock()),message,tone});this.events=this.events.slice(0,5);const box=this.$('c-event-list');box.replaceChildren();this.$('c-event-empty').hidden=true;
  for(const e of this.events){const li=this.doc.createElement('li'),t=this.doc.createElement('time'),s=this.doc.createElement('span');li.className=`t-${e.tone}`;t.textContent=stamp(e.at);s.textContent=e.message;li.append(t,s);box.append(li);}}
 // 逐条移到本地回收站；任何一条失败即停止并保留剩余，不自动重试。
 async clearOtps(){if(this.clearing)return;const epoch=this.epoch,ids=this.otps.map(o=>o.id),state=this.$('c-otp-state');this.clearing=true;let done=0;
  try{for(const id of ids){const r=await this.request({op:'delete_sms',sms_id:id});if(epoch!==this.epoch)return;if(record(r).sms_id!==id||record(r).deleted!==true)throw Error('设备响应不匹配');done++;}state.textContent=`已移到回收站 ${done} 条，所有者可恢复`;this.log(`清理验证码 ${done} 条`,'b');}
  catch(error){if(epoch!==this.epoch)return;state.textContent=`已移到回收站 ${done} 条，其余未处理：${error?.message||'需要所有者最近5分钟内登录'}；未自动重试`;}
  finally{this.clearing=false;if(epoch===this.epoch)await this.readLists();}}
}
