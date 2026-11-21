// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';
const code=(await readFile(new URL('../crates/edge/src/web/console.js',import.meta.url))).toString();const html=(await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url))).toString();const css=(await readFile(new URL('../crates/edge/src/web/console.css',import.meta.url))).toString();const linux=(await readFile(new URL('../crates/edge/src/linux.rs',import.meta.url))).toString();
const {ConsoleDashboard,PAGES,cpuPercent,usedPercent,signalLabel,rsrpLevel,rsrqLevel,sinrLevel,networkView,deviceClock,otpOf,maskNumber,callView,bytesText,uptimeText,shortTime}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
// 纯函数
assert.equal(cpuPercent({cpu_busy_ticks:100,cpu_total_ticks:1000},{cpu_busy_ticks:150,cpu_total_ticks:1200}),25);
for(const [a,b] of [[null,{cpu_busy_ticks:1,cpu_total_ticks:2}],[{cpu_busy_ticks:9,cpu_total_ticks:9},{cpu_busy_ticks:1,cpu_total_ticks:20}],[{cpu_busy_ticks:1,cpu_total_ticks:9},{cpu_busy_ticks:2,cpu_total_ticks:9}],[{cpu_busy_ticks:'1',cpu_total_ticks:2},{cpu_busy_ticks:2,cpu_total_ticks:9}]])assert.equal(cpuPercent(a,b),null,'first sample, reset counters and bad types stay unknown');
assert.equal(usedPercent(100,45),55);for(const [t,f] of [[0,0],[null,1],[10,20],[10,null]])assert.equal(usedPercent(t,f),null);
assert.deepEqual([0,1,25,50,75,100].map(p=>signalLabel(p).bars),[0,1,2,3,4,4]);for(const bad of [null,-1,101,NaN,'80'])assert.equal(signalLabel(bad).label,'未知');
assert.deepEqual([-80,-90,-100,-110,-120,0,null].map(rsrpLevel),[4,3,2,1,0,0,0]);assert.deepEqual([-8,-11,-14,-18,-25,1].map(rsrqLevel),[4,3,2,1,0,0]);assert.deepEqual([25,15,8,2,-5,99].map(sinrLevel),[4,3,2,1,0,0]);
assert.equal(otpOf('【银行】验证码 482916，5分钟内有效'),'482916');assert.equal(otpOf('Your verification code is 7351.'),'7351');assert.equal(otpOf('您本月剩余382分钟，订单 20261103'),null);assert.equal(otpOf('验证码 123'),null);assert.equal(otpOf('验证码 123456789'),null);assert.equal(otpOf(42),null);
assert.equal(maskNumber('+8613812345678'),'+86 138****5678');assert.equal(maskNumber('10010'),'10010');assert.equal(maskNumber(''),null);
assert.equal(callView({direction:1,state:'ended',number:'13812345678'}).label,'来电 · 已结束');assert.equal(callView({direction:2,state:'active'}).live,true);assert.doesNotMatch(JSON.stringify(callView({direction:1,state:'ended'})),/未接/,'calls carry no answered flag');assert.equal(callView(null).number,'号码未知');
assert.equal(bytesText(1.2e9),'1.20 GB');assert.equal(bytesText(-1),'—');assert.equal(uptimeText(90061),'1 天 01:01:01');assert.equal(uptimeText(-1),'—');assert.equal(shortTime('garbage'),'时间未知');assert.equal(shortTime(null),'时间未知');
{const c=deviceClock(1000,1000000,1003000);assert.equal(c.time.getTime(),1003000);assert.equal(c.synced,true);assert.equal(deviceClock(1000,1061001,0).synced,false);assert.equal(deviceClock('x',0,0),null);}
{const v=networkView({cellular:{operator:'中国联通',registration:'home',access_technologies:['lte'],signal_percent:60,serving_cell:{rsrp:-92,rsrq:-11,sinr:14,channel_derived:{band:3}}},devices:[{kind:'wifi',interface:'wlan0',state:'100 (connected)',ipv4:['192.168.8.1/24']},{kind:'cellular',state:'30 (disconnected)',ipv4:['10.0.0.1/8']}]});
 assert.deepEqual([v.operator,v.tech,v.registration,v.band,v.signal.bars,v.rsrp,v.rsrq,v.sinr,v.wifi.online,v.wifi.ip,v.data.online,v.data.ip],['中国联通','4G (LTE)','已注册','Band 3',3,-92,-11,14,true,'192.168.8.1',false,null],'a disconnected modem never shows an address');
 const empty=networkView({cellular:{operator:'x'.repeat(99),signal_percent:'90'},devices:'bad'});assert.equal(empty.operator.length,32);assert.equal(empty.signal.bars,0);assert.equal(empty.data.online,false);assert.equal(networkView(null).operator,null);}
