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
 return [['统计状态',cached?`缓存估计：${freshness[value.freshness]}`:'原统计模块返回查询成功'],['统计窗口',windows[e.window]],['近期活跃安装估计',count],['原始观察时间',stamp(e.observed_at_secs)],['上次成功读取',stamp(e.succeeded_at_secs)],['缓存到期',stamp(e.expires_at_secs)],['说明','仅统计独立授权的近期活跃安装估计；不是人数、在线用户数或连接节点数。此页面不启用引擎、续订、转发或数据流量。']];
}
export class UsageStatsPanel {
 constructor({document,root,read=null,clock=()=>performance.now()}){this.doc=document;this.read=read;this.clock=clock;this.epoch=0;this.lastRead=-Infinity;this.busy=false;this.select=document.createElement('select');this.select.setAttribute('aria-label','统计窗口');for(const [value,label] of Object.entries(windows)){const o=document.createElement('option');o.value=value;o.textContent=label;this.select.append(o);}this.select.value='24h';this.button=document.createElement('button');this.button.type='button';this.button.textContent='只读刷新统计';this.button.onclick=()=>this.refresh();this.state=document.createElement('p');this.state.setAttribute('role','status');this.rows=document.createElement('dl');this.rows.style.overflowWrap='anywhere';root.replaceChildren(this.select,this.button,this.state,this.rows);this.reset();}
 display(rows){this.rows.replaceChildren();for(const [label,value] of rows){const dt=this.doc.createElement('dt'),dd=this.doc.createElement('dd');dt.textContent=label;dd.textContent=value;this.rows.append(dt,dd);}}
 reset(){this.epoch++;this.controller?.abort();this.busy=false;this.button.disabled=typeof this.read!=='function';this.display(usageRows(null));this.state.textContent=typeof this.read==='function'?'仅点击时读取已授权的统计结果':'统计读取端口尚未安装';}
 async refresh(){if(this.busy||typeof this.read!=='function')return;const now=this.clock();if(!Number.isFinite(now)||now-this.lastRead<5000){this.state.textContent='请至少间隔5秒后再手动读取；没有自动请求';return;}const window=this.select.value;if(!Object.hasOwn(windows,window))return;this.lastRead=now;const epoch=this.epoch;this.busy=true;this.button.disabled=true;this.controller=new AbortController();const controller=this.controller;let timer;try{const value=await Promise.race([this.read({window,signal:controller.signal}),new Promise((_,reject)=>{timer=setTimeout(()=>{controller.abort();reject(Error('deadline'));},10000);})]);if(epoch!==this.epoch)return;this.display(usageRows(value,window));this.state.textContent='已读取展示结果；没有启动统计、网络或续订任务';}catch{if(epoch===this.epoch){this.display(usageRows(null));this.state.textContent='读取未确认，当前计数未知；没有自动重试';}}finally{clearTimeout(timer);if(epoch===this.epoch){this.busy=false;this.button.disabled=false;}}}
}
