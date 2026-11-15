// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
if(!globalThis.crypto)Object.defineProperty(globalThis,'crypto',{value:webcrypto});
const source=await readFile(new URL('../crates/edge/src/web/network.js',import.meta.url));
const {bytes,rate,policyInput,radioRows,NetworkPanel}=await import('data:text/javascript;base64,'+source.toString('base64'));
assert.equal(bytes(1e9),'1.00 GB');assert.equal(bytes(-1),'暂不可用');assert.match(rate(125000),/1.00 Mbps/);
assert.deepEqual(policyInput(true,'1.5','31'),{enabled:true,limit_bytes:1500000000,reset_day:31});
for(const values of [['',1],['NaN',1],['Infinity',1],[1,0],[1,32],[1,1.5],[100001,1]])assert.throws(()=>policyInput(true,...values));
const rows=radioRows({configured_bands:['eutran-1','eutran-3'],serving_cell:{}});assert.equal(rows.find(r=>r[0]==='当前驻留频段')[1],null);assert.equal(rows.find(r=>r[0]==='当前带宽')[1],'设备暂未提供');
class Element {constructor(){this.value='';this.checked=false;this.children=[];this.textContent='';}addEventListener(){}replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}get options(){return this.children;}getContext(){return null;}}
function setup(request){const map=new Map();const doc={hidden:false,getElementById:id=>{if(!map.has(id))map.set(id,new Element());return map.get(id);},createElement:()=>new Element()};return {panel:new NetworkPanel({document:doc,request,getToken:()=>true,runTest:()=>{throw Error('must not run automatically');}}),doc,map};}
const turn=()=>new Promise(r=>setImmediate(r));
{
 let finish;const {panel,map}=setup(()=>new Promise(r=>{finish=r;}));panel.start();await turn();panel.stop();finish({available:true,devices:[{interface:'private-address'}]});await turn();assert.equal(map.get('network-devices').children.length,0);assert.equal(panel.sample,null);assert.equal(panel.running,false);
}
{
 const seen=[];let ack='';const sample={available:true,sample_unix_seconds:1,devices:[],policy:{enabled:false,limit_bytes:1e9,reset_day:1},usage:{},last_request_id:''};
 const {panel,doc}=setup(async c=>{seen.push(c);if(c.op==='set_network_policy')return {request_id:c.request_id,state:'pending'};return {...sample,last_request_id:ack};});panel.running=true;panel.sample=sample;
 doc.getElementById('network-limit-enabled').checked=true;doc.getElementById('network-limit-gb').value='2';doc.getElementById('network-reset-day').value='15';
 await panel.savePolicy();const id=panel.pending;assert.ok(id);await panel.savePolicy();assert.equal(seen.filter(c=>c.op==='set_network_policy').length,1);ack=id;await panel.refresh();assert.equal(panel.pending,null);panel.stop();
}
{
 let calls=0;const {panel,doc}=setup(async()=>{calls++;});doc.hidden=true;panel.start();await turn();assert.equal(calls,0);panel.stop();
}
assert.ok(!source.toString().includes('innerHTML'));
console.log('PASS: network units/input, radio unknowns, logout late-response isolation, pending-policy no-repeat, hidden-page no-poll, no automatic data tests');

