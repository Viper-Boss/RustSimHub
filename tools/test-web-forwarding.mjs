// SPDX-License-Identifier: AGPL-3.0-only
// Execute the real panel, only synthetic Core replies and disposable DOM/storage.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const source=await readFile(new URL('../crates/edge/src/web/forwarding.js',import.meta.url));
const {ForwardingPanel,ForwardingQuickTest,policyInput,pendingInput,processText}=await import('data:text/javascript;base64,'+source.toString('base64'));
const id='11111111-1111-4111-8111-111111111111',rule='22222222-2222-4222-8222-222222222222';
const other='33333333-3333-4333-8333-333333333333';
const secret='synthetic-only-credential',endpoint='https://example.invalid/notify?key='+secret;
const clone=structuredClone,turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {
 constructor(tag='div'){this.tagName=tag;this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;this.checked=false;this.selected=false;}
 append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;this.textContent='';}get options(){return this.children;}
 set innerHTML(_){throw Error('Never parse provider/configuration data as HTML');}
}
function fixture({existing=false,managed=true,consent=true,onRequest,onSave,onLoad,testState='confirmed',onDelay}={}){
 const elements=new Map(),$=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const doc={getElementById:$,createElement:tag=>new Element(tag)};
 let token=true,pending=null,policy={enabled:false,targets:existing?[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}]:[],rules:existing?[{id:rule,targets:[id],sender_equals:'sender',body_contains:null,line:null,share:'notice'}]:[]};
 let current=existing?1:0;const calls=[],writes=[],archives=[],markers=[],requests=new Map(),tests=new Map(),confirmations=[];
 const status=()=>({available:true,configured:current>0,generation:current,enabled:policy.enabled,configuration_api:true,retained_credentials_api:true,request_receipt_api:true,synthetic_test_api:true,delivery_api:policy.enabled,service_management_api:managed,process_state_verified:true,state:policy.enabled?'running':'disabled'});
 const view=()=>({enabled:policy.enabled,generation:current,targets:policy.targets.map(t=>({id:t.id,kind:t.destination.kind,credentials_redacted:true})),rules:clone(policy.rules)});
 const panel=new ForwardingPanel({document:doc,crypto:webcrypto,delay:async()=>{await onDelay?.();},getToken:()=>token,confirm:message=>{confirmations.push(message);return consent;},
  loadPending:async()=>{await onLoad?.();return clone(pending);},savePending:async value=>{await onSave?.(value);pending=clone(value);markers.push(clone(value));},archivePending:async value=>{archives.push(clone(value));},
  request:async c=>{
   calls.push(clone(c));const custom=await onRequest?.(c);if(custom!==undefined)return custom;
   if(c.op==='forwarding_status')return status();
   if(c.op==='forwarding_rules')return view();
   if(c.op==='forwarding_request'){const gen=requests.get(c.request_id);return {request_id:c.request_id,known:!!gen,applied_generation:gen??null,current_generation:current,not_found_is_not_proof:true};}
   if(c.op==='forwarding_receipts')return {receipts:[]};
   if(c.op==='forwarding_test'){
    assert.equal(policy.enabled,true);assert.deepEqual(pending,{format:1,id:c.request_id,kind:'test',expected_generation:current,target_id:c.target_id});
    const receipt={id:c.request_id,target_id:c.target_id,generation:current,synthetic:true,state:'queued'};tests.set(c.request_id,receipt);return {request_id:c.request_id,accepted:true,synthetic:true,receipt:clone(receipt)};
   }
   if(c.op==='forwarding_test_request'){const receipt=tests.get(c.request_id);if(receipt)receipt.state=testState;return {request_id:c.request_id,known:!!receipt,not_found_is_not_proof:true,receipt:clone(receipt??null)};}

   if(c.op==='forwarding_configure'||c.op==='set_forwarding_enabled'){
    assert.equal(c.expected_generation,current);assert.deepEqual(pending,{format:1,id:c.request_id,kind:c.op==='forwarding_configure'?'configure':c.enabled?'enable':'disable',expected_generation:current});
    for(const input of panel.secretInputs)assert.equal(input.value,'','Clear drafts before dispatch');
    writes.push(clone(c));
    if(c.op==='forwarding_configure'){
     const old=new Map(policy.targets.map(t=>[t.id,t.destination]));
     policy={...clone(c.policy),targets:c.policy.targets.map(t=>({id:t.id,destination:clone(t.destination.kind==='keep'?old.get(t.id):t.destination)}))};
    }else policy.enabled=c.enabled;
    current++;requests.set(c.request_id,current);
    return {...status(),configuration_applied:true,request_id:c.request_id};
   }
   throw Error('unexpected test command');
  }});
 return {panel,$,calls,writes,archives,markers,confirmations,status,view,requests,read:()=>panel.action(g=>panel.read(g)),
  pending:()=>clone(pending),setPending:v=>{pending=clone(v);},setToken:v=>{token=v;},apply:(p,g)=>{policy=clone(p);current=g;}};
}
function allText(f){const walk=e=>[e.textContent,...e.children.flatMap(walk)];return ['forwarding-state','forwarding-pending','forwarding-targets','forwarding-rules','forwarding-receipts'].map(id=>walk(f.$(id)).join('\n')).join('\n');}
{
 const f=fixture();assert.equal(f.calls.length,0);assert.equal(f.$('forwarding-save').disabled,true);
 await f.read();assert.equal(f.panel.targets.length,1);assert.equal(f.panel.rules.length,1);
 const t=f.panel.targets[0],password=t.endpointInput;t.endpointInput.value=endpoint;t.bearerInput.value=secret;
 f.$('forwarding-add-rule').onclick();assert.equal(f.panel.targets[0].endpointInput,password);assert.equal(password.value,endpoint,'Re-render retains drafts only in password elements');
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,1);assert.equal(f.writes[0].policy.enabled,false);assert.equal(f.writes[0].policy.targets[0].destination.url,endpoint);
 assert.equal(f.pending(),null);assert.equal(password.value,'');assert.ok(!JSON.stringify([f.markers,f.archives]).includes(secret));assert.equal(f.panel.targets[0].keep,true);
 assert.match(allText(f),/进程已关闭/);assert.ok(!allText(f).includes(secret));
 await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes.length,2);assert.equal(f.writes[1].op,'set_forwarding_enabled');assert.equal('policy' in f.writes[1],false);
 assert.match(allText(f),/运行中/);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes[2].enabled,false);
}
{
 const f=fixture({existing:true});await f.read();f.panel.rules[0].contains='过滤 <img src=x>';f.panel.rules[0].full=true;
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes[0].policy.targets[0].destination.kind,'keep');assert.equal(f.writes[0].policy.rules[0].share,'full_text');assert.match(f.confirmations[0],/完整短信正文/);
 assert.ok(!JSON.stringify(f.writes[0]).includes(secret));assert.equal(f.panel.targets[0].keep,true);assert.equal(f.panel.rules[0].contains,'过滤 <img src=x>');
}
{
 const f=fixture({managed:false,existing:true});await f.read();assert.equal(f.$('forwarding-save').disabled,true);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes.length,0);
 const p={enabled:true,state:'awaiting_service',process_state_verified:false};assert.doesNotMatch(processText(p),/服务运行中/);
}
{
 const f=fixture({consent:false});await f.read();f.panel.targets[0].endpointInput.value=endpoint;
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,0);assert.equal(f.markers.length,0);
}
{
 const f=fixture({onSave:async value=>{if(value)throw Error(secret);}});await f.read();const password=f.panel.targets[0].endpointInput;password.value=endpoint;
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,0);assert.equal(password.value,'');assert.match(allText(f),/未提交到设备/);assert.ok(!allText(f).includes(secret));
}
for(const point of ['before_marker','after_dispatch']){
 const gate=deferred();let entered=false;
 const f=fixture({onSave:async value=>{if(point==='before_marker'&&value){entered=true;await gate.promise;}},onRequest:async c=>{if(point==='after_dispatch'&&c.op==='forwarding_configure'){entered=true;return gate.promise;}}});
 await f.read();const password=f.panel.targets[0].endpointInput;password.value=endpoint;
 const saving=f.panel.action(g=>f.panel.save(g));while(!entered)await turn();
 f.panel.reset();f.setToken(false);gate.resolve({configuration_applied:true});await saving;
 assert.equal(password.value,'');assert.equal(f.panel.targets.length,0);assert.equal(f.archives.length,0);assert.ok(f.pending());
 assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,point==='before_marker'?0:1);
}
{
 let lost=true;const f=fixture({existing:true,onRequest:async c=>{if(c.op==='forwarding_configure'&&lost){lost=false;throw Error(secret);}}});await f.read();
 await f.panel.action(g=>f.panel.save(g));assert.ok(f.pending());assert.equal(f.$('forwarding-save').disabled,true);assert.ok(!allText(f).includes(secret));
 await f.panel.action(g=>f.panel.query(g));assert.ok(f.pending(),'Unknown/pruned is not proof of non-admission');assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,1);
 await f.panel.action(g=>f.panel.archiveUnknown(g));assert.equal(f.pending(),null);assert.equal(f.archives[0].uncertain,true);assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,1);
}
{
 const f=fixture({existing:true});await f.read();const record={format:1,id:other,kind:'configure',expected_generation:1};f.setPending(record);f.requests.set(other,2);
 f.apply({enabled:false,targets:[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}],rules:f.view().rules},2);
 await f.read();await f.panel.action(g=>f.panel.query(g));assert.equal(f.pending(),null);assert.equal(f.archives[0].applied_generation,2);assert.equal(f.writes.length,0);
}
{
 const f=fixture({existing:true,onRequest:async c=>c.op==='forwarding_receipts'?{receipts:[{seq:'1',id:other,message_id:id,target_id:rule,state:'unknown',text:secret}]}:undefined});await f.panel.action(g=>f.panel.receipts(g,true));
 assert.match(allText(f),/不会自动重发/);assert.ok(!allText(f).includes(secret));assert.equal(f.writes.length,0);
}
{
 const target={id,kind:'webhook',existing:true,keep:true};const r={id:rule,targets:[id],sender:'',contains:'',line:'',full:false};
 assert.deepEqual(policyInput([target],[r],true).targets[0].destination,{kind:'keep'});
 assert.equal(policyInput([target],[r],false).rules[0].share,'notice');
 for(const candidate of ['http://example.invalid','https://u:p@example.invalid','https://example.invalid/#fragment','https://example.invalid:99999'])assert.throws(()=>policyInput([{...target,keep:false,endpoint:candidate}],[r],false));
 assert.throws(()=>policyInput(Array(9).fill(target),[r],false));assert.throws(()=>policyInput([target],Array(33).fill(r),false));
 assert.throws(()=>policyInput([target],[{...r,contains:'🙂'.repeat(65)}],false));assert.throws(()=>policyInput([target],[{...r,line:'1.5'}],false));
 assert.throws(()=>pendingInput({format:1,id,kind:'configure',expected_generation:0,password:secret}));
}
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
{
 const gate=deferred(),f=fixture({onLoad:()=>gate.promise});f.setPending({format:1,id,kind:'configure',expected_generation:0});
 const reading=f.read();await turn();f.panel.reset();f.setToken(false);gate.resolve();await reading;
 assert.equal(f.panel.pending,null);assert.equal(f.calls.length,0,'Stale browser storage cannot resurrect another session');
}
{
 let mismatch=false;const f=fixture({existing:true,onRequest:async c=>mismatch&&c.op==='forwarding_rules'?{enabled:false,generation:2,targets:[],rules:[]}:undefined});
 await f.read();assert.equal(f.panel.ready,true);mismatch=true;await f.read();
 assert.equal(f.panel.ready,false);assert.equal(f.panel.sample,null);assert.equal(f.$('forwarding-save').disabled,true);assert.equal(f.writes.length,0);
}
assert.match(html,/<details id="forwarding-panel" data-configuration>/);assert.doesNotMatch(html,/<details id="forwarding-panel"[^>]*\bopen\b/);
assert.doesNotMatch(source.toString(),/setInterval|localStorage|innerHTML/);
console.log('PASS: actual forwarding panel, redacted keep edits, durable metadata before one submit, exact receipt reconciliation, no automatic mutation/passive polling, input privacy, limits and logout/storage races; synthetic only.');

