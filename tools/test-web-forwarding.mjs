// SPDX-License-Identifier: AGPL-3.0-only
// Execute the real panel, only synthetic Core replies and disposable DOM/storage.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const source=await readFile(new URL('../crates/edge/src/web/forwarding.js',import.meta.url));
const {ForwardingPanel,ForwardingQuickTest,policyInput,pendingInput,processText,chatToken}=await import('data:text/javascript;base64,'+source.toString('base64'));
const id='11111111-1111-4111-8111-111111111111',rule='22222222-2222-4222-8222-222222222222';
const other='33333333-3333-4333-8333-333333333333';
const secret='synthetic-only-credential',endpoint='https://example.invalid/notify?key='+secret;
const clone=structuredClone,turn=()=>new Promise(r=>setImmediate(r));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {
 constructor(tag='div'){this.tagName=tag;this.value='';this.textContent='';this.children=[];this.disabled=false;this.hidden=false;this.checked=false;this.selected=false;}
 append(...items){this.children.push(...items);}prepend(item){this.children=this.children.filter(child=>child!==item);this.children.unshift(item);}
 setAttribute(name,value){this[name]=value;}removeAttribute(name){delete this[name];}
 showModal(){this.open=true;}close(){this.open=false;}
 replaceChildren(...items){this.children=items;this.textContent='';}get options(){return this.children;}
 set innerHTML(_){throw Error('Never parse provider/configuration data as HTML');}
}
function fixture({existing=false,managed=true,consent=true,onRequest,onSave,onLoad,testState='confirmed',quick=false,quickStopped=true,onDelay,rulesRevision=2,emailRevision=null}={}){
 const elements=new Map(),$=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const doc={getElementById:$,createElement:tag=>new Element(tag)};
 let token=true,pending=null,policy={enabled:false,targets:existing?[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}]:[],rules:existing?[{id:rule,targets:[id],sender_equals:'sender',body_contains:null,line:null,share:'notice'}]:[]};
 let current=existing?1:0;const calls=[],writes=[],archives=[],markers=[],requests=new Map(),tests=new Map(),confirmations=[];
 const status=()=>({email_revision:emailRevision,rules_revision:rulesRevision,available:true,configured:current>0,generation:current,enabled:policy.enabled,configuration_api:true,quick_test_api:quick,retained_credentials_api:true,request_receipt_api:true,synthetic_test_api:true,delivery_api:policy.enabled,service_management_api:managed,process_state_verified:true,state:policy.enabled?'running':'disabled'});
 const view=()=>({enabled:policy.enabled,generation:current,targets:policy.targets.map(t=>({id:t.id,kind:t.destination.kind,credentials_redacted:true})),rules:clone(policy.rules)});
 const panel=new ForwardingPanel({document:doc,crypto:webcrypto,delay:async()=>{await onDelay?.();},getToken:()=>token,confirm:message=>{confirmations.push(message);return consent;},
  loadPending:async()=>{await onLoad?.();return clone(pending);},savePending:async value=>{await onSave?.(value);pending=clone(value);markers.push(clone(value));},archivePending:async value=>{archives.push(clone(value));},
  request:async c=>{
   calls.push(clone(c));const custom=await onRequest?.(c);if(custom!==undefined)return custom;
   if(c.op==='forwarding_status')return status();
   if(c.op==='forwarding_rules')return view();
   if(c.op==='forwarding_request'){const gen=requests.get(c.request_id);return {request_id:c.request_id,known:!!gen,applied_generation:gen??null,current_generation:current,not_found_is_not_proof:true};}
   if(c.op==='forwarding_log_page')return {records:[],through:'0',removable:0};
   if(c.op==='forwarding_test'||c.op==='forwarding_quick_test'){
    assert.equal(policy.enabled,c.op==='forwarding_test');assert.deepEqual(pending,{format:1,id:c.request_id,kind:c.op==='forwarding_test'?'test':'quick_test',expected_generation:current,target_id:c.target_id});
    const receipt={id:c.request_id,target_id:c.target_id,generation:current,synthetic:true,state:'queued'};tests.set(c.request_id,receipt);return {request_id:c.request_id,accepted:true,synthetic:true,receipt:clone(receipt)};
   }
   if(c.op==='forwarding_test_request'){const receipt=tests.get(c.request_id);if(receipt)receipt.state=testState;return {request_id:c.request_id,known:!!receipt,not_found_is_not_proof:true,receipt:clone(receipt??null),automatic_stop_verified:quickStopped,quick_active:!quickStopped};}

   if(c.op==='forwarding_configure'||c.op==='set_forwarding_enabled'){
    assert.equal(c.expected_generation,current);assert.deepEqual(pending,{format:1,id:c.request_id,kind:c.op==='forwarding_configure'?'configure':c.enabled?'enable':'disable',expected_generation:current});
    for(const input of panel.secretInputs)assert.equal(input.value,'','Clear drafts before dispatch');
    writes.push(clone(c));
    if(c.op==='forwarding_configure'){
     const old=new Map(policy.targets.map(t=>[t.id,t.destination]));
     policy={...clone(c.policy),targets:c.policy.targets.map(t=>({id:t.id,destination:clone(t.destination.kind==='keep'?old.get(t.id):t.destination.kind==='copy'?old.get(t.destination.source_id):t.destination)}))};
    }else policy.enabled=c.enabled;
    current++;requests.set(c.request_id,current);
    return {...status(),configuration_applied:true,request_id:c.request_id};
   }
   throw Error('unexpected test command');
  }});
 return {panel,$,calls,writes,archives,markers,confirmations,status,view,requests,read:()=>panel.action(g=>panel.read(g)),
  pending:()=>clone(pending),setPending:v=>{pending=clone(v);},setToken:v=>{token=v;},apply:(p,g)=>{policy=clone(p);current=g;}};
}
function allText(f){const walk=e=>[e.textContent,...e.children.flatMap(walk)];return ['forwarding-state','forwarding-pending','forwarding-targets','forwarding-rules','forwarding-receipts','fw-dialog-error'].map(id=>walk(f.$(id)).join('\n')).join('\n');}
const save=f=>f.panel.action(g=>f.panel.save(g));
function newWebhook(f){
 f.panel.edit('channel');
 const row=f.panel.draft.row;
 row.endpointInput.value=endpoint;row.bearerInput.value=secret;
 return row;
}
function editRule(f){f.panel.edit('rule',f.view().rules[0]);return f.panel.draft.row;}
{
 const f=fixture();assert.equal(f.calls.length,0);assert.equal(f.$('forwarding-save').disabled,true);
 await f.read();assert.equal(f.panel.view.targets.length,0);assert.equal(f.panel.view.rules.length,0);
 assert.equal(f.$('forwarding-add-rule').disabled,true);
 const row=newWebhook(f),password=row.endpointInput;
 await save(f);assert.equal(f.writes.length,1);assert.equal(f.writes[0].policy.enabled,false);
 assert.equal(f.writes[0].policy.targets[0].destination.url,endpoint);assert.deepEqual(f.writes[0].policy.rules,[]);
 assert.equal(f.pending(),null);assert.equal(password.value,'');assert.ok(!JSON.stringify([f.markers,f.archives]).includes(secret));
 assert.equal(f.panel.draft,null);assert.ok(!allText(f).includes(secret));
 f.panel.edit('rule');assert.deepEqual(f.panel.draft.row.targets,[row.id]);await save(f);
 assert.equal(f.writes.length,2);assert.equal(f.writes[1].policy.targets[0].destination.kind,'keep');
 assert.equal(f.writes[1].policy.rules[0].share,'notice');
 await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes.length,3);assert.equal(f.writes[2].op,'set_forwarding_enabled');assert.equal('policy' in f.writes[2],false);
 assert.match(allText(f),/运行中/);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes[3].enabled,false);
}
{
 const f=fixture({existing:true});await f.read();const row=editRule(f);row.contains='过滤 <img src=x>';row.full=true;
 await save(f);assert.equal(f.writes[0].policy.targets[0].destination.kind,'keep');assert.equal(f.writes[0].policy.rules[0].share,'full_text');assert.match(f.confirmations[0],/完整短信正文/);
 assert.ok(!JSON.stringify(f.writes[0]).includes(secret));assert.equal(f.view().rules[0].body_contains,'过滤 <img src=x>');
}
{
 const f=fixture({existing:true});await f.read();f.panel.edit('channel',f.view().targets[0]);
 await f.panel.action(g=>f.panel.remove(g));assert.match(allText(f),/关联 1 条规则/);assert.equal(f.writes.length,0);
 f.panel.closeEditor();editRule(f);await f.panel.action(g=>f.panel.remove(g));assert.equal(f.writes[0].policy.rules.length,0);
 f.panel.edit('channel',f.view().targets[0]);await f.panel.action(g=>f.panel.remove(g));assert.equal(f.writes[1].policy.targets.length,0);
}
{
 const f=fixture({managed:false,existing:true});await f.read();assert.equal(f.$('forwarding-save').disabled,true);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.writes.length,0);
 const p={enabled:true,state:'awaiting_service',process_state_verified:false};assert.doesNotMatch(processText(p),/服务运行中/);
}
{
 const f=fixture({consent:false,existing:true});await f.read();editRule(f).full=true;
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,0);assert.equal(f.markers.length,0);
}
{
 const f=fixture({onSave:async value=>{if(value)throw Error(secret);}});await f.read();const password=newWebhook(f).endpointInput;
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,0);assert.equal(password.value,'');assert.match(allText(f),/本次未提交/);assert.ok(!allText(f).includes(secret));
}
for(const point of ['before_marker','after_dispatch']){
 const gate=deferred();let entered=false;
 const f=fixture({onSave:async value=>{if(point==='before_marker'&&value){entered=true;await gate.promise;}},onRequest:async c=>{if(point==='after_dispatch'&&c.op==='forwarding_configure'){entered=true;return gate.promise;}}});
 await f.read();const password=newWebhook(f).endpointInput;
 const saving=f.panel.action(g=>f.panel.save(g));while(!entered)await turn();
 f.panel.reset();f.setToken(false);gate.resolve({configuration_applied:true});await saving;
 assert.equal(password.value,'');assert.equal(f.panel.view,null);assert.equal(f.archives.length,0);assert.ok(f.pending());
 assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,point==='before_marker'?0:1);
}
{
 let lost=true;const f=fixture({existing:true,onRequest:async c=>{if(c.op==='forwarding_configure'&&lost){lost=false;throw Error(secret);}}});await f.read();editRule(f).contains='new match';
 await save(f);assert.ok(f.pending());assert.equal(f.$('forwarding-save').disabled,true);assert.ok(!allText(f).includes(secret));
 await f.panel.action(g=>f.panel.query(g));assert.ok(f.pending(),'Unknown/pruned is not proof of non-admission');assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,1);
 await f.panel.action(g=>f.panel.archiveUnknown(g));assert.equal(f.pending(),null);assert.equal(f.archives[0].uncertain,true);assert.equal(f.calls.filter(c=>c.op==='forwarding_configure').length,1);
}
{
 const f=fixture({existing:true});await f.read();const record={format:1,id:other,kind:'configure',expected_generation:1};f.setPending(record);f.requests.set(other,2);
 f.apply({enabled:false,targets:[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}],rules:f.view().rules},2);
 await f.read();assert.equal(f.pending(),null);assert.equal(f.archives[0].applied_generation,2);assert.equal(f.writes.length,0);
}
{
 let lookups=0;const f=fixture({existing:true,onRequest:async c=>{
  if(c.op==='forwarding_request'&&++lookups===1)throw Object.assign(Error('synthetic device busy'),{coreCode:'busy'});
 }});
 const pending={format:1,id:other,kind:'configure',expected_generation:0};f.setPending(pending);
 await f.read();assert.equal(lookups,2);assert.deepEqual(f.pending(),pending);
 assert.match(allText(f),/没有找到这笔保存回执/);assert.equal(f.writes.length,0);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_log_page').length,1);
}
{
 const f=fixture({existing:true,onRequest:async c=>c.op==='forwarding_log_page'?{records:[{seq:'1',id:other,message_id:id,target_id:rule,state:'unknown',text:secret}],through:'1',removable:0}:undefined});await f.panel.action(g=>f.panel.receipts(g,true));
 assert.match(allText(f),/待核对/);assert.ok(!allText(f).includes(secret));assert.equal(f.writes.length,0);
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

async function readyTest(options={}){const f=fixture({existing:true,...options});await f.read();assert.equal(f.panel.testReady,false);await f.panel.action(g=>f.panel.toggle(g));assert.equal(f.panel.testReady,true);return f;}
{
 const f=await readyTest();await f.panel.action(g=>f.panel.test(g,id));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,1);
 assert.equal(f.pending(),null);assert.equal(f.archives.at(-1).kind,'test');assert.equal(f.archives.at(-1).target_id,id);assert.equal(f.archives.at(-1).applied_generation,2);assert.ok(!JSON.stringify(f.archives).includes(secret));assert.match(allText(f),/合成测试已获目标确认/);
}
{
 let reads=0;
 const f=await readyTest({onRequest:async c=>{
  if(c.op==='forwarding_test_request'&&++reads<=2)throw Object.assign(Error('synthetic device busy'),{coreCode:'busy'});
 }});
 await f.panel.action(g=>f.panel.test(g,id));
 assert.equal(reads,3);
 assert.match(allText(f),/合成测试已获目标确认/);
 assert.equal(f.pending(),null);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);
}
for(const state of ['queued','unknown','failed','cancelled']){
 const f=await readyTest({testState:state});await f.panel.action(g=>f.panel.test(g,id));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,state==='queued'?10:1);
 assert.equal(!!f.pending(),['queued','unknown'].includes(state));
 if(state==='unknown'){await f.panel.action(g=>f.panel.query(g));assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.ok(f.pending());}
}
{
 let fail=true;const f=await readyTest({onRequest:async c=>{if(c.op==='forwarding_test'&&fail)throw Error('lost reply');}});
 await f.panel.action(g=>f.panel.test(g,id));assert.ok(f.pending());assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,0);
 fail=false;await f.panel.action(g=>f.panel.query(g));assert.ok(f.pending());assert.match(allText(f),/不代表从未外发/);assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);
}
{
 let reject=false;const f=await readyTest({onSave:async value=>{if(reject&&value)throw Error('storage failed');}});reject=true;
 await f.panel.action(g=>f.panel.test(g,id));assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,0);assert.match(allText(f),/本次未提交测试/);
}
{
 const gate=deferred();let waiting=false;const f=await readyTest({onDelay:async()=>{waiting=true;await gate.promise;}});
 const running=f.panel.action(g=>f.panel.test(g,id));while(!waiting)await turn();f.panel.suspend();gate.resolve();await running;
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test').length,1);assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,0);assert.ok(f.pending());assert.equal(f.panel.testReady,false);
}
assert.throws(()=>pendingInput({format:1,id,kind:'test',expected_generation:1}));
assert.throws(()=>pendingInput({format:1,id,kind:'configure',expected_generation:1,target_id:other}));
console.log('PASS: explicit synthetic target, durable no-repeat marker, bounded receipt reads, confirmed/failed/unknown/timeout/lost replies and panel-close cancellation.');