// B-05/B-06: exercise the actual renderer with synthetic device DTOs.
const rowMap=rows=>Object.fromEntries(rows);
for(const input of [null,[],{serving_cell:null},{serving_cell:{channel_derived:{downlink_mhz:'1805',uplink_mhz:{}}}},
 {signal_percent:NaN,serving_cell:{rsrp:'-95',rsrq:Infinity,sinr:[],bandwidth_mhz:-1,earfcn:-1}}]){
 assert.doesNotThrow(()=>radioRows(input),'missing/malformed radio data must not crash the whole panel');
 const values=Object.values(rowMap(radioRows(input))).map(String).join(' ');
 assert.doesNotMatch(values,/NaN|Infinity|undefined|\[object Object\]/);
}
{
 const valid=rowMap(radioRows({operator:'<img src=x onerror=alert(1)>',registration:'future-state',signal_percent:0,
  access_technologies:['lte'],configured_bands:['eutran-1'],serving_cell:{cell_id:'00af',tracking_area_code:'0',physical_cell_id:'0',earfcn:0,rsrp:-95.5,rsrq:-10,sinr:0,
  channel_derived:{band:3,downlink_mhz:1815,uplink_mhz:1720}}}));
 assert.equal(valid['信号质量'],'0%');assert.match(valid['下行频率'],/1815.0 MHz（按信道计算）/);
 assert.equal(valid['RSRP'],'-95.5 dBm');assert.equal(valid['SINR'],'0.0 dB');
 assert.equal(valid['小区 ID'],'0x00af');assert.equal(valid['注册状态'],'future-state');
 assert.equal(rowMap(radioRows({signal_percent:101}))['信号质量'],null);
 assert.equal(rowMap(radioRows({serving_cell:{bandwidth_mhz:-1}}))['当前带宽'],'设备暂未提供');
 assert.doesNotMatch(rate(Number.MAX_VALUE),/Infinity/);
 assert.equal(bytes(1024),'1.02 KB');assert.match(rate(0),/^0.00 Mbps/);
 for(const invalid of [NaN,Infinity,-Infinity,-1,null,'10',undefined]){
  assert.equal(bytes(invalid),'暂不可用');assert.equal(rate(invalid),'等待下一次采样');
 }
}
function dlValues(element){const result={};for(let i=0;i<element.children.length;i+=2)result[element.children[i].textContent]=element.children[i+1].textContent;return result;}
const displaySample=()=>({available:true,sample_unix_seconds:100,boot_id:'synthetic-boot',devices:[],cellular:{},
 policy:{enabled:false,limit_bytes:1e9,reset_day:1},usage:{uncertain:false,rx_bytes:100,tx_bytes:20,total_bytes:120,remaining_bytes:999999880}});
{
 const {panel,doc}=setup(async()=>displaySample());
 const s=displaySample();s.devices=[null,{}, {kind:'wifi',interface:'wlan0',state:'100 connected',ipv4:['192.0.2.1'],ipv6:['2001:db8::1'],dns_ipv4:['192.0.2.53'],dns_ipv6:'bad'},
  {kind:'cellular',interface:'wwan0',state:'disconnected',apn:'<svg onload=alert(1)>',dns_ipv4:{}}];
 assert.doesNotThrow(()=>panel.render(s));
 const articles=doc.getElementById('network-devices').children;
 assert.equal(articles.length,3);
 const wifi=dlValues(articles[1].children[1]);assert.equal(wifi.IPv4,'192.0.2.1');assert.equal(wifi.IPv6,'2001:db8::1');assert.equal(wifi.DNS,'192.0.2.53');
 assert.equal(dlValues(articles[2].children[1]).APN,'<svg onload=alert(1)>');
 assert.equal(doc.getElementById('network-interface').children.length,1);
 const uncertain={...displaySample(),usage:{uncertain:true,remaining_bytes:1e9}};panel.render(uncertain);
 assert.match(dlValues(doc.getElementById('network-month'))['剩余额度'],/不确定/);
 panel.render({...displaySample(),usage:{remaining_bytes:1e9},policy:null});
 const missing=dlValues(doc.getElementById('network-month'));
 assert.equal(missing['计量状态'],'暂不可用');assert.equal(missing['到量断开数据'],'暂不可用');
 assert.equal(missing['本月合计'],'暂不可用');assert.equal(missing['剩余额度'],'暂不可用');
 panel.stop();
}
{
 const {panel,doc}=setup(async()=>displaySample());
 const coords=[];const context={clearRect(){},beginPath(){},moveTo(x,y){coords.push([x,y]);},lineTo(x,y){coords.push([x,y]);},stroke(){}};
 const canvas=doc.getElementById('network-chart');canvas.width=720;canvas.height=120;canvas.getContext=()=>context;
 for(const bad of [undefined,null,NaN,Infinity,-1,'10'])panel.render({...displaySample(),rx_bytes_per_second:bad,tx_bytes_per_second:bad});
 assert.ok(panel.history.every(pair=>pair.every(n=>n===null)),'missing samples are chart gaps, not fabricated zero traffic');
 panel.render({...displaySample(),rx_bytes_per_second:125000,tx_bytes_per_second:0});
 assert.match(doc.getElementById('network-rates').textContent,/1.00 Mbps/);
 panel.render({...displaySample(),rx_bytes_per_second:Number.MAX_VALUE,tx_bytes_per_second:0});
 assert.ok(coords.flat().every(Number.isFinite));
 panel.stop();assert.equal(panel.history.length,0);
}
{
 let fail=false;const {panel,doc}=setup(async()=>{if(fail)throw Error('synthetic stale snapshot');return {...displaySample(),devices:[{kind:'wifi',interface:'wlan0',state:'100 connected',ipv4:['192.0.2.1'],dns_ipv4:['192.0.2.53']}],rx_bytes_per_second:10,tx_bytes_per_second:20};});
 panel.running=true;await panel.refresh();assert.ok(panel.history.length);assert.equal(doc.getElementById('network-interface').children.length,1);
 fail=true;await panel.refresh();assert.equal(panel.sample,null);assert.equal(panel.history.length,0);assert.equal(doc.getElementById('network-interface').children.length,0);
 assert.match(doc.getElementById('network-rates').textContent,/过期/);assert.equal(doc.getElementById('network-policy-save').disabled,true);panel.stop();
}
console.log('PASS: B-05/B-06 malformed/zero radio fields, finite rates, text-only DTO rendering, unknown quotas, chart gaps and stale-state cleanup');

