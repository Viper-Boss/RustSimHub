// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';
const code=await readFile(new URL('../crates/edge/src/web/configuration-state.js',import.meta.url));const {configurationText,ConfigurationState}=await import('data:text/javascript;base64,'+code.toString('base64'));
assert.match(configurationText('forwarding',{configuration_applied:true,enabled:true,state:'running',process_state_verified:false},0),/保存意图已确认；服务进程状态未确认/);
assert.match(configurationText('forwarding',{enabled:false,state:'disabled',process_state_verified:true},0),/服务关闭已核对/);
for(const age of [null,NaN,-1,5001])assert.match(configurationText('forwarding',{configuration_applied:true},age),/陈旧或未知/);
assert.match(configurationText('ap-commit',{state:'saved',settings_saved:true,authoritative_result:false},0),/未确认/);
assert.match(configurationText('ap-trial',{state:'ready',settings_saved:false,authoritative_result:true,ap_return_verified:true},0),/尚未保存/);
class Element{constructor(){this.style={};}setAttribute(){}replaceChildren(){}set innerHTML(_){throw Error('no HTML');}}
{let now=0,calls=0;const p=new ConfigurationState({document:{createElement:()=>new Element()},root:new Element(),clock:()=>now,read:async()=>{calls++;return {source:'forwarding',data:{configuration_applied:true,enabled:true,state:'running',process_state_verified:true},age_ms:0};}});assert.equal(calls,0);await p.refresh();assert.match(p.output.textContent,/服务运行已核对/);now=6000;p.render();assert.match(p.output.textContent,/陈旧/);assert.equal(calls,1);p.reset();}
{let finish;const p=new ConfigurationState({document:{createElement:()=>new Element()},root:new Element(),read:()=>new Promise(r=>{finish=r;})});const query=p.refresh();p.reset();finish({source:'forwarding',data:{configuration_applied:true},age_ms:0});await query;assert.match(p.output.textContent,/等待授权/);p.reset();}
console.log('PASS: source-specific save/process evidence, authoritative AP results, monotonic staleness, no polling and reset fencing');