{
 const f=fixture({quick:true});
 await f.read();const row=newWebhook(f);await save(f);
 await f.panel.action(g=>f.panel.test(g,row.id));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);
 assert.equal(f.calls.filter(c=>c.op==='set_forwarding_enabled').length,0);
 assert.equal(f.view().enabled,false);assert.equal(f.view().rules.length,0);
 assert.equal(f.archives.at(-1).target_id,row.id);
}
{
 const f=fixture({existing:true,quick:true,quickStopped:false});
 await f.read();await f.panel.action(g=>f.panel.test(g,id));
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);
 assert.equal(f.pending()?.kind,'quick_test');assert.equal(f.archives.length,0);
}
{
 const record=n=>({seq:String(n),id:other,message_id:id,target_id:id,state:n===20?'failed':'confirmed',synthetic:false,text:secret});
 const f=fixture({existing:true,onRequest:async c=>c.op==='forwarding_log_page'?
  {through:'20',removable:20,records:c.filter.state==='failed'?[record(20)]:c.before===null?Array.from({length:16},(_,i)=>record(20-i)):[record(4),record(1)]}:undefined});
 await f.read();assert.equal(f.panel.receiptRows[0].seq,'20');assert.equal(f.panel.more,true);
 await f.panel.action(g=>f.panel.receipts(g,false));
 assert.equal(f.calls.at(-1).before,'5');assert.equal(f.panel.receiptRows.at(-1).seq,'1');
 f.$('fw-record-filter').value='failed';await f.$('fw-record-filter').onchange();
 assert.equal(f.$('forwarding-receipts').children.length,1);assert.ok(!allText(f).includes(secret));
}
console.log('PASS: isolated channel wizard, saved rules, newest-first records and finite channel test before enabling forwarding.');

