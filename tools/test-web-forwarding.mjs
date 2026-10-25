// SPDX-License-Identifier: AGPL-3.0-only
// Execute the real panel, only synthetic Core replies and disposable DOM/storage.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {webcrypto} from 'node:crypto';
const source=await readFile(new URL('../crates/edge/src/web/forwarding.js',import.meta.url));
const {ForwardingPanel,policyInput,pendingInput,processText}=await import('data:text/javascript;base64,'+source.toString('base64'));
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
function fixture({existing=false,managed=true,consent=true,onRequest,onSave,onLoad}={}){
 const elements=new Map(),$=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const doc={getElementById:$,createElement:tag=>new Element(tag)};
 let token=true,pending=null,policy={enabled:false,targets:existing?[{id,destination:{kind:'webhook',url:endpoint,bearer:secret}}]:[],rules:existing?[{id:rule,targets:[id],sender_equals:'sender',body_contains:null,line:null,share:'notice'}]:[]};
 let current=existing?1:0;const calls=[],writes=[],archives=[],markers=[],requests=new Map(),confirmations=[];
 const status=()=>({available:true,configured:current>0,generation:current,enabled:policy.enabled,configuration_api:true,retained_credentials_api:true,request_receipt_api:true,service_management_api:managed,process_state_verified:true,state:policy.enabled?'running':'disabled'});
 const view=()=>({enabled:policy.enabled,generation:current,targets:policy.targets.map(t=>({id:t.id,kind:t.destination.kind,credentials_redacted:true})),rules:clone(policy.rules)});
 const panel=new ForwardingPanel({document:doc,crypto:webcrypto,getToken:()=>token,confirm:message=>{confirmations.push(message);return consent;},
  loadPending:async()=>{await onLoad?.();return clone(pending);},savePending:async value=>{await onSave?.(value);pending=clone(value);markers.push(clone(value));},archivePending:async value=>{archives.push(clone(value));},
  request:async c=>{
   calls.push(clone(c));const custom=await onRequest?.(c);if(custom!==undefined)return custom;
   if(c.op==='forwarding_status')return status();
   if(c.op==='forwarding_rules')return view();
   if(c.op==='forwarding_request'){const gen=requests.get(c.request_id);return {request_id:c.request_id,known:!!gen,applied_generation:gen??null,current_generation:current,not_found_is_not_proof:true};}
   if(c.op==='forwarding_receipts')return {receipts:[]};
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
assert.doesNotMatch(source.toString(),/setInterval|setTimeout|localStorage|innerHTML/);
console.log('PASS: actual forwarding panel, redacted keep edits, durable metadata before one submit, exact receipt reconciliation, no auto retry/poll, input privacy, limits and logout/storage races; synthetic only.');
