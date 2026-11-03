// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';
const code=await readFile(new URL('../crates/edge/src/web/home.js',import.meta.url));const html=(await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url))).toString();
const {HomeDashboard,signalView,cellularView,deviceClock,otpOf,senderName,callView,greeting,bytesText,rateText}=await import('data:text/javascript;base64,'+code.toString('base64'));
// 纯函数
assert.deepEqual([0,1,25,50,75,100].map(p=>signalView(p).bars),[0,1,2,3,4,4]);for(const bad of [null,-1,101,NaN,'80'])assert.equal(signalView(bad).label,'信号未知');
assert.equal(otpOf('【银行】验证码 482916，5分钟内有效'),'482916');assert.equal(otpOf('Your verification code is 7351.'),'7351');assert.equal(otpOf('您本月剩余382分钟，订单 20261103'),null);assert.equal(otpOf('验证码 123'),null);assert.equal(otpOf('验证码 123456789'),null);assert.equal(otpOf(42),null);
assert.equal(senderName('+8613812345678'),'138 **** 5678');assert.equal(senderName('10010'),'10010');assert.equal(senderName(''),'未知发件人');
assert.equal(callView({direction:1,state:'terminated',number:'13812345678'}).kind,'missed');assert.equal(callView({direction:2,state:'active'}).kind,'out');assert.equal(callView(null).number,'未知发件人');
assert.equal(greeting(3),'夜深了');assert.equal(greeting(9),'早上好');assert.equal(greeting(12),'中午好');assert.equal(greeting(15),'下午好');assert.equal(greeting(21),'晚上好');
assert.equal(bytesText(1.2e9),'1.20 GB');assert.equal(bytesText(-1),'—');assert.equal(rateText(300000),'2.4 Mbps');assert.equal(rateText(null),'—');
{const c=deviceClock(1000,1000000,1003000);assert.equal(c.device.getTime(),1003000);assert.equal(c.synced,true);assert.equal(deviceClock(1000,1000000+61000,0).synced,false);assert.equal(deviceClock('x',0,0),null);}
{const v=cellularView({cellular:{operator:'中国联通',registration:'home',access_technologies:['lte'],signal_percent:60,serving_cell:{rsrp:-92,sinr:14,channel_derived:{band:3}}},devices:[{kind:'wifi',state:'100 (connected)'},{kind:'cellular',state:'30 (disconnected)'}]});
 assert.deepEqual([v.operator,v.registration,v.tech,v.band,v.signal.bars,v.rsrp,v.sinr,v.data,v.wifi],['中国联通','已注册','4G LTE','Band 3',3,-92,14,false,true]);
 const empty=cellularView({cellular:{operator:'<b>x</b>'.repeat(20),signal_percent:'90'},devices:'bad'});assert.equal(empty.operator.length,32);assert.equal(empty.signal.bars,0);assert.equal(empty.data,false);}
// 页面骨架：每个首页 ID 都在 index.html 中，且没有内联样式或脚本（CSP 只允许 'self'）
for(const id of code.toString().matchAll(/'(home-[a-z-]+)'/g))assert.ok(html.includes(`id="${id[1]}"`),id[1]);
const home=html.slice(html.indexOf('<section id="home"'),html.indexOf('</section>',html.indexOf('<section id="home"')));assert.ok(home.length>1000);assert.doesNotMatch(home,/style=|<script|on[a-z]+=/i);
// 假 DOM
class El{constructor(id){this.id=id;this.children=[];this.textContent='';this.hidden=false;this.dataset={};this.attrs={};this.listeners={};this.className='';this.classList={set:new Set(),add:c=>this.classList.set.add(c),remove:c=>this.classList.set.delete(c),toggle:(c,on)=>on?this.classList.set.add(c):this.classList.set.delete(c)};}
 get firstChild(){return this.children[0];}append(...v){this.children.push(...v);}replaceChildren(...v){this.children=v;}setAttribute(k,v){this.attrs[k]=v;}addEventListener(k,f){this.listeners[k]=f;}set innerHTML(_){throw Error('text only');}showModal(){this.open=true;}scrollIntoView(){}}
