// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';
const code=await readFile(new URL('../crates/edge/src/web/operation-receipt.js',import.meta.url));const {OperationReceipt}=await import('data:text/javascript;base64,'+code.toString('base64'));
const id='11111111-1111-4111-8111-111111111111';
class Element{constructor(){this.style={};this.children=[];this.textContent='';}setAttribute(){}replaceChildren(...v){this.children=v;}set innerHTML(_){throw Error('no HTML');}}
const fixture=c=>new OperationReceipt({document:{createElement:()=>new Element()},root:new Element(),...c});
{const calls=[];const p=fixture({query:async v=>{calls.push(v);},archive:async()=>{throw Error('must not archive');}});p.update({id,state:'unknown',submit_started:true,canQuery:true,canArchive:false,token:'SECRET',text:'SECRET'});assert.equal(calls.length,0);assert.match(p.text.textContent,/结果不明/);assert.doesNotMatch(p.text.textContent,/SECRET/);await p.run('archive');assert.equal(calls.length,0);await p.run('query');assert.deepEqual(calls,[id]);assert.equal(p.view.id,id);p.reset();assert.ok(p.buttons.query.disabled);}
{let finish,count=0;const p=fixture({query:async(_id,scope)=>{count++;await new Promise(r=>{finish=r;});scope.update({id,state:'submitted',canQuery:true});}});p.update({id,state:'unknown',canQuery:true});const run=p.run('query');await p.run('query');assert.equal(count,1);p.reset();finish();await run;assert.equal(p.view,null);}
{let count=0;const p=fixture({archive:async v=>{assert.equal(v,id);count++;}});p.update({id,state:'unknown',submit_started:true,canArchive:true});assert.equal(count,0);await p.run('archive');assert.equal(count,1);assert.equal(p.view.id,id);}
for(const [failure,label] of [['expired','登录已过期'],['denied','权限被拒绝'],['unconfirmed','设备结果尚未确认']]){const p=fixture({});p.update({id,state:'unknown',failure});assert.ok(p.text.textContent.includes(label));}
assert.ok(!code.toString().includes('fetch('));console.log('PASS: receipt exact ID/submit marker, explicit host capabilities, no render submissions or auto archive, query/reset fencing');
