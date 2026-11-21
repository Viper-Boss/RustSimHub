// SPDX-License-Identifier: AGPL-3.0-only
export function bytes(value){if(!Number.isFinite(value)||value<0)return '暂不可用';const units=['B','KB','MB','GB','TB'];let i=0;while(value>=1000&&i<units.length-1){value/=1000;i++;}return `${value.toFixed(i?2:0)} ${units[i]}`;}
export function rate(value){return Number.isFinite(value)&&value>=0?`${(value/1e6*8).toFixed(2)} Mbps（${bytes(value)}/s）`:'等待下一次采样';}
export function policyInput(enabled,gb,day){const number=Number(gb),reset=Number(day);if(!Number.isFinite(number)||number<=0||number>100000||!Number.isInteger(reset)||reset<1||reset>31)throw Error('请填写有效额度和 1–31 日的重置日');const limit=Math.round(number*1e9);if(!Number.isSafeInteger(limit)||limit<1)throw Error('流量额度无效');return {enabled:!!enabled,limit_bytes:limit,reset_day:reset};}
const uuid=value=>typeof value==='string'&&/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(value);
const devicePin=value=>typeof value==='string'&&/^[0-9a-f]{64}$/.test(value);
const fields=(value,keys)=>value!==null&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).length===keys.length&&Object.keys(value).every(key=>keys.includes(key));
const requirePolicy=ok=>{if(!ok)throw Error('限额回执不可核验；保留原记录，没有重新提交');};
export function policyRecordInput(value,pin){
 requirePolicy(devicePin(pin)&&fields(value,['format','id','device_pin','enabled','limit_bytes','reset_day','acknowledge_meter_gap'])&&value.format===1&&uuid(value.id)&&value.device_pin===pin&&typeof value.enabled==='boolean'&&Number.isSafeInteger(value.limit_bytes)&&value.limit_bytes>=1&&value.limit_bytes<=100000e9&&Number.isInteger(value.reset_day)&&value.reset_day>=1&&value.reset_day<=31&&typeof value.acknowledge_meter_gap==='boolean');
 return {format:1,id:value.id,device_pin:pin,enabled:value.enabled,limit_bytes:value.limit_bytes,reset_day:value.reset_day,acknowledge_meter_gap:value.acknowledge_meter_gap};
}
export function policyJournalInput(value,pin){
 requirePolicy(devicePin(pin));if(value===null||value===undefined)return {format:1,pending:null,receipts:[]};
 requirePolicy(fields(value,['format','pending','receipts'])&&value.format===1&&Array.isArray(value.receipts)&&value.receipts.length<=64);
 const pending=value.pending===null?null:policyRecordInput(value.pending,pin),receipts=value.receipts.map(row=>{
  requirePolicy(fields(row,['record','state','boot_id','observed_unix_seconds']));const intent=policyRecordInput(row.record,pin);
  requirePolicy(row.state==='unknown'&&row.boot_id===null&&row.observed_unix_seconds===null||row.state==='observed'&&uuid(row.boot_id)&&Number.isSafeInteger(row.observed_unix_seconds)&&row.observed_unix_seconds>0);
  return {record:intent,state:row.state,boot_id:row.boot_id,observed_unix_seconds:row.observed_unix_seconds};
 });
 const ids=[...(pending?[pending.id]:[]),...receipts.map(row=>row.record.id)];requirePolicy(new Set(ids).size===ids.length);
 return {format:1,pending,receipts};
}
const samePolicy=(sample,intent)=>sample?.policy_applied===true&&sample.last_request_id===intent.id&&sample.policy?.enabled===intent.enabled&&sample.policy.limit_bytes===intent.limit_bytes&&sample.policy.reset_day===intent.reset_day&&(!intent.acknowledge_meter_gap||sample.usage?.uncertain===false)&&uuid(sample.boot_id)&&Number.isSafeInteger(sample.sample_unix_seconds)&&sample.sample_unix_seconds>0;
function policyReceipt(journal,row){const receipts=journal.receipts.slice();if(receipts.length>=64){const index=receipts.findIndex(entry=>entry.state==='observed');requirePolicy(index>=0);receipts.splice(index,1);}return {format:1,pending:null,receipts:[...receipts,row]};}
const text=value=>typeof value==='string'&&value.trim()!==''?value:Number.isFinite(value)?String(value):'暂不可用';
const record=value=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value:{};
const list=value=>Array.isArray(value)?value.filter(v=>typeof v==='string'&&v.trim()!==''):[];
const join=value=>list(value).length?list(value).join('、'):'暂无';
const measurement=(value,unit,positive=false)=>Number.isFinite(value)&&(!positive||value>0)?`${value.toFixed(1)} ${unit}`:null;
const chartRate=value=>Number.isFinite(value)&&value>=0?value:null;
export function radioRows(cellular={}){
 cellular=record(cellular);const c=record(cellular.serving_cell),f=record(c.channel_derived);
 const frequency=value=>measurement(value,'MHz',true)===null?null:`${measurement(value,'MHz',true)}（按信道计算）`;
 return [['运营商',text(cellular.operator)],['注册状态',({home:'本地注册',roaming:'漫游',searching:'搜索网络',denied:'注册被拒绝'})[cellular.registration]||text(cellular.registration)],['接入制式',join(cellular.access_technologies)],['信号质量',Number.isFinite(cellular.signal_percent)&&cellular.signal_percent>=0&&cellular.signal_percent<=100?`${cellular.signal_percent}%`:null],['当前驻留频段',Number.isInteger(f.band)&&f.band>0?`LTE B${f.band}`:null],['下行频率',frequency(f.downlink_mhz)],['上行频率',frequency(f.uplink_mhz)],['当前带宽',measurement(c.bandwidth_mhz,'MHz',true)||'设备暂未提供'],['小区 ID',typeof c.cell_id==='string'&&/^[0-9a-f]+$/i.test(c.cell_id)?`0x${c.cell_id}`:null],['物理小区 ID',text(c.physical_cell_id)],['TAC',typeof c.tracking_area_code==='string'&&/^[0-9a-f]+$/i.test(c.tracking_area_code)?`0x${c.tracking_area_code}`:null],['EARFCN 信道',Number.isInteger(c.earfcn)&&c.earfcn>=0?c.earfcn:null],['RSRP',measurement(c.rsrp,'dBm')],['RSRQ',measurement(c.rsrq,'dB')],['SINR',measurement(c.sinr,'dB')],['允许使用的频段（不是当前驻留）',join(cellular.configured_bands)]];
}
export class NetworkPanel {
 constructor({document,request,getToken,runTest,onSample=()=>{},getDevicePin,loadPolicyJournal,savePolicyJournal,confirm=()=>false}){Object.assign(this,{doc:document,request,getToken,runTest,onSample,getDevicePin,loadPolicyJournal,savePolicyJournal,confirm});this.epoch=0;this.running=false;this.timer=null;this.sample=null;this.editing=false;this.history=[];this.pending=null;this.testing=false;this.loading=false;this.journal=null;this.policyBusy=false;this.policyGeneration=0;
  this.$=id=>document.getElementById(id);
  this.$('network-refresh').onclick=()=>this.refresh();
  this.$('network-policy-save').onclick=()=>this.savePolicy();
  this.$('network-policy-archive').onclick=()=>this.archivePolicy();
  this.$('network-test-connectivity').onclick=()=>this.test('connectivity');
  this.$('network-test-download').onclick=()=>this.test('download');
  for(const id of ['network-limit-enabled','network-limit-gb','network-reset-day','network-ack-gap'])this.$(id).addEventListener('input',()=>{this.editing=true;this.policyControls();});
  this.testControls();this.policyControls();
 }
 testControls(){for(const id of ['network-test-connectivity','network-test-download'])this.$(id).disabled=!this.running||!this.sample||!this.getToken()||this.testing;}
 policyControls(){const ready=this.running&&!!this.getToken()&&!!this.sample&&!!this.journal&&!this.policyBusy,full=this.journal?.receipts.length===64&&!this.journal.receipts.some(row=>row.state==='observed');this.$('network-policy-save').disabled=!ready||!!this.pending||full||(!this.editing&&this.journal?.receipts.at(-1)?.state==='unknown');this.$('network-policy-archive').disabled=!ready||!this.pending||full;}
 policyCurrent(epoch,pin,generation=this.policyGeneration){requirePolicy(epoch===this.epoch&&this.running&&!!this.getToken()&&pin===this.getDevicePin?.()&&generation===this.policyGeneration);}
 useJournal(value,pin){this.journal=policyJournalInput(value,pin);this.pending=this.journal.pending?.id??null;}
 async writeJournal(next,previous,epoch,pin,generation){
  this.policyCurrent(epoch,pin,generation);requirePolicy(typeof this.savePolicyJournal==='function'&&typeof this.loadPolicyJournal==='function');
  await this.savePolicyJournal(next,previous,pin,()=>this.policyCurrent(epoch,pin,generation));this.policyCurrent(epoch,pin,generation);
  const actual=policyJournalInput(await this.loadPolicyJournal(pin),pin);this.policyCurrent(epoch,pin,generation);requirePolicy(JSON.stringify(actual)===JSON.stringify(next));this.useJournal(actual,pin);
 }
 start(){if(this.running)return;this.running=true;this.epoch++;void this.refresh();}
 stop(){this.running=false;this.epoch++;clearTimeout(this.timer);this.timer=null;this.pending=null;this.journal=null;this.policyBusy=false;this.policyGeneration++;this.sample=null;this.onSample(null);this.history=[];this.editing=false;this.loading=false;this.testing=false;this.testControls();this.policyControls();this.$('network-policy-state').textContent='登录后只读核对原设置；注销不清除本机回执';
  for(const id of ['network-devices','network-radio','network-month','network-test-result'])this.$(id).replaceChildren();this.$('network-interface').replaceChildren();this.$('network-state').textContent='登录所有者或具有网络管理权限的浏览器后查看';this.$('network-rates').textContent='等待登录';this.draw();
  this.$('network-policy-save').disabled=true;this.$('network-limit-gb').value='';this.$('network-limit-enabled').checked=false;this.$('network-ack-gap').checked=false;
  this.$('network-cellular-consent').checked=false;
 }
 rows(parent,rows){parent.replaceChildren();for(const [label,value] of rows){const a=this.doc.createElement('dt'),b=this.doc.createElement('dd');a.textContent=label;b.textContent=text(value);parent.append(a,b);}}
 async refresh(){clearTimeout(this.timer);if(!this.running||!this.getToken()||this.doc.hidden){this.schedule();return;}if(this.loading){this.schedule();return;}this.loading=true;const epoch=this.epoch,pin=this.getDevicePin?.(),generation=this.policyGeneration;
  try{requirePolicy(devicePin(pin)&&typeof this.loadPolicyJournal==='function');const reading=this.loadPolicyJournal(pin),response=this.request({op:'network_diagnostics'});const [saved,s]=await Promise.all([reading,response]);this.policyCurrent(epoch,pin);if(s.available!==true)throw Error('网络监测尚未就绪');
   if(generation===this.policyGeneration)this.useJournal(saved,pin);this.sample=s;this.render(s);this.onSample(s);this.$('network-state').textContent=`已更新 · ${new Date(s.sample_unix_seconds*1000).toLocaleTimeString()} · 仅查看状态不消耗测试流量`;
   if(generation===this.policyGeneration&&!this.pending&&!this.policyBusy){const last=this.journal.receipts.at(-1);this.$('network-policy-state').textContent=last?last.state==='observed'?`原设置 ${last.record.id} 曾已核对；当前配置以上方刷新为准`:`原设置 ${last.record.id} 仍作为未知记录保留；只有修改表单后才能明确保存新设置`:'没有待核对的限额设置；保存不会自动打开蜂窝数据';}
   if(generation===this.policyGeneration&&this.pending){const intent=this.journal.pending;this.$('network-policy-state').textContent=`原设置 ${intent.id} 结果未确认；刷新只核对，不会重新提交`;
    if(samePolicy(s,intent)){const next=policyReceipt(this.journal,{record:intent,state:'observed',boot_id:s.boot_id,observed_unix_seconds:s.sample_unix_seconds});await this.writeJournal(next,this.journal,epoch,pin,generation);this.editing=false;this.$('network-ack-gap').checked=false;this.$('network-policy-state').textContent=`原设置 ${intent.id} 的策略与应用标记已核对；当前配置以上方刷新为准，没有自动启用蜂窝数据`;this.render(s);}
   }
  }catch(error){if(epoch!==this.epoch)return;this.sample=null;this.onSample(null);this.$('network-state').textContent=error.message||'网络状态暂不可用';this.$('network-policy-save').disabled=true;this.$('network-devices').replaceChildren();this.$('network-radio').replaceChildren();this.$('network-month').replaceChildren();this.$('network-rates').textContent='数据已过期，等待重新连接';this.$('network-interface').replaceChildren();this.history=[];this.draw();}
  finally{if(epoch===this.epoch){this.loading=false;this.testControls();this.policyControls();this.schedule();}}
 }
 schedule(){if(this.running)this.timer=setTimeout(()=>void this.refresh(),2500);}
 render(s){
  const devices=Array.isArray(s.devices)?s.devices.filter(d=>d!==null&&typeof d==='object'&&!Array.isArray(d)).slice(0,16):[];const parent=this.$('network-devices');parent.replaceChildren();const select=this.$('network-interface'),selected=select.value;select.replaceChildren();
  for(const d of devices){const box=this.doc.createElement('article'),h=this.doc.createElement('h3'),dl=this.doc.createElement('dl');dl.className='network-grid';h.textContent=`${({cellular:'蜂窝数据',wifi:'Wi-Fi',ethernet:'有线 / USB',loopback:'本机'})[d.kind]||text(d.kind)} · ${text(d.interface||d.device)}`;this.rows(dl,[['状态',d.state],['IPv4',join(d.ipv4)],['IPv6',join(d.ipv6)],['IPv4 网关',d.gateway_ipv4],['IPv6 网关',d.gateway_ipv6],['DNS',join([...list(d.dns_ipv4),...list(d.dns_ipv6)])],['APN',d.kind==='cellular'?d.apn:'不适用'],...(d.kind==='cellular'?[]:[['本接口接收',bytes(d.rx_bytes)],['本接口发送',bytes(d.tx_bytes)]])]);box.append(h,dl);parent.append(box);
   if(typeof d.interface==='string'&&String(d.state).startsWith('100 ')&&list(d.ipv4).length&&list(d.dns_ipv4).length){const option=this.doc.createElement('option');option.value=d.interface;option.textContent=`${d.interface}${d.kind==='cellular'?' · 使用流量':''}`;select.append(option);}
  }
  if([...select.options].some(o=>o.value===selected))select.value=selected;
  this.rows(this.$('network-radio'),radioRows(s.cellular));const u=record(s.usage),p=record(s.policy);
  this.rows(this.$('network-month'),[['统计周期',u.period_start&&u.period_end?`${u.period_start} 至 ${u.period_end}（不含结束日，UTC+8）`:null],['本月接收',bytes(u.rx_bytes)],['本月发送',bytes(u.tx_bytes)],['本月合计',bytes(u.total_bytes)],['月额度',bytes(p.limit_bytes)],['剩余额度',u.uncertain===false?bytes(u.remaining_bytes):u.uncertain===true?'计量不确定，暂不可用':'暂不可用'],['到量断开数据',p.enabled===true?(u.limit_reached===true?'已阻断普通数据':u.limit_reached===false?'已开启':'暂不可用'):p.enabled===false?'已关闭':'暂不可用'],['计量状态',u.uncertain===true?'上次计量中断，需要确认差异':u.uncertain===false?'正常':'暂不可用']]);
  this.$('network-accounting-note').textContent=s.accounting_note||'';this.$('network-ack-row').hidden=!u.uncertain;
  if(!this.editing&&!this.pending){this.$('network-limit-enabled').checked=!!p.enabled;this.$('network-limit-gb').value=Number.isFinite(p.limit_bytes)?String(p.limit_bytes/1e9):'';this.$('network-reset-day').value=String(p.reset_day||1);}
  this.policyControls();
  this.$('network-rates').textContent=`蜂窝下载 ${rate(s.rx_bytes_per_second)}　上传 ${rate(s.tx_bytes_per_second)}`;
  this.history.push([chartRate(s.rx_bytes_per_second),chartRate(s.tx_bytes_per_second)]);this.history=this.history.slice(-60);this.draw();
  this.testControls();
 }
 draw(){const canvas=this.$('network-chart'),c=canvas.getContext?.('2d');if(!c)return;c.clearRect(0,0,canvas.width,canvas.height);const max=Math.max(1,...this.history.flat().filter(Number.isFinite));for(const [column,color] of [[0,'#5bd3bd'],[1,'#eac36f']]){c.strokeStyle=color;c.lineWidth=2;c.beginPath();let connected=false;this.history.forEach((p,i)=>{if(p[column]===null){connected=false;return;}const x=i/59*canvas.width,y=canvas.height-8-p[column]/max*(canvas.height-16);if(connected)c.lineTo(x,y);else c.moveTo(x,y);connected=true;});c.stroke();}}
 async savePolicy(){if(!this.running||!this.getToken()||!this.sample||this.pending||this.policyBusy)return;const epoch=this.epoch,pin=this.getDevicePin?.(),generation=++this.policyGeneration;this.policyBusy=true;this.policyControls();let submitted=false,originalId=null;
  try{this.policyCurrent(epoch,pin,generation);requirePolicy(devicePin(pin)&&typeof this.loadPolicyJournal==='function'&&typeof this.savePolicyJournal==='function');const policy=policyInput(this.$('network-limit-enabled').checked,this.$('network-limit-gb').value,this.$('network-reset-day').value),ack=this.$('network-ack-gap').checked;
   const saved=await this.loadPolicyJournal(pin);this.policyCurrent(epoch,pin,generation);this.useJournal(saved,pin);requirePolicy(!this.pending&&(this.editing||this.journal.receipts.at(-1)?.state!=='unknown')&&(this.journal.receipts.length<64||this.journal.receipts.some(row=>row.state==='observed')));
   const intent=policyRecordInput({format:1,id:crypto.randomUUID(),device_pin:pin,...policy,acknowledge_meter_gap:ack},pin),previous=this.journal,next={...previous,pending:intent};originalId=intent.id;this.pending=intent.id;
   await this.writeJournal(next,previous,epoch,pin,generation);this.policyCurrent(epoch,pin,generation);submitted=true;
   const r=await this.request({op:'set_network_policy',request_id:intent.id,...policy,acknowledge_meter_gap:ack});this.policyCurrent(epoch,pin,generation);requirePolicy(r?.request_id===intent.id&&r.state==='pending'&&r.apn_activated===false);await this.refresh();
  }catch{if(epoch===this.epoch&&!this.journal?.receipts.some(row=>row.record.id===originalId&&row.state==='observed')){this.$('network-policy-state').textContent=submitted?`原设置 ${originalId} 结果未确认；保留原记录，刷新只核对，没有重发`:'设置未提交或本机记录未确认；请先刷新核对原记录，没有发送新请求';}}
  finally{if(epoch===this.epoch){this.policyBusy=false;this.policyControls();}}
 }
 async archivePolicy(){if(!this.running||!this.getToken()||!this.sample||!this.pending||this.policyBusy)return;const epoch=this.epoch,pin=this.getDevicePin?.(),generation=++this.policyGeneration;this.policyBusy=true;this.policyControls();
  try{this.policyCurrent(epoch,pin,generation);const saved=await this.loadPolicyJournal(pin);this.policyCurrent(epoch,pin,generation);this.useJournal(saved,pin);requirePolicy(this.pending);
   if(!this.confirm('原设置结果仍未确认。保留未知记录并解除本页阻止？这不会撤销或重发原请求；如需新设置，请先修改表单再明确保存。'))return;this.policyCurrent(epoch,pin,generation);
   const next=policyReceipt(this.journal,{record:this.journal.pending,state:'unknown',boot_id:null,observed_unix_seconds:null});await this.writeJournal(next,this.journal,epoch,pin,generation);this.editing=false;this.$('network-ack-gap').checked=false;this.render(this.sample);this.$('network-policy-state').textContent='原请求已作为未知记录保留，没有撤销或重发；只有修改表单后才能明确保存新设置';
  }catch{if(epoch===this.epoch)this.$('network-policy-state').textContent='原记录归档尚未确认；请刷新核对，没有重新提交';}
  finally{if(epoch===this.epoch){this.policyBusy=false;this.policyControls();}}
 }
 async test(profile){if(this.testing||!this.sample||!this.running||!this.getToken())return;const iface=this.$('network-interface').value;if(!iface){this.$('network-test-result').textContent='没有已联网且具备 IPv4/DNS 的接口；请先连接网络';return;}const epoch=this.epoch;this.testing=true;this.$('network-test-result').textContent='正在测试所选链路，最多15秒…';this.testControls();
  try{const result=await this.runTest({interface:iface,profile,allow_cellular:this.$('network-cellular-consent').checked});if(epoch!==this.epoch)return;this.$('network-test-result').textContent=result.ok?`通过 · ${result.target} · DNS ${result.dns_ms} ms / TCP ${result.tcp_ms} ms / TLS ${result.tls_ms} ms / 首字节 ${result.first_byte_ms} ms · 接收 ${bytes(result.received_bytes)} · 本次样本 ${result.sample_download_mbps.toFixed(2)} Mbps（不代表最大带宽）`:`未通过：${result.reason||'请求失败'}；未自动重试或切换链路`;}
  catch(error){if(epoch===this.epoch)this.$('network-test-result').textContent=`测试未完成：${error.message||'连接中断'}；未自动重试`;}
  finally{if(epoch===this.epoch){this.testing=false;this.testControls();void this.refresh();}}
 }
}
