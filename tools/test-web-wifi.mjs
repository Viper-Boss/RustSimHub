// SPDX-License-Identifier: AGPL-3.0-only
// Production app catalogue and trial handling; synthetic IPC and DOM only.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
import vm from 'node:vm';
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]);assert.equal(new Set(ids).size,ids.length);
for(const match of app.matchAll(/\$\('(wifi-[^']+)'\)/g))assert(ids.includes(match[1]),match[1]+' missing from HTML');
assert(html.indexOf('id="wifi-section"')<html.indexOf('<summary>一键通话测试'));
assert(app.includes("bind('wifi-login',login)")&&app.includes("bind('wifi-networks-refresh',loadWifiNetworks)"));
const clone=value=>structuredClone(value);
const row={ssid:'合成 <img src=x>',strength:80,frequency_mhz:2412,security:'wpa2_personal',selectable:true,connected:true};
const catalogue={schema:1,source:'networkmanager_cache',scan_requested:false,hardware_mutations:false,wifi_enabled:true,scan_cache_known:true,omitted:0,truncated:false,networks:[row]};
const trial={trial_id:'a'.repeat(64),state:'ready',expires_in_seconds:30,wifi_only:true,requires_confirmation:true,credentials_returned:false,cellular_activation:false};
const turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element{
 constructor(){this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;this.focused=false;}
 set innerHTML(_){throw Error('Network names must not be parsed as HTML');}
 append(...values){this.children.push(...values);}replaceChildren(...values){this.children=values;this.textContent='';}addEventListener(){}focus(){this.focused=true;}
}
function fixture(respond=()=>clone(catalogue)){
 const elements=new Map(),sent=[];
 const $=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const context={document:{hidden:false,getElementById:$,createElement:()=>new Element(),querySelectorAll:()=>[],addEventListener(){}},crypto:webcrypto,TextEncoder,AbortController,AbortSignal,setTimeout,clearTimeout,setInterval(){},addEventListener(){},indexedDB:{open:()=>({})},
  AudioLink:class{constructor(){this.state='closed';}async stop(){}},ConfigurationIndicator:class{async set(){}},CallTest:class{async stop(){}},NetworkPanel:class{start(){}stop(){}},EventSync:class{reset(){}},SmsManager:class{reset(){}},DailyPhone:class{reset(){}},SessionKeeper:class{reset(){}},
  fetch:async(_url,options)=>{const body=JSON.parse(options.body),command=body.command;sent.push(command);const result=await respond(command,sent.length);return {ok:true,json:async()=>({protocol:2,id:body.id,result})};},
 };
 vm.runInNewContext(app.replace(/^import .*;\r?$/gm,'')+'\nglobalThis.fixture={loadWifiNetworks,clearWifiNetworks,showWifi,wifiAction,loginAs:()=>{token="a".repeat(64);},trial:()=>wifiTrial,logout:()=>$("logout").onclick()};',context);
 context.fixture.loginAs();
 return {...context.fixture,$,sent};
}
{
 const f=fixture();await f.$('wifi-networks-refresh').onclick();
 assert.deepEqual(f.sent.map(c=>c.op),['wifi_networks']);assert.equal(f.$('wifi-networks').children.length,1);
 const item=f.$('wifi-networks').children[0];assert.match(item.children[0].textContent,/<img src=x>/);assert.match(item.children[0].textContent,/当前连接/);
 f.$('wifi-passphrase').value='old-private-password';await item.children[1].onclick();
 assert.equal(f.$('wifi-ssid').value,row.ssid);assert.equal(f.$('wifi-passphrase').value,'');assert(f.$('wifi-passphrase').focused);
 assert.match(f.$('wifi-networks-state').textContent,/尚未切换/);assert.equal(f.sent.length,1);
}
for(const security of ['open','unsupported']){
 const f=fixture(()=>({...clone(catalogue),networks:[{...row,security,selectable:false}]}));await f.loadWifiNetworks();
 const button=f.$('wifi-networks').children[0].children[1];assert(button.disabled);await button.onclick();assert.equal(f.$('wifi-ssid').value,'');assert.equal(f.sent.length,1);
}
for(const bad of [
 {...clone(catalogue),networks:Array.from({length:33},()=>clone(row))},
 {...clone(catalogue),networks:[row,{...row,strength:101}]},
 {...clone(catalogue),networks:[{...row,ssid:'bad\0name'}]},
 {...clone(catalogue),networks:[{...row,ssid:'中'.repeat(11)}]},
 {...clone(catalogue),networks:[{...row,security:'open',selectable:true}]},
 {...clone(catalogue),scan_requested:true},
 {...clone(catalogue),source:'new_scan'},
]){
 const f=fixture(()=>bad);await assert.rejects(f.loadWifiNetworks());assert.equal(f.$('wifi-networks').children.length,0);assert.match(f.$('wifi-networks-state').textContent,/未能读取/);
}
for(const fields of [{wifi_enabled:false,scan_cache_known:false},{wifi_enabled:true,scan_cache_known:false},{wifi_enabled:true,scan_cache_known:true}]){
 const f=fixture(()=>({...clone(catalogue),...fields,networks:[]}));await f.loadWifiNetworks();assert.equal(f.$('wifi-networks').children.length,0);assert.equal(f.sent.length,1);assert.equal(f.$('wifi-networks-refresh').disabled,false);
}
{
 const wait=deferred(),f=fixture(()=>wait.promise);const reading=f.loadWifiNetworks();await turn();f.logout();wait.resolve(clone(catalogue));await assert.rejects(reading);
 assert.equal(f.$('wifi-networks').children.length,0);assert.equal(f.$('wifi-ssid').value,'');assert.match(f.$('wifi-networks-state').textContent,/点击读取/);
}
{
 const f=fixture();await f.loadWifiNetworks();const button=f.$('wifi-networks').children[0].children[1];f.clearWifiNetworks();await button.onclick();assert.equal(f.$('wifi-ssid').value,'');
}
{
 const wait=deferred(),f=fixture((_c,n)=>n===1?wait.promise:{...clone(catalogue),networks:[{...row,ssid:'new-synthetic'}]});
 const old=f.loadWifiNetworks();await turn();await f.loadWifiNetworks();wait.resolve(clone(catalogue));await old;
 assert.match(f.$('wifi-networks').children[0].children[0].textContent,/new-synthetic/);
}
{
 let denied=false;const f=fixture(()=>{if(denied)throw Error('denied');return clone(catalogue);});await f.loadWifiNetworks();denied=true;
 await assert.rejects(f.loadWifiNetworks());assert.equal(f.$('wifi-networks').children.length,0);
}
{
 const f=fixture(()=>{throw Error('query failed');});f.showWifi(clone(trial));await assert.rejects(f.wifiAction('wifi_status'));
 assert.equal(f.trial().trial_id,trial.trial_id);assert.equal(f.$('wifi-confirm').disabled,true);assert.equal(f.$('wifi-status').disabled,false);assert.match(f.$('wifi-state').textContent,/保留试连接编号/);
 assert.deepEqual(f.sent.map(c=>c.op),['wifi_status']);
}
{
 const f=fixture();f.showWifi(clone(trial));await f.loadWifiNetworks();await f.$('wifi-networks').children[0].children[1].onclick();assert.equal(f.$('wifi-ssid').value,'');assert.equal(f.trial().trial_id,trial.trial_id);
}
console.log('Web Wi-Fi catalogue, selection, session fencing and retained trial query passed');
