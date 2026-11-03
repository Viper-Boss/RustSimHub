// SPDX-License-Identifier: AGPL-3.0-only
// 首页概览：以只读展示为主。唯一的写操作是用户确认后的“清理验证码”，逐条调用原有的移到回收站接口（Core 仍要求所有者最近登录），
// 其余变更都跳转到原业务区（带最近登录、确认和回执）。
const record=v=>v!==null&&typeof v==='object'&&!Array.isArray(v)?v:{};
const text=(v,max=64)=>typeof v==='string'&&v.trim()?v.trim().slice(0,max):null;
const finite=v=>typeof v==='number'&&Number.isFinite(v)?v:null;
const p2=n=>String(n).padStart(2,'0');
export function greeting(hour){return hour<5?'夜深了':hour<11?'早上好':hour<13?'中午好':hour<18?'下午好':'晚上好';}
export function dateText(d){return `${d.getMonth()+1}月${d.getDate()}日 星期${'日一二三四五六'[d.getDay()]}`;}
export function timeText(d){return `${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}`;}
export function bytesText(n){n=finite(n);if(n===null||n<0)return '—';if(n<1e6)return `${(n/1e3).toFixed(0)} KB`;if(n<1e9)return `${(n/1e6).toFixed(1)} MB`;return `${(n/1e9).toFixed(2)} GB`;}
export function rateText(n){n=finite(n);if(n===null||n<0)return '—';const bits=n*8;return bits<1e6?`${(bits/1e3).toFixed(0)} Kbps`:`${(bits/1e6).toFixed(1)} Mbps`;}
// 信号格：只用设备给出的0–100百分比，缺失时为0格并显示“未知”。
export function signalView(percent){const p=finite(percent);if(p===null||p<0||p>100)return {bars:0,label:'信号未知'};const bars=p>=75?4:p>=50?3:p>=25?2:p>0?1:0;return {bars,label:['无信号','信号弱','信号一般','信号良好','信号极好'][bars]};}
export function cellularView(sample){
 const s=record(sample),c=record(s.cellular),cell=record(c.serving_cell),band=record(cell.channel_derived).band;
 const tech=Array.isArray(c.access_technologies)?c.access_technologies.filter(t=>typeof t==='string').slice(0,4):[];
 const devices=Array.isArray(s.devices)?s.devices.map(record):[];const connected=kind=>devices.some(d=>d.kind===kind&&String(d.state).startsWith('100 '));
 return {operator:text(c.operator,32),registration:({home:'已注册',roaming:'漫游',searching:'正在搜索网络',denied:'注册被拒绝'})[c.registration]||null,
  tech:tech.map(t=>t.toUpperCase()==='LTE'?'4G LTE':t.toUpperCase()).join(' / ')||null,band:Number.isInteger(band)&&band>0?`Band ${band}`:null,
  signal:signalView(c.signal_percent),rsrp:finite(cell.rsrp),sinr:finite(cell.sinr),data:connected('cellular'),wifi:connected('wifi')};
}
// 设备时间：采样时间加上收到后经过的本机时间；与本机差超过60秒提示未同步。
export function deviceClock(sampleSeconds,receivedAt,now){const t=finite(sampleSeconds);if(t===null)return null;const device=t*1000+(now-receivedAt);return {device:new Date(device),synced:Math.abs(t*1000-receivedAt)<=60000};}
// 验证码识别：正文含验证码类关键词时，取第一个独立的4–8位数字。只用于展示和复制。
export function otpOf(preview){const s=text(preview,384);if(!s||!/验证码|校验码|动态码|动态密码|确认码|verification code|\bcode\b/i.test(s))return null;const m=s.match(/(?<![0-9])[0-9]{4,8}(?![0-9])/);return m?m[0]:null;}
export function senderName(sender){const s=text(sender,40)||'未知发件人';const m=s.match(/^(?:\+?86)?(1[3-9][0-9])([0-9]{4})([0-9]{4})$/);return m?`${m[1]} **** ${m[3]}`:s;}
export function callView(call){const c=record(call);const incoming=c.direction===1;const missed=incoming&&['terminated','missed'].includes(c.state)&&c.media_state!=='active';
 return {number:senderName(c.number),kind:missed?'missed':incoming?'in':'out',label:missed?'未接来电':incoming?'呼入':'呼出',state:text(c.state,32)};}