{
 // A completed old test must not re-enable controls after logout.
 const {panel,doc}=setup(async()=>displaySample());let finish;
 panel.running=true;panel.sample=displaySample();doc.getElementById('network-interface').value='wlan0';
 panel.runTest=()=>new Promise(resolve=>finish=resolve);
 const old=panel.test('connectivity');assert.equal(doc.getElementById('network-test-connectivity').disabled,true);
 doc.getElementById('network-cellular-consent').checked=true;
 panel.stop();finish({ok:false,reason:'SYNTHETIC_OLD_TEST'});await old;
 assert.equal(doc.getElementById('network-test-connectivity').disabled,true,'late test completion must not enable logged-out controls');
 assert.equal(doc.getElementById('network-test-download').disabled,true);assert.equal(doc.getElementById('network-test-result').textContent,'');
 assert.equal(doc.getElementById('network-cellular-consent').checked,false,'logout clears the old session cellular-test consent');
}
{
 // Stop/start opens a new read slot. The old finally must not release it.
 const requests=[],{panel}=setup(()=>new Promise(resolve=>requests.push(resolve)));
 panel.running=true;const old=panel.refresh();assert.equal(requests.length,1);
 panel.stop();panel.start();assert.equal(requests.length,2,'new session must not wait for an old in-flight read');
 requests[0]({...displaySample(),boot_id:'old'});await old;
 assert.equal(panel.loading,true,'old read must not release the new session read slot');assert.equal(panel.sample,null);
 requests[1]({...displaySample(),boot_id:'new'});await turn();assert.equal(panel.sample.boot_id,'new');panel.stop();
}
{
 // An old network-test result must not release a newer test's busy state.
 const {panel,doc}=setup(async()=>displaySample()),tests=[];
 panel.runTest=()=>new Promise(resolve=>tests.push(resolve));
 panel.running=true;panel.sample=displaySample();doc.getElementById('network-interface').value='wlan0';const old=panel.test('connectivity');
 panel.stop();panel.start();await turn();doc.getElementById('network-interface').value='wlan0';
 const next=panel.test('connectivity');assert.equal(tests.length,2,'new session may start its own explicit test');
 tests[0]({ok:false,reason:'SYNTHETIC_OLD_TEST'});await old;
 assert.equal(panel.testing,true,'old completion must not release the new test');assert.equal(doc.getElementById('network-test-connectivity').disabled,true);
 assert.doesNotMatch(doc.getElementById('network-test-result').textContent,/SYNTHETIC_OLD_TEST/);
 tests[1]({ok:false,reason:'SYNTHETIC_CURRENT_TEST'});await next;
 assert.equal(panel.testing,false);assert.match(doc.getElementById('network-test-result').textContent,/SYNTHETIC_CURRENT_TEST/);panel.stop();
}
console.log('PASS: network logout/read/test epoch fences preserve disabled controls and new-session busy slots');