async function readyTest(options={}){const f=fixture({existing:true,...options});await f.read();assert.equal(f.$('forwarding-test').disabled,true);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.$('forwarding-test').disabled,false);return f;}
{
 const f=await readyTest();await f.panel.action(g=>f.panel.test(g));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,1);
 assert.equal(f.pending(),null);assert.equal(f.archives.at(-1).kind,'test');assert.equal(f.archives.at(-1).target_id,id);assert.equal(f.archives.at(-1).applied_generation,2);assert.ok(!JSON.stringify(f.archives).includes(secret));assert.match(allText(f),/合成测试已获目标确认/);
}
for(const state of ['queued','unknown','failed','cancelled']){
 const f=await readyTest({testState:state});await f.panel.action(g=>f.panel.test(g));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,state==='queued'?10:1);
 assert.equal(!!f.pending(),['queued','unknown'].includes(state));
 if(state==='unknown'){await f.panel.action(g=>f.panel.query(g));assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.ok(f.pending());}
}
{
 let fail=true;const f=await readyTest({onRequest:async c=>{if(c.op==='forwarding_test'&&fail)throw Error('lost reply');}});
 await f.panel.action(g=>f.panel.test(g));assert.ok(f.pending());assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,0);
 fail=false;await f.panel.action(g=>f.panel.query(g));assert.ok(f.pending());assert.match(allText(f),/不代表从未外发/);assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);
}
{
 let reject=false;const f=await readyTest({onSave:async value=>{if(reject&&value)throw Error('storage failed');}});reject=true;
 await f.panel.action(g=>f.panel.test(g));assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,0);assert.match(allText(f),/本次未提交测试/);
}
{
 const gate=deferred();let waiting=false;const f=await readyTest({onDelay:async()=>{waiting=true;await gate.promise;}});
 const running=f.panel.action(g=>f.panel.test(g));while(!waiting)await turn();f.panel.suspend();gate.resolve();await running;
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,0);assert.ok(f.pending());assert.equal(f.$('forwarding-test').disabled,true);
}
assert.throws(()=>pendingInput({format:1,id,kind:'test',expected_generation:1}));
assert.throws(()=>pendingInput({format:1,id,kind:'configure',expected_generation:1,target_id:other}));
console.log('PASS: explicit synthetic target, durable no-repeat marker, bounded receipt reads, confirmed/failed/unknown/timeout/lost replies and panel-close cancellation.');