const STATES={ready:'SIM 就绪',locked:'SIM 已锁定',absent:'未插 SIM',unavailable:'SIM 不可用'};
export class HomeDashboard{
 constructor({document,request,open,clock=()=>Date.now(),copy}){this.doc=document;this.request=request;this.open=open;this.clock=clock;this.copy=copy;this.epoch=0;this.sample=null;this.receivedAt=0;this.ticker=null;this.otps=[];
  this.$=id=>document.getElementById(id);
  for(const [id,target] of [['home-dial','phone'],['home-sms','sms'],['home-calls','phone'],['home-sms-all','sms'],['home-calls-all','phone'],['home-wifi','network'],['home-forwarding','forwarding'],['home-nkn','support'],['home-nas','nas'],['home-airsim','clients'],['home-diagnostics','support'],['home-settings','account'],['home-data','network']])this.$(id)?.addEventListener('click',()=>this.open(target));
  this.$('home-otp')?.addEventListener('click',()=>this.$('home-otp-section')?.scrollIntoView({behavior:'smooth',block:'start'}));
  const dialog=this.$('home-otp-dialog');this.$('home-otp-clear')?.addEventListener('click',()=>{if(!this.otps.length||this.clearing)return;this.$('home-otp-dialog-title').textContent=`清理 ${this.otps.length} 条验证码短信？`;dialog.showModal();});
  dialog?.addEventListener('close',()=>{if(dialog.returnValue==='clear')void this.clearOtps();});
  this.tick();}
 // 逐条移到本地回收站；任何一条失败即停止并保留剩余，不自动重试。
 async clearOtps(){if(this.clearing)return;const epoch=this.epoch,ids=this.otps.map(o=>o.id);this.clearing=true;let done=0;const state=this.$('home-otp-state');
  try{for(const id of ids){const r=await this.request({op:'delete_sms',sms_id:id});if(epoch!==this.epoch)return;if(record(r).sms_id!==id||record(r).deleted!==true)throw Error('设备响应不匹配');done++;}
   state.textContent=`已移到回收站 ${done} 条，所有者可恢复`;}
  catch(error){if(epoch!==this.epoch)return;state.textContent=`已移到回收站 ${done} 条，其余未处理：${error?.message||'需要所有者最近5分钟内登录'}；未自动重试`;}
  finally{this.clearing=false;if(epoch===this.epoch)await this.refresh();}}
 tick(){clearTimeout(this.ticker);const now=new Date(this.clock());this.$('home-greet').textContent=greeting(now.getHours());this.$('home-date').textContent=dateText(now);
  const clock=this.sample?deviceClock(this.sample.sample_unix_seconds,this.receivedAt,this.clock()):null;const sync=this.$('home-sync');
  this.$('home-device-time').textContent=clock?timeText(clock.device):'--:--:--';sync.textContent=clock?(clock.synced?'已同步':'时间未同步'):'未读取';sync.className=`sync ${clock?(clock.synced?'ok':'warn'):'idle'}`;
  this.ticker=setTimeout(()=>this.tick(),1000-(this.clock()%1000)+20);}
 reset(){this.epoch++;this.sample=null;this.otps=[];this.$('home').dataset.state='signed-out';this.$('home-operator').textContent='未登录';this.$('home-network').textContent='登录后显示信号、短信和通话';
  for(const id of ['home-sms-list','home-calls-list','home-otp-list'])this.$(id).replaceChildren();for(const id of ['home-sms-empty','home-calls-empty','home-otp-empty']){this.$(id).hidden=false;this.$(id).textContent='登录后显示';}this.signal(signalView(null));for(const id of ['home-rsrp','home-sinr','home-data-state','home-sim','home-usage','home-usage-limit','home-rx-rate','home-tx-rate','home-forwarding-state','home-nkn-state','home-wifi-state'])this.$(id).textContent='—';this.ring(0);this.tick();}
 signal(view){const bars=this.$('home-bars');[...bars.children].forEach((bar,i)=>bar.classList.toggle('on',i<view.bars));bars.setAttribute('aria-label',`信号 ${view.bars}/4 格`);this.$('home-quality').textContent=view.label;}
 ring(fraction){const f=Math.max(0,Math.min(1,finite(fraction)??0));this.$('home-ring-value').setAttribute('stroke-dashoffset',String(301.6*(1-f)));}
 // 网络页面已在轮询诊断状态；首页复用同一份采样，不额外读取。
 network(sample){if(!sample){this.sample=null;this.tick();return;}this.sample=sample;this.receivedAt=this.clock();const v=cellularView(sample);
  this.$('home-operator').textContent=v.operator||'运营商未知';this.$('home-network').textContent=[v.tech,v.band,v.registration].filter(Boolean).join(' · ')||'网络信息暂未提供';this.signal(v.signal);
  this.$('home-rsrp').textContent=v.rsrp===null?'—':`${v.rsrp} dBm`;this.$('home-sinr').textContent=v.sinr===null?'—':`${v.sinr} dB`;this.$('home-data-state').textContent=v.data?'已连接':'关闭';this.$('home-wifi-state').textContent=v.wifi?'已连接':'未连接';
  const u=record(sample.usage),p=record(sample.policy),total=finite(u.total_bytes),limit=p.enabled===true?finite(p.limit_bytes):null;
  this.$('home-usage').textContent=u.uncertain===true?'计量不确定':bytesText(total);this.$('home-usage-limit').textContent=limit?`/ ${bytesText(limit)}`:'未设限额';this.ring(limit&&total!==null?total/limit:0);
  this.$('home-rx-rate').textContent=rateText(sample.rx_bytes_per_second);this.$('home-tx-rate').textContent=rateText(sample.tx_bytes_per_second);this.tick();}
 async optional(command){try{return await this.request(command);}catch{return null;}}
 async refresh(){const epoch=++this.epoch;this.$('home').dataset.state='loading';
  const line=await this.optional({op:'line_status'});if(epoch!==this.epoch)return;this.$('home-sim').textContent=STATES[record(line).status]||'SIM 状态未知';
  const [inbox,calls,forwarding,nkn]=[await this.optional({op:'inbox',after:null,limit:16}),await this.optional({op:'calls',after:null,limit:16}),await this.optional({op:'forwarding_status'}),await this.optional({op:'nkn_status'})];if(epoch!==this.epoch)return;
  this.messages(record(inbox).messages);this.calls(record(calls).calls);
  this.$('home-sms-empty').textContent=inbox?'没有短信':'短信暂不可用';this.$('home-otp-empty').textContent=inbox?'没有验证码短信':'短信暂不可用';this.$('home-calls-empty').textContent=calls?'没有通话记录':'通话记录暂不可用';
  const onOff=v=>record(v).enabled===true?'已开启':record(v).enabled===false?'已关闭':'点击查看';this.$('home-forwarding-state').textContent=onOff(forwarding);this.$('home-nkn-state').textContent=onOff(nkn);
  this.$('home').dataset.state='ready';}
 row(parent,{avatar,tone,title,time,body,onClick}){const doc=this.doc,b=doc.createElement('button'),a=doc.createElement('span'),main=doc.createElement('div'),top=doc.createElement('div'),t=doc.createElement('b'),when=doc.createElement('time'),p=doc.createElement('p');
  b.type='button';b.className='row';a.className=`av g-${tone}`;a.textContent=avatar;main.className='row-body';top.className='row-title';t.textContent=title;when.textContent=time||'';p.textContent=body;top.append(t,when);main.append(top,p);b.append(a,main);if(onClick)b.addEventListener('click',onClick);parent.append(b);}
 messages(list){const all=Array.isArray(list)?list.map(record).filter(m=>typeof m.id==='string'):[];const otps=[],plain=[];
  for(const m of all){const d=record(m.display),code=otpOf(d.preview);(code?otps:plain).push({id:m.id,sender:senderName(d.sender),time:text(d.timestamp,32),preview:text(d.preview,384)||(m.binary?'二进制短信':'（正文为空）'),code});}
  this.otps=otps;const box=this.$('home-otp-list');box.replaceChildren();this.$('home-otp-empty').hidden=otps.length>0;
  for(const o of otps.slice(0,6)){const doc=this.doc,card=doc.createElement('div'),src=doc.createElement('div'),name=doc.createElement('b'),when=doc.createElement('time'),code=doc.createElement('div'),foot=doc.createElement('div'),btn=doc.createElement('button');
   card.className='otp-item';src.className='otp-src';name.textContent=o.sender;when.textContent=o.time||'';src.append(name,when);code.className='otp-code num';code.textContent=o.code;foot.className='otp-foot';btn.type='button';btn.className='copy';btn.textContent='复制';
   btn.addEventListener('click',async()=>{try{await this.copy(o.code);btn.textContent='已复制';btn.classList.add('done');}catch{btn.textContent='复制失败';}setTimeout(()=>{btn.textContent='复制';btn.classList.remove('done');},1500);});
   foot.append(btn);card.append(src,code,foot);box.append(card);}
  if(otps[0])box.firstChild.classList.add('fresh');
  const sms=this.$('home-sms-list');sms.replaceChildren();this.$('home-sms-empty').hidden=plain.length>0;const tones=['blue','purple','orange','teal','pink','green'];
  plain.slice(0,3).forEach((m,i)=>this.row(sms,{avatar:[...m.sender].find(ch=>/[^\d\s*+]/.test(ch))||'信',tone:tones[i%tones.length],title:m.sender,time:m.time,body:m.preview,onClick:()=>this.open('sms')}));}
 calls(list){const box=this.$('home-calls-list');box.replaceChildren();const calls=Array.isArray(list)?list.map(callView):[];this.$('home-calls-empty').hidden=calls.length>0;
  for(const c of calls.slice(0,3))this.row(box,{avatar:c.kind==='out'?'↗':'↙',tone:c.kind==='missed'?'pink':c.kind==='out'?'green':'blue',title:c.number,time:'',body:c.label,onClick:()=>this.open('phone')});
  [...box.children].forEach((b,i)=>b.classList.toggle('missed',calls[i]?.kind==='missed'));}
}