function dom(){const els=new Map(),doc={getElementById:id=>{if(!els.has(id))els.set(id,new El(id));const e=els.get(id);if(id==='home-bars'&&!e.children.length)e.children=[new El(),new El(),new El(),new El()];return e;},createElement:()=>new El()};return {doc,els};}
const timers=[];globalThis.setTimeout=(f,ms)=>{timers.push(f);return timers.length;};globalThis.clearTimeout=()=>{};
const inbox={messages:[{id:'a',display:{sender:'95588',timestamp:'11:47',preview:'验证码 482916'}},{id:'b',display:{sender:'10010',timestamp:'11:40',preview:'套餐余量 382 分钟'}},{id:'c',display:{sender:'1069',timestamp:'10:02',preview:'动态码 7351'}}]};
function harness(handler){const {doc,els}=dom(),seen=[],opened=[];const h=new HomeDashboard({document:doc,clock:()=>Date.UTC(2026,10,3,12,0,0),open:t=>opened.push(t),copy:async()=>{},request:async c=>{seen.push(c);return handler(c);}});return {h,els,seen,opened,$:id=>doc.getElementById(id)};}
{const {h,seen,$}=harness(c=>c.op==='inbox'?inbox:c.op==='calls'?{calls:[{direction:1,state:'terminated',number:'13812345678'}]}:c.op==='line_status'?{status:'ready'}:{enabled:true});
 h.reset();assert.equal($('home').dataset.state,'signed-out');assert.equal($('home-sms-empty').textContent,'登录后显示');await h.refresh();
 assert.deepEqual(seen.map(c=>c.op),['line_status','inbox','calls','forwarding_status','nkn_status']);assert.ok(seen.every(c=>!/delete|set_|submit|prepare/.test(c.op)),'refresh is read-only');
 assert.equal($('home-otp-list').children.length,2);assert.equal($('home-sms-list').children.length,1);assert.equal($('home-sim').textContent,'SIM 就绪');assert.equal($('home-forwarding-state').textContent,'已开启');assert.ok($('home-calls-list').children[0].classList.set.has('missed'));
 h.network({sample_unix_seconds:Date.UTC(2026,10,3,12,0,0)/1000,cellular:{operator:'中国联通',signal_percent:80},usage:{total_bytes:1.2e9},policy:{enabled:true,limit_bytes:1e10}});
 assert.equal($('home-operator').textContent,'中国联通');assert.equal($('home-sync').textContent,'已同步');assert.equal($('home-usage-limit').textContent,'/ 10.00 GB');assert.equal($('home-ring-value').attrs['stroke-dashoffset'],String(301.6*0.88));
 h.network(null);assert.equal($('home-sync').textContent,'未读取');}
// 读失败：显示“暂不可用”，不误报为空
{const {h,$}=harness(()=>{throw Error('busy');});await h.refresh();assert.equal($('home-sms-empty').textContent,'短信暂不可用');assert.equal($('home-calls-empty').textContent,'通话记录暂不可用');assert.equal($('home-sim').textContent,'SIM 状态未知');assert.equal($('home-forwarding-state').textContent,'点击查看');}
// 清理验证码：只删验证码；中途拒绝即停止，不重试
{let deletes=[];const {h,$}=harness(c=>{if(c.op==='delete_sms'){deletes.push(c.sms_id);if(c.sms_id==='c')throw Error('需要所有者最近5分钟内登录');return {sms_id:c.sms_id,deleted:true};}return c.op==='inbox'?inbox:{};});
 await h.refresh();await h.clearOtps();assert.deepEqual(deletes,['a','c']);assert.match($('home-otp-state').textContent,/已移到回收站 1 条，其余未处理.*未自动重试/);}
{let deletes=0;const {h,$}=harness(c=>{if(c.op==='delete_sms'){deletes++;return {sms_id:'other',deleted:true};}return c.op==='inbox'?inbox:{};});await h.refresh();await h.clearOtps();assert.equal(deletes,1);assert.match($('home-otp-state').textContent,/已移到回收站 0 条.*响应不匹配/);}
// 登出打断：旧请求结果不再渲染，也不继续删除
{let release;const {h,$}=harness(c=>c.op==='delete_sms'?new Promise(r=>{release=()=>r({sms_id:c.sms_id,deleted:true});}):c.op==='inbox'?inbox:{});await h.refresh();const clearing=h.clearOtps();await Promise.resolve();h.reset();release();await clearing;assert.equal($('home-otp-list').children.length,0);assert.equal($('home').dataset.state,'signed-out');}
{let release;const {h,$}=harness(c=>c.op==='line_status'?new Promise(r=>{release=()=>r({status:'ready'});}):{});const reading=h.refresh();h.reset();release();await reading;assert.equal($('home-sim').textContent,'—');}
// 导航只打开原业务区
{const {h,els,opened}=harness(()=>({}));els.get('home-forwarding').listeners.click();els.get('home-dial').listeners.click();assert.deepEqual(opened,['forwarding','phone']);}
assert.ok(!code.toString().includes('fetch(')&&!code.toString().includes('innerHTML'));
console.log('PASS: home dashboard projections, OTP detection, read-only refresh, unavailable vs empty, confirmed OTP clean stops on denial/mismatch without retry, logout fencing, CSP-safe markup');