function quickFixture({state='confirmed',active=false,lost=false,saveFails=false,enabled=false,stop=true,readFails=false,initialPending=null,configurationKnown=true}={}){
 let pending=clone(initialPending),accepted=false,logins=0;const calls=[],messages=[],archives=[],gate=deferred();
 const status=()=>({quick_test_api:true,service_management_api:true,enabled,process_state_verified:true,state:enabled?'running':'disabled',quick_test_active:false,generation:1});
 const quick=new ForwardingQuickTest({crypto:webcrypto,login:async()=>{logins++;},notify:s=>messages.push(s),delay:async()=>{await gate.promise;},
  loadPending:async()=>clone(pending),savePending:async(value,expected)=>{assert.equal(pending?.id??null,expected);if(saveFails&&value)throw Error('storage failed');pending=clone(value);},archivePending:async value=>archives.push(clone(value)),
  request:async c=>{
   calls.push(clone(c));
   if(c.op==='forwarding_status')return {...status(),...(stop?{}:{process_state_verified:false})};
   if(c.op==='forwarding_rules')return {generation:1,enabled:false,targets:[{id,kind:'bark',credentials_redacted:true}]};
   if(c.op==='forwarding_request')return {request_id:c.request_id,known:configurationKnown,current_generation:1,applied_generation:configurationKnown?1:null,not_found_is_not_proof:true};
   if(c.op==='forwarding_quick_test'){
    assert.equal(pending.kind,'quick_test');assert.equal(pending.id,c.request_id);accepted=true;
    if(lost)throw Error('lost admission reply');return {accepted:true,quick_test:true,request_id:c.request_id};
   }
   assert.equal(c.op,'forwarding_test_request');if(readFails)throw Error('logged out');
   return {request_id:c.request_id,known:accepted,not_found_is_not_proof:true,quick_active:active,automatic_stop_verified:stop,
    receipt:accepted?{id:c.request_id,target_id:id,generation:1,synthetic:true,state}:null};
  }});
 return {quick,calls,messages,archives,pending:()=>pending,gate,logins:()=>logins};
}
{
 const f=quickFixture();await f.quick.run();assert.equal(f.logins(),1);assert.equal(f.pending(),null);assert.match(f.messages.at(-1),/Bark 已确认.*自动关闭/);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);assert.equal(f.archives[0].kind,'quick_test');
 assert.ok(f.calls.every(c=>!['set_forwarding_enabled','forwarding_configure','forwarding_test'].includes(c.op)));
}
for(const state of ['failed','cancelled','unknown']){
 const f=quickFixture({state});await f.quick.run();assert.equal(!!f.pending(),state==='unknown');assert.match(f.messages.at(-1),/自动关闭|转发已关闭/);
}
{
 const f=quickFixture({lost:true});await f.quick.run();assert.ok(f.pending());await f.quick.run();
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);assert.equal(f.pending(),null);assert.match(f.messages.at(-1),/已确认/);
}
{
 const f=quickFixture({saveFails:true});await f.quick.run();assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,0);
}
{
 const f=quickFixture({enabled:true});await f.quick.run();assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,0);assert.equal(f.pending(),null);
}
{
 const f=quickFixture({readFails:true});await f.quick.run();assert.ok(f.pending());assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);
}
for(const configurationKnown of [true,false]){
 const previous={format:1,id:other,kind:'configure',expected_generation:0};
 const f=quickFixture({initialPending:previous,configurationKnown});await f.quick.run();
 assert.equal(f.pending(),null);assert.equal(f.archives.length,2);assert.equal(f.archives[0].id,other);
 assert.equal(f.archives[0].applied_generation,configurationKnown?1:null);assert.equal(f.archives[0].uncertain,!configurationKnown);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);
 assert.ok(f.calls.every(c=>!['forwarding_configure','set_forwarding_enabled'].includes(c.op)));
}
{
 const previous={format:1,id:other,kind:'enable',expected_generation:1};
 const f=quickFixture({initialPending:previous,configurationKnown:false,enabled:true});await f.quick.run();
 assert.deepEqual(f.pending(),previous);assert.equal(f.archives.length,0);assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,0);
}
{
 const f=quickFixture({active:true});const run=f.quick.run();while(!f.messages.some(m=>m.includes('正在自动关闭')))await turn();
 await f.quick.run();assert.equal(f.logins(),1);f.gate.resolve();await run;assert.ok(f.pending());assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);assert.match(f.messages.at(-1),/超时/);
}
assert.match(html,/id="test-forwarding"/);
console.log('PASS: one-click Bark admission, unchanged policy, metadata before submit, lost reply recovery without resubmission, terminal/Unknown/timeout/logout/storage failure and duplicate-click fences.');
