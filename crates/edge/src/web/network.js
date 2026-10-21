// SPDX-License-Identifier: AGPL-3.0-only
export function bytes(value){if(!Number.isFinite(value)||value<0)return '暂不可用';const units=['B','KB','MB','GB','TB'];let i=0;while(value>=1000&&i<units.length-1){value/=1000;i++;}return `${value.toFixed(i?2:0)} ${units[i]}`;}
export function rate(value){return Number.isFinite(value)&&value>=0?`${(value*8/1e6).toFixed(2)} Mbps（${bytes(value)}/s）`:'等待下一次采样';}
export function policyInput(enabled,gb,day){const number=Number(gb),reset=Number(day);if(!Number.isFinite(number)||number<=0||number>100000||!Number.isInteger(reset)||reset<1||reset>31)throw Error('请填写有效额度和 1–31 日的重置日');const limit=Math.round(number*1e9);if(!Number.isSafeInteger(limit)||limit<1)throw Error('流量额度无效');return {enabled:!!enabled,limit_bytes:limit,reset_day:reset};}
const text=value=>value===null||value===undefined||value===''?'暂不可用':String(value);
const join=value=>Array.isArray(value)&&value.length?value.map(text).join('、'):'暂无';
export function radioRows(cellular={}){
 const c=cellular.serving_cell||{},f=c.channel_derived||{};
 return [['运营商',cellular.operator],['注册状态',({home:'本地注册',roaming:'漫游',searching:'搜索网络',denied:'注册被拒绝'})[cellular.registration]||cellular.registration],['接入制式',join(cellular.access_technologies)],['信号质量',cellular.signal_percent==null?null:`${cellular.signal_percent}%`],['当前驻留频段',f.band?`LTE B${f.band}`:null],['下行频率',f.downlink_mhz==null?null:`${f.downlink_mhz.toFixed(1)} MHz（按信道计算）`],['上行频率',f.uplink_mhz==null?null:`${f.uplink_mhz.toFixed(1)} MHz（按信道计算）`],['当前带宽',c.bandwidth_mhz==null?'设备暂未提供':`${c.bandwidth_mhz} MHz`],['小区 ID',c.cell_id?`0x${c.cell_id}`:null],['物理小区 ID',c.physical_cell_id],['TAC',c.tracking_area_code?`0x${c.tracking_area_code}`:null],['EARFCN 信道',c.earfcn],['RSRP',c.rsrp==null?null:`${c.rsrp.toFixed(1)} dBm`],['RSRQ',c.rsrq==null?null:`${c.rsrq.toFixed(1)} dB`],['SINR',c.sinr==null?null:`${c.sinr.toFixed(1)} dB`],['允许使用的频段（不是当前驻留）',join(cellular.configured_bands)]];
}
export class NetworkPanel {
 constructor({document,request,getToken,runTest}){this.doc=document;this.request=request;this.getToken=getToken;this.runTest=runTest;this.epoch=0;this.running=false;this.timer=null;this.sample=null;this.editing=false;this.history=[];this.pending=null;this.testing=false;
  this.$=id=>document.getElementById(id);
  this.$('network-refresh').onclick=()=>this.refresh();
  this.$('network-policy-save').onclick=()=>this.savePolicy();
  this.$('network-test-connectivity').onclick=()=>this.test('connectivity');
  this.$('network-test-download').onclick=()=>this.test('download');
  for(const id of ['network-limit-enabled','network-limit-gb','network-reset-day','network-ack-gap'])this.$(id).addEventListener('input',()=>{this.editing=true;});
 }
 start(){if(this.running)return;this.running=true;this.epoch++;void this.refresh();}
 stop(){this.running=false;this.epoch++;clearTimeout(this.timer);this.timer=null;this.pending=null;this.sample=null;this.history=[];this.editing=false;
  for(const id of ['network-devices','network-radio','network-month','network-test-result'])this.$(id).replaceChildren();this.$('network-interface').replaceChildren();this.$('network-state').textContent='登录所有者或具有网络管理权限的浏览器后查看';this.$('network-rates').textContent='等待登录';this.draw();
  this.$('network-policy-save').disabled=true;this.$('network-limit-gb').value='';this.$('network-limit-enabled').checked=false;this.$('network-ack-gap').checked=false;
 }
 rows(parent,rows){parent.replaceChildren();for(const [label,value] of rows){const a=this.doc.createElement('dt'),b=this.doc.createElement('dd');a.textContent=label;b.textContent=text(value);parent.append(a,b);}}
 async refresh(){clearTimeout(this.timer);if(!this.running||!this.getToken()||this.doc.hidden){this.schedule();return;}if(this.loading){this.schedule();return;}this.loading=true;const epoch=this.epoch;
  try{const s=await this.request({op:'network_diagnostics'});if(epoch!==this.epoch)return;if(s.available!==true)throw Error(s.reason||'网络监测尚未就绪');this.sample=s;this.render(s);this.$('network-state').textContent=this.pending?'正在等待设备应用限额设置…':`已更新 · ${new Date(s.sample_unix_seconds*1000).toLocaleTimeString()} · 仅查看状态不消耗测试流量`;
   if(this.pending&&s.last_request_id===this.pending){this.pending=null;this.editing=false;this.$('network-state').textContent='限额设置已生效；没有自动启用蜂窝数据';this.render(s);}
  }catch(error){if(epoch!==this.epoch)return;this.sample=null;this.$('network-state').textContent=error.message||'网络状态暂不可用';this.$('network-policy-save').disabled=true;this.$('network-devices').replaceChildren();this.$('network-radio').replaceChildren();this.$('network-month').replaceChildren();this.$('network-rates').textContent='数据已过期，等待重新连接';}
  finally{this.loading=false;this.schedule();}
 }
 schedule(){if(this.running)this.timer=setTimeout(()=>void this.refresh(),2500);}
 render(s){
  const devices=Array.isArray(s.devices)?s.devices.slice(0,16):[];const parent=this.$('network-devices');parent.replaceChildren();const select=this.$('network-interface'),selected=select.value;select.replaceChildren();
  for(const d of devices){const box=this.doc.createElement('article'),h=this.doc.createElement('h3'),dl=this.doc.createElement('dl');dl.className='network-grid';h.textContent=`${({cellular:'蜂窝数据',wifi:'Wi-Fi',ethernet:'有线 / USB',loopback:'本机'})[d.kind]||text(d.kind)} · ${text(d.interface||d.device)}`;this.rows(dl,[['状态',d.state],['IPv4',join(d.ipv4)],['IPv6',join(d.ipv6)],['IPv4 网关',d.gateway_ipv4],['IPv6 网关',d.gateway_ipv6],['DNS',join([...(d.dns_ipv4||[]),...(d.dns_ipv6||[])])],['APN',d.kind==='cellular'?d.apn:'不适用'],...(d.kind==='cellular'?[]:[['本接口接收',bytes(d.rx_bytes)],['本接口发送',bytes(d.tx_bytes)]])]);box.append(h,dl);parent.append(box);
   if(typeof d.interface==='string'&&String(d.state).startsWith('100 ')&&d.ipv4?.length&&d.dns_ipv4?.length){const option=this.doc.createElement('option');option.value=d.interface;option.textContent=`${d.interface}${d.kind==='cellular'?' · 使用流量':''}`;select.append(option);}
  }
  if([...select.options].some(o=>o.value===selected))select.value=selected;
  this.rows(this.$('network-radio'),radioRows(s.cellular));const u=s.usage||{},p=s.policy||{};
  this.rows(this.$('network-month'),[['统计周期',u.period_start&&u.period_end?`${u.period_start} 至 ${u.period_end}（不含结束日，UTC+8）`:null],['本月接收',bytes(u.rx_bytes)],['本月发送',bytes(u.tx_bytes)],['本月合计',bytes(u.total_bytes)],['月额度',bytes(p.limit_bytes)],['剩余额度',bytes(u.remaining_bytes)],['到量断开数据',p.enabled?(u.limit_reached?'已阻断普通数据':'已开启'):'已关闭'],['计量状态',u.uncertain?'上次计量中断，需要确认差异':'正常']]);
  this.$('network-accounting-note').textContent=s.accounting_note||'';this.$('network-ack-row').hidden=!u.uncertain;
  if(!this.editing&&!this.pending){this.$('network-limit-enabled').checked=!!p.enabled;this.$('network-limit-gb').value=Number.isFinite(p.limit_bytes)?String(p.limit_bytes/1e9):'';this.$('network-reset-day').value=String(p.reset_day||1);}
  this.$('network-policy-save').disabled=!!this.pending;
  this.$('network-rates').textContent=`蜂窝下载 ${rate(s.rx_bytes_per_second)}　上传 ${rate(s.tx_bytes_per_second)}`;
  this.history.push([Math.max(0,Number(s.rx_bytes_per_second)||0),Math.max(0,Number(s.tx_bytes_per_second)||0)]);this.history=this.history.slice(-60);this.draw();
 }
 draw(){const canvas=this.$('network-chart'),c=canvas.getContext?.('2d');if(!c)return;c.clearRect(0,0,canvas.width,canvas.height);const max=Math.max(1,...this.history.flat());for(const [column,color] of [[0,'#5bd3bd'],[1,'#eac36f']]){c.strokeStyle=color;c.lineWidth=2;c.beginPath();this.history.forEach((p,i)=>{const x=i/59*canvas.width,y=canvas.height-8-p[column]/max*(canvas.height-16);if(i)c.lineTo(x,y);else c.moveTo(x,y);});c.stroke();}}
 async savePolicy(){if(!this.sample||this.pending)return;const epoch=this.epoch;try{const policy=policyInput(this.$('network-limit-enabled').checked,this.$('network-limit-gb').value,this.$('network-reset-day').value);const id=crypto.randomUUID();this.pending=id;this.$('network-policy-save').disabled=true;
   const r=await this.request({op:'set_network_policy',request_id:id,...policy,acknowledge_meter_gap:this.$('network-ack-gap').checked});if(epoch!==this.epoch)return;if(r.request_id!==id||r.state!=='pending')throw Error('设备尚未确认收到设置');await this.refresh();
  }catch(error){if(epoch!==this.epoch)return;this.pending=null;this.$('network-policy-save').disabled=false;this.$('network-state').textContent=`设置未确认：${error.message||'请求失败'}；需要所有者在最近5分钟内登录，请刷新核对实际设置后再操作`;}
 }
 async test(profile){if(this.testing||!this.sample)return;const iface=this.$('network-interface').value;if(!iface){this.$('network-test-result').textContent='没有已联网且具备 IPv4/DNS 的接口；请先连接网络';return;}const epoch=this.epoch;this.testing=true;this.$('network-test-result').textContent='正在测试所选链路，最多15秒…';for(const id of ['network-test-connectivity','network-test-download'])this.$(id).disabled=true;
  try{const result=await this.runTest({interface:iface,profile,allow_cellular:this.$('network-cellular-consent').checked});if(epoch!==this.epoch)return;this.$('network-test-result').textContent=result.ok?`通过 · ${result.target} · DNS ${result.dns_ms} ms / TCP ${result.tcp_ms} ms / TLS ${result.tls_ms} ms / 首字节 ${result.first_byte_ms} ms · 接收 ${bytes(result.received_bytes)} · 本次样本 ${result.sample_download_mbps.toFixed(2)} Mbps（不代表最大带宽）`:`未通过：${result.reason||'请求失败'}；未自动重试或切换链路`;}
  catch(error){if(epoch===this.epoch)this.$('network-test-result').textContent=`测试未完成：${error.message||'连接中断'}；未自动重试`;}
  finally{this.testing=false;for(const id of ['network-test-connectivity','network-test-download'])this.$(id).disabled=false;if(epoch===this.epoch)void this.refresh();}
 }
}
