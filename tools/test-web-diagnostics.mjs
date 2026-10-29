// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../crates/edge/src/web/diagnostics.js',import.meta.url));
const {diagnosticSummary,diagnosticText,DiagnosticPanel}=await import('data:text/javascript;base64,'+source.toString('base64'));
const id='11111111-1111-4111-8111-111111111111';
const snapshot=()=>({origin:'https://device.local:8443',device_pin:'a'.repeat(64),certificate:'trusted',session:'expired',business:'unknown',remote_path:'relay',operation:{id,type:'call',state:'unknown',submit_started:true}});
for(const value of [null,{}, {origin:'https://secret:password@device.local/?token=secret'}, {origin:'javascript:alert(1)'}, {session:'<img onerror=alert(1)>'}]){
 const s=diagnosticSummary(value);assert.equal(s.business,'unknown');assert.doesNotMatch(JSON.stringify(s),/secret|password|token|onerror|device\.local/);
}
const good=diagnosticSummary(snapshot());assert.equal(good.address.scheme,'https');assert.equal(good.device_pin_present,true);assert.equal(good.business,'unknown');assert.equal(good.session,'expired');
assert.doesNotMatch(diagnosticText({...snapshot(),ssid:'MUST-NOT-COPY',passphrase:'MUST-NOT-COPY',token:'MUST-NOT-COPY',operation:{...snapshot().operation,text:'MUST-NOT-COPY',lease:'MUST-NOT-COPY'}}),/MUST-NOT-COPY|aaaaaaaa|device\.local/);
class Element{constructor(){this.style={};this.children=[];this.textContent='';}setAttribute(){}replaceChildren(...v){this.children=v;}set innerHTML(_){throw Error('Never parse diagnostics as HTML');}}
function fixture(query){const root=new Element(),copies=[];return {copies,panel:new DiagnosticPanel({document:{createElement:()=>new Element()},root,queryOperation:query,copy:async text=>{copies.push(text);}})};}
{
 const calls=[];const {panel,copies}=fixture(async(idArg)=>{calls.push(idArg);return snapshot();});panel.update(snapshot());assert.equal(calls.length,0);await panel.query();assert.deepEqual(calls,[id]);assert.equal(panel.summary.operation.id,id);await panel.copyResult();assert.equal(copies.length,1);assert.doesNotMatch(copies[0],/device\.local|aaaaaaaa/);panel.reset();assert.equal(panel.queryButton.disabled,true);
}
{
 let resolve;const calls=[];const {panel}=fixture(async value=>{calls.push(value);return new Promise(r=>{resolve=r;});});panel.update(snapshot());const query=panel.query();await panel.query();assert.equal(calls.length,1);panel.reset();resolve(snapshot());await query;assert.equal(panel.summary,null);assert.match(panel.output.textContent,/等待授权/);
}
{
 const {panel}=fixture(async()=>({...snapshot(),operation:{id:'22222222-2222-4222-8222-222222222222'}}));panel.update(snapshot());await panel.query();assert.equal(panel.summary.operation.id,id);assert.match(panel.message.textContent,/未确认/);
}
assert.ok(!source.toString().includes('fetch('));assert.ok(!source.toString().includes('innerHTML'));
console.log('PASS: redacted address/pin and credential-free diagnostics, independent TLS/session/business states, exact original-ID queries, duplicate/reset/mismatch fencing; no submit or network discovery');