{
 const f=fixture({existing:true});await f.read();
 f.panel.edit('channel',f.view().targets[0],null,true);
 await save(f);
 assert.equal(f.view().targets.length,2);
 assert.deepEqual(f.writes[0].policy.targets[1].destination,{kind:'copy',source_id:id});
 assert.ok(!JSON.stringify(f.writes).includes(secret));
}
function quickFixture({state='confirmed',active=false,lost=false,saveFails=false,enabled=false,stop=true,readFails=false,busyReads=0,initialPending=null,configurationKnown=true}={}){
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
   if(busyReads>0){busyReads--;throw Object.assign(Error('synthetic device busy'),{coreCode:'busy'});}
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
{
 const f=quickFixture({busyReads:2});f.gate.resolve();await f.quick.run();
 assert.equal(f.calls.filter(c=>c.op==='forwarding_test_request').length,3);
 assert.equal(f.calls.filter(c=>c.op==='forwarding_quick_test').length,1);
 assert.equal(f.pending(),null);
 assert.match(f.messages.at(-1),/Bark 已确认.*自动关闭/);
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

// New rule fields survive real panel read/edit/save, and are never silently lost.
{
 const f=fixture({existing:true});
 const p={enabled:false,targets:[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}],rules:[{id:rule,targets:[id],sender_equals:null,body_contains:null,line:4,share:'masked_text',conditions:{body_regex:'验证码[A-Z0-9]{6}',time_window:{start_minute:1320,end_minute:360,utc_offset_minutes:480}}}]};
 f.apply(p,4);await f.read();assert.equal(f.panel.ready,true);const draft=editRule(f);assert.equal(draft.masked,true);
 assert.equal(draft.start,'22:00');assert.equal(draft.end,'06:00');
 assert.match(allText(f),/发送脱敏正文/);
 draft.sender='synthetic-sender';await save(f);
 assert.equal(f.writes.length,1);assert.equal(f.writes[0].policy.rules_revision,2);
 assert.deepEqual(f.writes[0].policy.rules[0].conditions,p.rules[0].conditions);
 assert.equal(f.writes[0].policy.rules[0].share,'masked_text');assert.ok(!JSON.stringify(f.writes).includes(secret));
 const legacy=fixture({existing:true,rulesRevision:null});legacy.apply(p,4);await legacy.read();assert.equal(legacy.panel.ready,false);assert.equal(legacy.writes.length,0);
 const unknown=fixture({existing:true});const future=clone(p);future.rules[0].conditions.unrecognized_filter=true;unknown.apply(future,4);await unknown.read();assert.equal(unknown.panel.ready,false);assert.equal(unknown.writes.length,0);
}
{
 const targets=[{id,kind:'webhook',existing:true,keep:true}];
 const base={id:rule,targets:[id],sender:'',contains:'',line:'',full:false,masked:true,regex:'通知',window:true,start:'09:00',end:'18:00',offset:'480'};
 assert.throws(()=>policyInput(targets,[base],false),/设备尚不支持/);
 const result=policyInput(targets,[base],false,2);assert.equal(result.rules[0].share,'masked_text');assert.equal(result.rules_revision,2);
 for(const changed of [{start:'24:00'},{end:'09:00'},{offset:'841'},{offset:''},{regex:'x'.repeat(257)},{full:true}])assert.throws(()=>policyInput(targets,[{...base,...changed}],false,2));
 // Browser does not execute arbitrary user regex with JS's backtracking engine.
 assert.equal(policyInput(targets,[{...base,regex:'(?=unsupported)'}],false,2).rules[0].conditions.body_regex,'(?=unsupported)');
}
console.log('Extended rule roundtrip, legacy-client guard, privacy mode and bounded form checks passed.');

// Actual candidate mail fields are capability-gated, private and never durable
// browser state. All values below are disposable synthetic test credentials.
{
 const target={id,kind:'email',smtp_host:'smtp.example.invalid',username:'SYNTHETIC_USER',password:'SYNTHETIC_MAIL_PASSWORD',mail_from:'sender@example.invalid',rcpt_to:'recipient@example.invalid'};
 const r={id:rule,targets:[id],sender:'',contains:'',line:'',full:false};
 assert.throws(()=>policyInput([target],[r],false,2),/尚未启用邮件/);
 const p=policyInput([target],[r],false,2,1);assert.equal(p.targets[0].destination.port,465);assert.equal(p.targets[0].destination.tls,'implicit');
 for(const changes of [{smtp_host:'smtp://host'},{smtp_host:'127.0.0.1'},{smtp_host:'smtp.invalid.'},{username:'a\r\nb'},{password:'secret\nAUTH'},{mail_from:'Name <sender@example.invalid>'},{rcpt_to:'first@example.invalid,second@example.invalid'},{rcpt_to:'a..b@example.invalid'},{rcpt_to:'用户@example.invalid'}])assert.throws(()=>policyInput([{...target,...changes}],[r],false,2,1));
 const f=fixture({emailRevision:1});await f.read();f.panel.edit('channel');const row=f.panel.draft.row;row.kind='email';f.panel.channelFields();
 for(const name of ['smtp_host','username','password','mail_from','rcpt_to']){row[name+'Input'].value=target[name];assert.equal(row[name+'Input'].type,'password');}
 await f.panel.action(g=>f.panel.save(g));assert.equal(f.writes.length,1);assert.equal(f.writes[0].policy.targets[0].destination.kind,'email');
 assert.ok(!JSON.stringify(f.markers).includes('SYNTHETIC_MAIL_PASSWORD'));assert.ok(!JSON.stringify(f.archives).includes('recipient@example.invalid'));
 for(const input of f.panel.secretInputs)assert.equal(input.value,'');
 assert.equal(f.panel.draft,null);assert.ok(!allText(f).includes('recipient@example.invalid'));
 f.panel.edit('rule');f.panel.draft.row.contains='new filter';await save(f);assert.deepEqual(f.writes[1].policy.targets[0].destination,{kind:'keep'});
 const old=fixture({emailRevision:null});old.apply({enabled:false,targets:p.targets,rules:p.rules},1);await old.read();assert.equal(old.panel.ready,false);assert.equal(old.writes.length,0);
 // 聊天类通道：凭据格式、完整地址粘贴、可选加签、拒绝会改写地址的字符
{const id='11111111-1111-4111-8111-111111111111',base={id,existing:false,keep:false,endpoint:'',bearer:'',chat:''};const one=(kind,extra)=>policyInput([{...base,kind,...extra}],[],false).targets[0].destination;
 assert.deepEqual(one('dingtalk',{key:'https://oapi.dingtalk.com/robot/send?access_token=abc123',chat:'SECxyz'}),{kind:'dingtalk',access_token:'abc123',secret:'SECxyz'});
 assert.deepEqual(one('dingtalk',{key:'abc123'}),{kind:'dingtalk',access_token:'abc123',secret:null});
 assert.deepEqual(one('wecom',{key:'https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=aa-bb'}),{kind:'wecom',key:'aa-bb'});
 assert.deepEqual(one('feishu',{key:'https://open.feishu.cn/open-apis/bot/v2/hook/hk-1',chat:''}),{kind:'feishu',hook:'hk-1',secret:null});
 assert.deepEqual(one('serverchan',{key:'https://sctapi.ftqq.com/SCT1.send'}),{kind:'serverchan',send_key:'SCT1'});
 assert.deepEqual(one('pushplus',{key:' tok '}),{kind:'pushplus',token:'tok'});
 assert.deepEqual(one('gotify',{key:'A.b_c',endpoint:'https://gotify.example.invalid/'}),{kind:'gotify',server:'https://gotify.example.invalid/',token:'A.b_c'});
 for(const bad of ['','a/b','a?b','a&b','a b','é','a'.repeat(129)])for(const kind of ['dingtalk','wecom','feishu','serverchan','pushplus'])assert.throws(()=>one(kind,{key:bad}),`${kind} ${bad}`);
 assert.throws(()=>one('wecom',{key:'k',chat:'unexpected'}),'no secret where the provider has none');assert.throws(()=>one('dingtalk',{key:'k',chat:'a/b'}));
 assert.throws(()=>one('gotify',{key:'k',endpoint:'http://gotify.example.invalid/'}));assert.throws(()=>one('gotify',{key:'k',endpoint:'https://gotify.example.invalid/x'}));
 assert.equal(chatToken('dingtalk','http://oapi.dingtalk.com/robot/send?access_token=abc'),'http://oapi.dingtalk.com/robot/send?access_token=abc','only https addresses are unpacked');}
console.log('SMTP capability, private input/Keep, no browser persistence and malformed mailbox checks passed.');
}
