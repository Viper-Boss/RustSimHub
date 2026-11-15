// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../crates/edge/src/web/usage-stats.js',import.meta.url));
const {UsageParticipationPanel,participationPending,participationStatus,participationArchive}=await import('data:text/javascript;base64,'+source.toString('base64'));
const id='11111111-1111-4111-8111-111111111111',ctx='a'.repeat(64),next='b'.repeat(64);
const turn=()=>new Promise(r=>setImmediate(r));
class Element {constructor(){this.children=[];this.style={};this.checked=false;}setAttribute(){}append(...v){this.children.push(...v);}replaceChildren(...v){this.children=v;}set innerHTML(_){throw Error('text only');}}
function fixture(){
 let pending=null,token='owner',generation=0,enabled=false,instance=ctx;const calls=[],archives=[];
 const status=()=>({format:1,available:true,core_instance:instance,generation:String(generation),enabled,disclosure_version:enabled?1:null,disabled_generation:enabled?'0':generation?'1':'0',owner_current:enabled,record_exists:generation>0,reporting_available:false,runtime:enabled?'authorized_not_connected':'disabled'});
 const f={calls,archives,status,confirm:true,failWrite:false,unknown:false,absent:false,loginGate:null,get pending(){return pending;},set pending(v){pending=v;},token(v){token=v;},instance(v){instance=v;},setServer(g,on){generation=g;enabled=on;}};
 let saved=null;
 f.panel=new UsageParticipationPanel({document:{createElement:()=>new Element()},root:new Element(),getToken:()=>token,login:async()=>{if(f.loginGate)await f.loginGate;},confirm:()=>f.confirm,crypto:{randomUUID:()=>id},loadPending:async()=>pending,
 savePending:async(value,expected)=>{assert.equal(pending?.id??null,expected);if(f.failWrite)throw Error('PRIVATE_STORAGE');pending=participationPending(value);},
 archivePending:async entry=>{archives.push(participationArchive(entry));},request:async c=>{
  calls.push(c);if(c.op==='usage_participation_status')return status();
  if(c.op==='usage_participation_set'){assert.equal(c.expected_generation,String(generation));generation++;enabled=c.enabled;saved={request_id:c.request_id,known:true,state:'saved',core_instance:c.expected_core_instance,current_core_instance:instance,enabled,disclosure_version:enabled?1:null,expected_generation:c.expected_generation,generation:String(generation),not_found_is_not_proof:true,reporting_available:false};if(f.unknown)throw Error('PRIVATE_NETWORK');return saved;}
  assert.equal(c.op,'usage_participation_request');return f.absent?{request_id:c.request_id,known:false,current_core_instance:instance,not_found_is_not_proof:true,reporting_available:false}:{...saved,current_core_instance:instance};
 }});return f;
}
const sent=f=>f.calls.filter(c=>c.op==='usage_participation_set');
{
 const f=fixture();assert.equal(f.calls.length,0);assert.equal(f.panel.consent.checked,false);f.panel.consent.checked=true;await f.panel.save();
 assert.equal(sent(f).length,1);assert.equal(sent(f)[0].enabled,true);assert.equal(sent(f)[0].disclosure_version,1);assert.equal(f.pending,null);assert.equal(f.archives.length,1);assert.match(f.panel.state.textContent,/未开始上报/);
 f.panel.consent.checked=false;f.panel.crypto={randomUUID:()=>id.replace(/^1/,'2')};await f.panel.save();assert.equal(sent(f).length,2);assert.equal(sent(f)[1].enabled,false);assert.equal(sent(f)[1].disclosure_version,null);assert.match(f.panel.state.textContent,/不参与/);
 assert(f.calls.every(c=>c.op.startsWith('usage_participation_')),'No query/Root/SDK/sign/chain operation');
}
for(const mode of ['cancel','storage']){
 const f=fixture();f.panel.consent.checked=true;if(mode==='cancel')f.confirm=false;else f.failWrite=true;await f.panel.save();assert.equal(sent(f).length,0);assert.equal(f.pending,null);assert.doesNotMatch(f.panel.state.textContent,/PRIVATE/);
}
{
 const f=fixture();f.unknown=true;f.panel.consent.checked=true;await f.panel.save();assert.equal(sent(f).length,1);assert(f.pending);assert.match(f.panel.state.textContent,/未确认/);await f.panel.save();assert.equal(sent(f).length,1);
 f.instance(next);await f.panel.action(g=>f.panel.query(g));assert.equal(sent(f).length,1);assert.equal(f.pending,null);assert.equal(f.archives[0].record.core_instance,ctx,'Cross-restart lookup binds the original instance');
}
{
 const f=fixture();f.unknown=true;f.panel.consent.checked=true;await f.panel.save();f.absent=true;await f.panel.action(g=>f.panel.query(g));assert.match(f.panel.state.textContent,/不证明未执行/);assert(f.pending);assert.equal(sent(f).length,1);
 await f.panel.action(g=>f.panel.archive(g));assert.equal(f.pending,null);assert.equal(f.archives[0].state,'unknown');assert.equal(sent(f).length,1);
}
{
 const f=fixture();f.pending={secret:'PRIVATE_BAD'};await f.panel.loginRead();assert.equal(f.calls.length,0);assert.doesNotMatch(f.panel.state.textContent,/PRIVATE_BAD/);
}
{
 const f=fixture();let resolve;f.loginGate=new Promise(r=>resolve=r);f.panel.consent.checked=true;const first=f.panel.save();await f.panel.save();resolve();await first;assert.equal(sent(f).length,1,'Double click during login reserves once');
}
{
 const f=fixture();let resolve;f.loginGate=new Promise(r=>resolve=r);const pending=f.panel.save();f.panel.reset();resolve();await pending;assert.equal(f.calls.length,0,'Reset during login cannot start a delayed permission save');
}
{
 const f=fixture();let resolve;const original=f.panel.request;f.panel.request=async c=>{const reply=await original(c);return c.op==='usage_participation_set'?await new Promise(r=>resolve=()=>r(reply)):reply;};
 f.panel.consent.checked=true;const saving=f.panel.save();await turn();assert.equal(sent(f).length,1);f.token(null);f.panel.reset();resolve();await saving;
 assert(f.pending);assert.equal(f.archives.length,0);assert.match(f.panel.state.textContent,/默认不参与/);assert.equal(f.panel.buttons.query.disabled,true);
}
{
 const f=fixture();const status=f.status();for(const extra of [{generation:0},{generation:'00'},{reporting_available:true},{runtime:'running'},{token:'PRIVATE'},{disabled_generation:'1'}])assert.throws(()=>participationStatus({...status,...extra}));
 for(const extra of [{enabled:1},{disclosure_version:2},{expected_generation:'00'},{seed:'PRIVATE'}])assert.throws(()=>participationPending({format:1,id,enabled:true,disclosure_version:1,expected_generation:'0',core_instance:ctx,...extra}));
}
console.log('PASS: explicit participation permission, default OFF/no IO, one save after fresh login, durable CAS before submit, storage/cancel failures, exact restart receipt, Unknown/no resend, archive and logout/login races; no signing or network activation');