// 页面：所有 ID 都在 index.html；CSP 只允许 'self'，首页不能有内联样式或脚本；导航目标都存在
const ids=new Set([...html.matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]));
for(const m of code.matchAll(/'(c-[a-z-]+)'/g))assert.ok(ids.has(m[1]),m[1]);
for(const [name,page] of Object.entries(PAGES))for(const id of page.show||[])assert.ok(ids.has(id),`${name}: ${id}`);
for(const m of html.matchAll(/data-go="([a-z]+)"/g))assert.ok(PAGES[m[1]],m[1]);
assert.doesNotMatch(html.slice(0,html.indexOf('<main class="legacy"')),/\sstyle=|<script(?! type="module" src)|\son[a-z]+=/i);assert.ok(html.includes('href="/console.css"'));
assert.match(linux,/"\/console\.js"/);assert.match(linux,/"\/console\.css"/);assert.doesNotMatch(css,/url\(|@import/,'no external or extra resources under CSP');
// 假 DOM
class El{constructor(id,tag='div'){this.id=id;this.tagName=tag.toUpperCase();this.children=[];this._t='';this.hidden=false;this.dataset={};this.attrs={};this.listeners={};this.style={};this.className='';this.value='';this.parentElement=null;this.classList={set:new Set(),add:c=>this.classList.set.add(c),remove:c=>this.classList.set.delete(c),toggle:(c,on)=>on?this.classList.set.add(c):this.classList.set.delete(c),contains:c=>this.classList.set.has(c)};}
 get textContent(){return this._t;}set textContent(v){this._t=v;this.children=[];}get firstChild(){if(!this.children.length)this.children=[{textContent:this._t}];return this.children[0];}get lastChild(){if(!this.children.length)this.children=[{textContent:this._t}];return this.children.at(-1);}
 append(...v){for(const n of v)if(n instanceof El)n.parentElement=this;this.children.push(...v);}replaceChildren(...v){this.children=[];this.append(...v);}setAttribute(k,v){this.attrs[k]=v;}dispatchEvent(){}click(){}insertBefore(n,r){this.children.splice(Math.max(0,this.children.indexOf(r)),0,n);}removeAttribute(k){delete this.attrs[k];}addEventListener(k,f){this.listeners[k]=f;}set innerHTML(_){throw Error('text only');}showModal(){this.open=true;}scrollIntoView(){}closest(){return null;}}
function dom(){const els=new Map(),get=(id,tag)=>{if(!els.has(id))els.set(id,new El(id,tag));const e=els.get(id);if(/-(bars|dots)$/.test(id)&&!e.children.length)for(let i=0;i<4;i++)e.append(new El());return e;};const doc={hidden:false,documentElement:new El('root'),getElementById:id=>get(id),createElement:t=>new El('',t),querySelectorAll:s=>s==='.nav[data-go]'?doc.navs:s==='[data-go]'?doc.navs:[],navs:[]};return {doc,els,get};}
const timers=[];globalThis.setTimeout=()=>0;globalThis.clearTimeout=()=>{};globalThis.setInterval=f=>{timers.push(f);return timers.length;};globalThis.clearInterval=()=>{};
const inbox={cursor:{db_epoch:'e',seq:'1'},visibility_seq:'0',total:3,messages:[{id:'a',display:{sender:'95588',timestamp:'11:47',preview:'验证码 482916'}},{id:'b',display:{sender:'10010',timestamp:'11:40',preview:'套餐余量 382 分钟'}},{id:'c',display:{sender:'1069',timestamp:'10:02',preview:'动态码 7351'}}]};
const sys=(busy=100,total=1000)=>({schema:1,firmware_version:'0.1.0',model:'UFI003',kernel:'6.12',os:'Debian',cpu_cores:4,cpu_busy_ticks:busy,cpu_total_ticks:total,memory_total_bytes:1000,memory_available_bytes:400,storage_total_bytes:2000,storage_free_bytes:500,temperature_c:46,uptime_seconds:90061});
function harness(handler,opts={}){const {doc,els,get}=dom(),seen=[];const nav=new El('nav','button');nav.dataset.go='comms';nav.classList.add('nav');doc.navs=[nav];const legacy=get('c-legacy','main');for(const id of ['phone-panel','sms-panel','notice','c-empty','login-section','session-section'])legacy.append(get(id,'details'));get('c-mc-f').value='all';const h=new (opts.Dashboard||ConsoleDashboard)({document:doc,clock:opts.clock||(()=>Date.UTC(2026,10,7,12,0,0)),copy:async()=>{},storage:opts.storage,matchDark:opts.matchDark,onLeave:opts.onLeave,readDelay:opts.readDelay,detailText:opts.detailText,confirm:opts.confirm,request:async c=>{seen.push(c);return handler(c);}});return {h,els,seen,doc,nav,$:id=>doc.getElementById(id)};}
{const {h,seen,$}=harness(c=>c.op==='inbox_recent'?inbox:c.op==='calls_recent'?{calls:[{direction:1,state:'ended',number:'13812345678'}]}:c.op==='line_status'?{status:'ready'}:c.op==='system_status'?sys():{});
 assert.equal($('c-online').lastChild.textContent,'未登录');assert.equal($('c-sms-empty').textContent,'登录后显示');await h.signedIn();
 assert.deepEqual(seen.map(c=>c.op),['system_status','line_status','inbox_recent','calls_recent']);assert.ok(!seen.some(c=>/forwarding|nkn|delete|set_|submit|prepare/.test(c.op)),'dashboard load is read-only and keeps forwarding/NKN manual');
 assert.equal($('c-otp-list').children.length,2);assert.equal($('c-sms-list').children.length,1);assert.equal($('c-call-list').children.length,1);assert.equal($('c-sim').textContent,'SIM 就绪');assert.equal($('c-model').textContent,'UFI003');assert.equal($('c-online').lastChild.textContent,'设备在线');
 assert.equal($('c-cpu').textContent,'采样中','single sample never claims a load');assert.equal($('c-mem').textContent.includes('(60%)'),true);assert.equal($('c-disk').textContent.includes('(75%)'),true);assert.equal($('c-temp').textContent,'46°C');assert.equal($('c-uptime').textContent,'1 天 01:01:01');
 h.renderSystem(sys(150,1200));assert.equal($('c-cpu').textContent,'25%');
 const now=Date.UTC(2026,10,7,12,0,0)/1000;h.network({sample_unix_seconds:now,cellular:{operator:'中国联通',signal_percent:80,serving_cell:{rsrp:-78}},usage:{rx_bytes:1.05e9,tx_bytes:.15e9},devices:[{kind:'wifi',interface:'wlan0',state:'100 (connected)',ipv4:['192.168.8.1/24']}]});
 assert.equal($('c-cell-op').textContent,'中国联通');assert.equal($('c-rsrp').textContent,'−78 dBm');assert.equal($('c-sync').hidden,true);assert.equal($('c-wifi-ip').textContent,'192.168.8.1');assert.match($('c-cell-usage').textContent,/1\.05 GB/);assert.equal($('c-rsrp-dots').children.filter(d=>d.classList.contains('on')).length,4);assert.equal($('c-data-pill').firstChild.textContent,'未连接');
 h.network({sample_unix_seconds:now-600});assert.equal($('c-sync').hidden,false,'drift warns');h.network(null);assert.equal($('c-cell-op').textContent,'—');assert.equal($('c-sync').hidden,true);
 inbox.messages.unshift({id:'z',display:{sender:'10086',preview:'新消息'}});await h.readLists();inbox.messages.shift();assert.ok(h.events.some(e=>e.message.includes('新短信')),'new mail is logged');
 h.reset();assert.equal($('c-cpu').textContent,'—');assert.equal($('c-otp-list').children.length,0);assert.equal($('c-sms-empty').textContent,'登录后显示');}
{
 const {h,$}=harness(()=>({}));
 for(const sample of [{sms_storage:{used_slots:50,total_slots:50}},{sms_storage:{used_slots:48,total_slots:50}}]){
  h.network(sample);
  assert.equal($('c-sm-sms-storage').textContent,sample.sms_storage.used_slots+' / 50 个槽位');
  assert.equal($('c-mail-sim-full').hidden,false);
  assert.equal($('c-mc-sim-full').textContent,$('c-mail-sim-full').textContent);
 }
 assert.match($('c-mail-sim-full').textContent,/仅剩 2 个槽位/);
 for(const sample of [{sms_storage:{used_slots:5,total_slots:50}},{sms_storage:{used_slots:51,total_slots:50}},null]){
  h.network(sample);assert.equal($('c-mail-sim-full').hidden,true);
 }
 assert.equal($('c-sm-sms-storage').textContent,'—');
}
// 读失败：显示“暂不可用”而非空；旧接口不能区分撤权/临时错误，保守清除私有缓存
{let now=10000;const {h,$}=harness(()=>{throw Error('busy');},{clock:()=>now});await h.signedIn();assert.match($('c-sms-empty').textContent,/短信暂不可用/);assert.match($('c-call-empty').textContent,/通话记录暂不可用/);assert.equal($('c-cpu').textContent,'—');
 assert.equal($('c-online').lastChild.textContent,'设备在线','a successful login is recent contact');
 now+=31000;h.tick();assert.equal($('c-online').lastChild.textContent,'暂时读不到设备');assert.equal($('c-online-short').textContent,'状态待同步');
 h.network({sample_unix_seconds:31,cellular:{signal_percent:70}});assert.equal($('c-online').lastChild.textContent,'设备在线');assert.equal($('c-online-short').textContent,'在线');
 h.network(null);assert.equal($('c-online-short').textContent,'在线','a transient loss of network status does not imply the device is unreachable');}
{let reads=0;const {h,$}=harness(c=>{
 if(c.op==='system_status'&&++reads===1)throw Object.assign(Error('busy'),{coreCode:'busy'});
 return c.op==='system_status'?sys():{};
 },{readDelay:()=>Promise.resolve()});
 await h.signedIn();assert.equal(reads,2);assert.equal($('c-model').textContent,'UFI003','temporary core contention is retried');
 h.reset();assert.equal($('c-online-short').textContent,'未登录');}

{let fail=false;const {h,$}=harness(c=>{if(fail)throw Error('busy');return c.op==='inbox_recent'?inbox:{};});await h.signedIn();assert.equal($('c-otp-list').children.length,2);fail=true;await h.readLists();assert.equal($('c-otp-list').children.length,0,'unavailable or revoked reads clear old private display data');}
// 异常字段：不抛错，显示未知
{const {h,$}=harness(c=>c.op==='system_status'?{cpu_busy_ticks:'x',memory_total_bytes:-1,storage_total_bytes:{},temperature_c:'hot',uptime_seconds:1.5,model:7}:c.op==='inbox_recent'?{messages:'bad'}:c.op==='calls_recent'?{calls:[null]}:null);await h.signedIn();assert.equal($('c-temp').textContent,'—');assert.equal($('c-mem').textContent,'—');assert.equal($('c-model').textContent,'型号未知');assert.equal($('c-uptime').textContent,'—');assert.equal($('c-call-list').children.length,1);}
// 清理验证码：只删验证码；中途拒绝即停止，不重试
{const deletes=[];const {h,$}=harness(c=>{if(c.op==='delete_sms'){deletes.push(c.sms_id);if(c.sms_id==='c')throw Error('需要所有者最近5分钟内登录');return {sms_id:c.sms_id,deleted:true};}return c.op==='inbox_recent'?inbox:{};});await h.signedIn();await h.clearOtps();assert.deepEqual(deletes,['a','c']);assert.match($('c-otp-state').textContent,/已移到回收站 1 条，其余未处理.*未自动重试/);}
{let n=0;const {h,$}=harness(c=>{if(c.op==='delete_sms'){n++;return {sms_id:'other',deleted:true};}return c.op==='inbox_recent'?inbox:{};});await h.signedIn();await h.clearOtps();assert.equal(n,1);assert.match($('c-otp-state').textContent,/已移到回收站 0 条.*响应不匹配/);}
// 确认框取消不删；打开确认框前没有写操作
{const {h,$,seen}=harness(c=>c.op==='inbox_recent'?inbox:{});await h.signedIn();$('c-otp-clear').listeners.click();assert.equal($('c-otp-dialog').open,true);$('c-otp-dialog').returnValue='cancel';$('c-otp-dialog').listeners.close();await Promise.resolve();assert.ok(!seen.some(c=>c.op==='delete_sms'));}
// 登出打断：旧请求结果不再渲染，也不继续删除
{let release,calls=0;const {h,$}=harness(c=>{if(c.op==='delete_sms'){calls++;return new Promise(r=>{release=()=>r({sms_id:c.sms_id,deleted:true});});}return c.op==='inbox_recent'?inbox:{};});await h.signedIn();const clearing=h.clearOtps();await Promise.resolve();h.reset();release();await clearing;assert.equal(calls,1);assert.equal($('c-otp-list').children.length,0);}
{let release;const {h,$}=harness(c=>c.op==='line_status'?new Promise(r=>{release=()=>r({status:'ready'});}):{});h.active=true;const reading=h.readLists();h.reset();release();await reading;assert.equal($('c-sim').textContent,'登录后显示设备信息');}
// 轮询：页面隐藏时不读
{timers.length=0;const {h,doc,seen}=harness(()=>({}));await h.signedIn();const before=seen.length;doc.hidden=true;for(const f of timers)f();await Promise.resolve();assert.equal(seen.length,before);doc.hidden=false;timers[0]();await Promise.resolve();assert.ok(seen.length>before);const after=seen.length;h.reset();for(const f of timers)f();await Promise.resolve();assert.equal(seen.length,after,'old timers are fenced by logout');}
// 导航：新页面用自己的区块；旧功能只在 设置 → 设备调试 出现，打开时展开目标
{let left=0;const {h,$,nav,doc}=harness(()=>({}),{onLeave:()=>left++});h.go('comms');assert.equal($('c-title').textContent,'通信中心');assert.equal($('c-dash').hidden,true);assert.equal($('c-legacy').hidden,true,'old panels stay out of new pages');assert.equal($('v-sms').hidden,false);assert.equal($('v-mcomm').hidden,false);assert.equal(nav.attrs['aria-current'],'page');assert.equal(left,1);assert.equal(doc.documentElement.dataset.page,'comms');
 h.go('comms',2);assert.equal($('v-phone').hidden,false);assert.equal($('v-sms').hidden,true);assert.equal(left,2);h.go('nope');assert.equal($('c-title').textContent,'通信中心');
 h.debug('sms-panel');assert.equal($('c-title').textContent,'设置');assert.equal($('c-legacy').hidden,false);assert.ok($('c-legacy').classList.contains('debug'));assert.equal($('sms-panel').classList.contains('page-off'),false);assert.equal($('sms-panel').open,true);
 h.go('login');assert.equal($('c-legacy').classList.contains('debug'),false);assert.equal($('login-section').classList.contains('page-off'),false);assert.equal($('sms-panel').classList.contains('page-off'),true);
 h.go('dashboard');assert.equal($('c-dash').hidden,false);assert.equal($('c-legacy').hidden,true);assert.equal($('v-mhome').hidden,false);assert.equal($('c-back').hidden,true);}
for(const [name,page] of Object.entries(PAGES))for(const tab of page.tabs||[])assert.ok(tab.view||tab.debug||(tab.show||[]).every(id=>id==='login-section'||name==='forwarding'&&['forwarding-panel','forwarding-history-panel'].includes(id)),`${name}/${tab.label} must not expose old panels outside device debug`);
assert.equal(PAGES.settings.tabs.at(-1).label,'设备调试');
// 新页面行为：读短信、回复、发送走原流程、拨号走原流程、删除需确认且拒绝不重试
{const seen=[];let confirmAnswer=false;const {h,$}=harness(c=>{seen.push(c.op);if(c.op==='inbox_recent')return inbox;if(c.op==='sms_detail')return {text:'完整正文'};if(c.op==='delete_sms')throw Error('需要所有者最近5分钟内登录');return {};},{detailText:v=>`来自：x\n\n${v.text}`,confirm:()=>confirmAnswer});await h.signedIn();h.go('comms');
 assert.equal(h.mail.length,3);await h.openMail(h.mail[1]);assert.equal($('c-mail-body').textContent,'完整正文');assert.ok(seen.includes('notification_read'));
 await h.deleteMail();assert.ok(!seen.includes('delete_sms'),'declined confirm sends nothing');confirmAnswer=true;await h.deleteMail();assert.equal(seen.filter(o=>o==='delete_sms').length,1);assert.match($('c-mail-operation').textContent,/操作未全部确认/);
 $('c-mail-reply').listeners.click();assert.equal($('c-cp-to').value,'10010');assert.equal($('c-title').textContent,'通信中心');
 $('c-cp-text').value='你好';let sent=0,dialed=0;$('send-sms').disabled=true;$('send-sms').click=()=>sent++;h.sendSms();assert.equal(sent,0,'disabled original control is not bypassed');assert.match($('c-cp-state').textContent,/不能发送/);$('send-sms').disabled=false;h.sendSms();assert.equal(sent,1);assert.equal($('recipient').value,'10010');assert.equal($('text').value,'你好');
 $('c-ph-num').value='10010';$('phone-dial').disabled=false;$('phone-dial').click=()=>dialed++;h.dial();assert.equal(dialed,1);assert.equal($('phone-number').value,'10010');}
{const {h,$}=harness(c=>c.op==='clients'?{clients:[{kind:'owner',public_key:[1,2,3,4,5,6,7],active:true,scopes:['a']}]}:{});await h.loadClients();assert.equal($('c-sc-list').children.length,1);const b=harness(()=>{throw Error('denied');});await b.h.loadClients();assert.match(b.$('c-sc-empty').textContent,/读取失败/);}
// The communication center exhausts private pages on entry. The dashboard still reads a preview.
{const row=i=>({id:'m'+i,display:{sender:'1',preview:'x'+i}}),seen=[];let total=40;
 const {h,$}=harness(c=>{seen.push(c);if(c.op==='inbox_recent'){
  const top=c.before?Number(c.before.seq):total+1;
  const messages=Array.from({length:top-1},(_,i)=>row(top-i-1)).slice(0,c.limit);
  return {messages,total,visibility_seq:'0',cursor:{db_epoch:'e',seq:messages.at(-1)?.id.slice(1)||'0'}};
 }return {};});await h.signedIn();
 assert.equal(h.mail.length,16);
 h.go('comms');await h.readLists();
 assert.equal(h.mail.length,40);assert.equal(h.mail.at(-1).id,'m1');
 assert.equal(seen.filter(c=>c.op==='inbox_recent').at(-1).before.seq,'9');
 assert.ok(!html.includes('id="c-mail-more"'));
 total=41;await h.readLists();assert.equal(h.mail.length,41);assert.equal(h.mail[0].id,'m41');
 assert.equal($('c-mh-q-otp').textContent,'0');
}
for(const bad of [undefined,-1,1.5,'128',null]){const {h,$}=harness(c=>c.op==='inbox_recent'?{messages:[{id:'a',display:{sender:'1',preview:'x'}}],total:bad,cursor:{db_epoch:'e',seq:'1'}}:c.op==='calls_recent'?{calls:[{id:'c',direction:1,state:'ended'}],total:bad}:{});await h.signedIn();assert.equal($('c-mh-q-sms').textContent,'1+');assert.equal($('c-mh-q-call').textContent,'1+');}
// 主题：优先保存的选择；存储不可用不影响页面
{const box={};const storage={getItem:k=>box[k]??null,setItem:(k,v)=>{box[k]=v;}};const {doc,$}=harness(()=>({}),{storage,matchDark:()=>true});assert.equal(doc.documentElement.dataset.theme,'dark');$('c-theme').listeners.click();assert.equal(doc.documentElement.dataset.theme,'light');assert.equal(box['rustsimhub-theme'],'light');const b=harness(()=>({}),{storage,matchDark:()=>true});assert.equal(b.doc.documentElement.dataset.theme,'light');}
{const storage={getItem(){throw Error('blocked');},setItem(){throw Error('blocked');}};const {doc,$}=harness(()=>({}),{storage});assert.equal(doc.documentElement.dataset.theme,'light');$('c-theme').listeners.click();assert.equal(doc.documentElement.dataset.theme,'dark');}
assert.ok(!code.includes('fetch(')&&!code.includes('innerHTML'));
// AirSIM 直连：状态以设备为准；配对码只显示在页面且退出即清除；停用和解除配对需确认；失败不显示成功
{const seen=[];let state={enabled:false,pcm_rate:8000,window_seconds:null,pairings:[]},confirmAnswer=false,now=1793600000000;
 const {h,$}=harness(c=>{seen.push(c);if(c.op==='airsim_status')return state;if(c.op==='airsim_configure'){state={...state,enabled:c.enabled,pcm_rate:c.pcm_rate};return state;}
  if(c.op==='airsim_open_pairing')return {code:'042917',expires_at:1793600120,pairing_port:7594};if(c.op==='airsim_revoke'){state={...state,pairings:[]};return {revoked:true};}return {};},{confirm:()=>confirmAnswer,clock:()=>now});
 await h.signedIn();assert.ok(!seen.some(c=>String(c.op).startsWith('airsim')),'nothing AirSIM is read until asked');
 await $('c-as-load').listeners.click();await h.airsim({op:'airsim_status'});assert.equal($('c-as-state').textContent,'未启用');assert.equal($('c-as-on').hidden,false);assert.equal($('c-as-pair').hidden,true);assert.equal($('c-as-empty').textContent,'启用后可以配对 iPhone');
 $('c-as-rate').value='16000';await h.airsim({op:'airsim_configure',enabled:true,pcm_rate:Number($('c-as-rate').value)});assert.deepEqual(seen.at(-1),{op:'airsim_configure',enabled:true,pcm_rate:16000});
 assert.equal($('c-as-state').textContent,'已启用');assert.equal($('c-as-rate').value,'16000');assert.equal($('c-as-on').hidden,true);assert.equal($('c-as-pair').hidden,false);assert.equal($('c-as-off').hidden,false);
 await h.airsimPair();assert.equal($('c-as-code').hidden,false);assert.equal($('c-as-digits').textContent,'042917');assert.match($('c-as-hint').textContent,/端口填 7594.*剩余 120 秒/);
 now+=121000;for(const f of timers.slice(-1))f();assert.equal($('c-as-code').hidden,true,'an expired code leaves the page');assert.equal($('c-as-digits').textContent,'');
 state={...state,pairings:[{id:'0123456789abcdef01234567',device_hint:'AB12',last_peer:'192.168.1.20',last_seen_ms:1793600000000}]};await h.airsim({op:'airsim_status'});assert.equal($('c-as-list').children.length,1);
 const revoke=$('c-as-list').children[0].children.at(-1);const before=seen.length;await revoke.listeners.click();assert.equal(seen.length,before,'declined confirm sends nothing');
 confirmAnswer=true;revoke.listeners.click();await new Promise(r=>setImmediate(r));await new Promise(r=>setImmediate(r));assert.deepEqual(seen.slice(before).map(c=>c.op),['airsim_revoke','airsim_status']);assert.equal(seen[before].pairing_id,'0123456789abcdef01234567');assert.equal($('c-as-list').children.length,0);
 confirmAnswer=false;const n=seen.length;$('c-as-off').listeners.click();await new Promise(r=>setImmediate(r));assert.equal(seen.length,n,'disabling asks first');
 now=1793600000000;await h.airsimPair();assert.equal($('c-as-code').hidden,false);h.reset();assert.equal($('c-as-code').hidden,true,'logout clears the code');assert.equal($('c-as-digits').textContent,'');assert.equal($('c-as-state').textContent,'未读取');assert.equal($('c-as-list').children.length,0);}
{const {h,$}=harness(c=>{if(String(c.op).startsWith('airsim'))throw Error('需要所有者最近5分钟内登录');return {};});await h.signedIn();
 assert.equal(await h.airsim({op:'airsim_configure',enabled:true,pcm_rate:8000}),null);assert.match($('c-as-msg').textContent,/未完成：需要所有者最近5分钟内登录/);assert.equal($('c-as-state').textContent,'未读取','a refused change never shows as enabled');
 await h.airsimPair();assert.equal($('c-as-code').hidden,true);assert.match($('c-as-msg').textContent,/没有打开配对窗口/);}
for(const bad of [{code:'12345',expires_at:1793600120,pairing_port:7594},{code:'abcdef',expires_at:1793600120,pairing_port:7594},{code:'042917',pairing_port:7594},{code:'042917',expires_at:1793600120},null]){const {h,$}=harness(c=>c.op==='airsim_open_pairing'?bad:{},{clock:()=>1793600000000});await h.signedIn();await h.airsimPair();assert.equal($('c-as-code').hidden,true);assert.equal($('c-as-digits').textContent,'');}
console.log('PASS: console projections, unknown-not-zero metrics, read-only load with manual forwarding/NKN, unavailable vs empty, confirmed OTP clean stops on denial/mismatch, logout and hidden-page fencing, navigation, theme, CSP-safe markup');

export {harness};
