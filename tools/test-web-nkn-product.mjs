// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const url=source=>'data:text/javascript;base64,'+Buffer.from(source).toString('base64');
const status=url(await readFile(new URL('../crates/edge/src/web/nkn-status.js',import.meta.url)));
const source=await readFile(new URL('../crates/edge/src/web/nkn-product.js',import.meta.url),'utf8');
const {NknProductPanel,pendingInput,reportInput,archiveInput}=await import(url(source.replace("'./nkn-status.js'",JSON.stringify(status))));
const ctx='a'.repeat(64),id='11111111-1111-4111-8111-111111111111';
const report=(sequence=1)=>({available:true,configuration_api:true,remote_control_supported:false,revision:'0',provider:{contract:1,core_instance:ctx,owners:{manual:false,remote:false,statistics:false},supervisor_state:'ready',root_generation:1,original_process_exit_verified:false,status:{contract:1,source:'engine',generation:2,sequence,age_ms:20,enabled:'on',service:'running',path:'unknown',dht:'unknown',rx_bytes:null,tx_bytes:null,rtt_ms:null,peer_count:null,last_error:null}}});
const record=()=>({format:1,id,owner:'manual_engine',enabled:true,expected_revision:'0',core_instance:ctx});
class Element{constructor(){this.style={};this.disabled=false;this.textContent='';}setAttribute(){}replaceChildren(...v){this.children=v;}append(v){this.children.push(v);}set innerHTML(_){throw Error('No markup from data');}}
function fixture(){let pending=null,token='current',sequence=0;const calls=[],archives=[],clock={mono:10,wall:100};
 const f={calls,archives,clock,failRead:false,failWrite:false,readback:true,result:'accepted',get pending(){return pending;},set pending(v){pending=v;},token(v){token=v;}};
 f.panel=new NknProductPanel({document:{createElement:()=>new Element()},root:new Element(),getToken:()=>token,confirm:()=>true,crypto:{randomUUID:()=>id},clock:()=>clock.mono,wallClock:()=>clock.wall,
  request:async c=>{calls.push(c);if(c.op==='nkn_status'){if(f.failRead)throw Error('SYNTHETIC_PRIVATE');return report(++sequence);}if(c.op==='nkn_request'&&f.result==='absent')return {request_id:id,known:false,not_found_is_not_proof:true};return {request_id:id,known:true,state:f.result,core_instance:ctx,owner:'manual_engine',enabled:true,expected_revision:'0',admitted_revision:'1',not_found_is_not_proof:true};},
  loadPending:async()=>pending,savePending:async(value,expected)=>{assert.equal(pending?.id??null,expected);if(f.failWrite)throw Error('private');if(f.readback)pending=value;},archivePending:async value=>{archives.push(archiveInput(value));}});return f;
}
const run=(f,fn)=>f.panel.action(fn);
const read=f=>run(f,g=>f.panel.read(g));
const set=f=>run(f,g=>f.panel.set(g,'manual_engine',true));
{
 const f=fixture();let seq=0;
 f.panel.request=async c=>{f.calls.push(c);if(c.op==='nkn_status')return report(++seq);return {request_id:id,known:true,state:'accepted',core_instance:ctx,owner:c.owner,enabled:c.enabled,expected_revision:'0',admitted_revision:'1',not_found_is_not_proof:true};};
 await read(f);assert.equal(f.panel.buttons.statistics_on.disabled,false);assert.equal(f.panel.buttons.statistics_off.disabled,true);
 await run(f,g=>f.panel.set(g,'statistics',true));
 const requests=f.calls.filter(c=>c.op==='set_nkn_enabled');assert.equal(requests.length,1);assert.equal(requests[0].owner,'statistics');
 assert.equal(f.archives[0].record.owner,'statistics');assert.equal(f.pending,null);assert.equal(f.calls.some(c=>/subscribe|check_in/.test(c.op)),false);f.panel.reset();
}
{
 const f=fixture();assert.equal(f.calls.length,0);await read(f);assert.equal(f.calls.length,1);assert.equal(f.panel.buttons.remote_on.disabled,true);f.clock.wall+=5001;f.panel.render();assert.match(f.panel.output.textContent,/陈旧/);assert.equal(f.calls.length,1);f.panel.reset();
}
{
 const f=fixture();await read(f);f.failRead=true;await read(f);assert.equal(f.panel.sample,null);assert.doesNotMatch(f.panel.output.textContent,/服务：运行中/);assert.doesNotMatch(f.panel.state.textContent,/SYNTHETIC_PRIVATE/);f.panel.reset();
}
for(const mode of ['failWrite','readback']){
 const f=fixture();await read(f);if(mode==='failWrite')f.failWrite=true;else f.readback=false;await set(f);assert.equal(f.calls.filter(c=>c.op==='set_nkn_enabled').length,0);f.panel.reset();
}
{
 const f=fixture();await read(f);await set(f);assert.equal(f.calls.filter(c=>c.op==='set_nkn_enabled').length,1);assert.equal(f.archives.length,1);assert.equal(f.pending,null);assert.equal(f.calls.filter(c=>c.op==='nkn_status').length,2);f.panel.reset();
}
{
 const f=fixture();f.result='unknown';await read(f);await set(f);assert.deepEqual(f.pending,record());f.panel.reset();assert.deepEqual(f.pending,record());await read(f);f.result='absent';await run(f,g=>f.panel.query(g));assert.equal(f.calls.filter(c=>c.op==='set_nkn_enabled').length,1);assert.deepEqual(f.pending,record());assert.match(f.panel.state.textContent,/不证明未执行/);f.panel.reset();
}
{
 const f=fixture();await read(f);f.panel.request=async()=>report(1);await read(f);assert.equal(f.panel.sample,null,'Repeated sequence cannot become fresh');f.panel.request=async()=>report(2);await read(f);assert.equal(f.panel.sample.data.provider.status.sequence,2);f.panel.request=async()=>{const r=report(3);r.provider.status.generation=1;return r;};await read(f);assert.equal(f.panel.sample,null,'Lower process epoch rejected');f.panel.reset();
}
{
 const f=fixture();let resolve;f.panel.request=()=>new Promise(r=>resolve=r);const p=read(f);await new Promise(r=>setImmediate(r));f.panel.reset();resolve(report());await p;assert.equal(f.panel.sample,null,'Late reply after logout discarded');f.panel.reset();
}
for(const change of [{expected_revision:'00'},{owner:'root'},{pid:1},{core_instance:'broken'}])assert.throws(()=>pendingInput({...record(),...change}));
assert.throws(()=>archiveInput({record:record(),state:'accepted',admitted_revision:'99'}));
assert.throws(()=>archiveInput({record:record(),state:'unknown',admitted_revision:'1'}));
{const v=report();v.provider.status.source='mock';assert.throws(()=>reportInput(v));}
{const v=report();v.provider.original_process_exit_verified=true;assert.throws(()=>reportInput(v));}
// A backend sample's existing age and response/local elapsed time share one
// five-second freshness limit. Stale displays must also disable mutations.
for(const {age,transit,after} of [
 {age:4900,transit:200,after:0},
 {age:4900,transit:50,after:51},
 {age:5000,transit:0,after:1},
]){
 const f=fixture();f.panel.request=async c=>{f.calls.push(c);const v=report();v.provider.status.age_ms=age;f.clock.mono+=transit;f.clock.wall+=transit;return v;};
 await read(f);f.clock.mono+=after;f.clock.wall+=after;f.panel.render();
 assert.equal(f.panel.fresh(),false,'Backend age must be included in mutation freshness');
 assert.equal(f.panel.buttons.manual_on.disabled,true);assert.equal(f.panel.buttons.statistics_on.disabled,true);
 await set(f);assert.equal(f.calls.filter(c=>c.op==='set_nkn_enabled').length,0);assert.equal(f.pending,null);f.panel.reset();
}
{
 const f=fixture(),schedule=globalThis.setTimeout,scheduled=[];
 globalThis.setTimeout=(fn,delay,...args)=>{scheduled.push(delay);return schedule(fn,delay,...args);};
 try{
  f.panel.request=async()=>{const v=report();v.provider.status.age_ms=4900;f.clock.mono+=50;f.clock.wall+=50;return v;};
  await read(f);assert.equal(f.panel.fresh(),true);assert.equal(f.panel.buttons.manual_on.disabled,false);
  assert.deepEqual(scheduled,[51],'Expiry timer includes backend age and response elapsed time');
 }finally{f.panel.reset();globalThis.setTimeout=schedule;}
}
{
 // When no engine exists, Core's fresh unavailable report still permits an
 // explicit Owner start. Unknown counters are not evidence of a stale report.
 const f=fixture();f.panel.request=async()=>{const v=report(),s=v.provider.status;s.source='unavailable';s.age_ms=null;for(const k of ['enabled','service','path','dht'])s[k]='unknown';for(const k of ['rx_bytes','tx_bytes','rtt_ms','peer_count'])s[k]=null;return v;};
 await read(f);assert.equal(f.panel.fresh(),true);assert.equal(f.panel.buttons.manual_on.disabled,false);assert.match(f.panel.output.textContent,/服务：未知/);f.panel.reset();
}
console.log('PASS: actual NKN product panel; durable CAS before mutation, exact receipts, Unknown/no resend, stale/sequence/epoch/logout fences, unavailable remote, no private errors or passive I/O');
if(process.argv[2]){
 const captured=JSON.parse(await readFile(process.argv[2],'utf8')).result.status;
 const parsed=reportInput(captured);
 assert.equal(parsed.provider.status.source,'engine');assert.equal(parsed.provider.status.service,'running');
 assert.equal(parsed.provider.status.dht,'unknown');assert.equal(parsed.provider.status.path,'unknown');
 assert.deepEqual(parsed.provider.owners,{manual:true,remote:true,statistics:true});
 console.log('PASS: captured actual SDK/Root/Core report accepted by production JS contract; this serialization check does not refresh the captured sample');
}
