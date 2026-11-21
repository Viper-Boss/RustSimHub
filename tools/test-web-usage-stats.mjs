// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';
const code=await readFile(new URL('../crates/edge/src/web/usage-stats.js',import.meta.url));const {usageRows,UsageStatsPanel,bindUsageCounts}=await import('data:text/javascript;base64,'+code.toString('base64'));
const good=()=>({status:'success',entry:{window:'24h',count:42,source:'read_only_transport',observed_at_secs:1793203200,succeeded_at_secs:1793203201,expires_at_secs:1793203801,freshness_revoked:false}});
assert.match(JSON.stringify(usageRows(good())),/42/);assert.match(JSON.stringify(usageRows({status:'disabled',entry:good().entry})),/已关闭/);assert.doesNotMatch(JSON.stringify(usageRows({status:'disabled',entry:good().entry})),/42/);
for(const change of [{count:9007199254740992},{count:'18446744073709551615'},{count:NaN},{source:'mock'},{freshness_revoked:true}])assert.match(JSON.stringify(usageRows({...good(),entry:{...good().entry,...change}})),/未知|未确认/);
assert.match(JSON.stringify(usageRows({...good(),status:'cached',freshness:'expired'})),/缓存已过期/);assert.match(JSON.stringify(usageRows(good(),'30d')),/窗口/);
assert.doesNotMatch(JSON.stringify(usageRows({...good(),key:'SECRET',peer_count:99,entry:{...good().entry,token:'SECRET'}})),/SECRET|99/);
class Element{constructor(){this.style={};this.children=[];this.value='';}setAttribute(){}replaceChildren(...v){this.children=v;}append(...v){this.children.push(...v);}set innerHTML(_){throw Error('text only');}}
const doc={createElement:()=>new Element()};
{let now=0,calls=0;const panel=new UsageStatsPanel({document:doc,root:new Element(),clock:()=>now,read:async()=>{calls++;return good();}});assert.equal(calls,0);await panel.refresh();await panel.refresh();assert.equal(calls,1);now=5000;await panel.refresh();assert.equal(calls,2);panel.reset();}
{let resolve;const panel=new UsageStatsPanel({document:doc,root:new Element(),read:()=>new Promise(r=>{resolve=r;})});const reading=panel.refresh();panel.reset();resolve(good());await reading;assert.match(panel.state.textContent,/仅点击/);}
assert.ok(!code.toString().includes('fetch('));console.log('PASS: actual A QueryOutcome projection, windows/timestamps, disabled/cache/unknown, unsafe-u64 rejection, no engine/peer-count inference, manual reads and reset fencing');

{
 const button = { addEventListener(type, listener) { assert.equal(type, 'click'); this.click = listener; } };
 const status = {};
 const cells = [{textContent:'—'}, {textContent:'—'}, {textContent:'—'}];
 let calls = 0;
 const replies = [
  {ok:true, json:async()=>({source:'domestic', counts:[0,7,9]})},
  {ok:true, json:async()=>({source:'official', counts:[0,8,10]})},
  {ok:true, json:async()=>({source:'official', counts:[0,-1,10]})},
  {ok:false},
 ];
 bindUsageCounts({
  querySelector: selector => selector === '[data-refresh]' ? button : status,
  querySelectorAll: () => cells,
 }, async signal => {
  assert.ok(signal instanceof AbortSignal);
  return replies[calls++];
 });
 assert.equal(calls, 0);
 await button.click();
 assert.deepEqual(cells.map(cell => cell.textContent), ['0','7','9']);
 assert.match(status.textContent, /国内节点/);
 await button.click();
 assert.match(status.textContent, /官方主网兜底/);
 await button.click();
 assert.match(status.textContent, /刷新失败/);
 await button.click();
 assert.deepEqual(cells.map(cell => cell.textContent), ['0','8','10']);
 assert.equal(button.disabled, false);
}
