// SPDX-License-Identifier: AGPL-3.0-only
// Real shipped module, synthetic DOM, keys and requests; no hardware or network.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../crates/edge/src/web/clients.js',import.meta.url));
const {ClientsPanel,scopeNames,keyHex,pendingInput,clientsInput}=await import('data:text/javascript;base64,'+source.toString('base64'));
const html=await readFile(new URL('../crates/edge/src/web/index.html',import.meta.url),'utf8');
const app=await readFile(new URL('../crates/edge/src/web/app.js',import.meta.url),'utf8');
for(const id of ['clients-open-pairing','clients-refresh','clients-state','clients-pending','clients-list'])assert.match(html,new RegExp(`id="${id}"`));
assert.match(html,/允许新设备配对（5 分钟）/);
assert.match(app,/import \{ ClientsPanel \} from '\.\/clients\.js';/);
assert.match(app,/clientsPanel\.reset\(\)/);
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
class Element {constructor(){this.textContent='';this.children=[];this.disabled=false;this.hidden=false;}replaceChildren(...v){this.children=v;this.textContent='';}append(...v){this.children.push(...v);}}
const key=n=>Array.from({length:32},(_,i)=>(n+i)&255);
const toolKey=key(7),otherKey=key(90),ownerKey=key(200);
const pairingId='ab'.repeat(32);
// status.read (bit 0) + network.manage (bit 6), as Core reports the mask.
const mask=1|1<<6;
const pending=()=>[{id:pairingId,public_key:toolKey,kind:'client',scopes:mask,expires_in_seconds:280}];
const ownerRow={id:'owner',public_key:ownerKey,kind:'owner',scopes:1023,generation:1,active:true};
const toolRow=(active=true,generation=1)=>({id:'tool',public_key:toolKey,kind:'client',scopes:mask,generation,active});
function fixture(respond){
 const elements=new Map(),commands=[];let consent=true;const prompts=[];
 const document={getElementById(id){if(!elements.has(id))elements.set(id,new Element());return elements.get(id);},createElement:()=>new Element()};
 const panel=new ClientsPanel({document,request:async c=>{commands.push(structuredClone(c));return respond(c);},confirm:m=>{prompts.push(m);return consent;}});
 return {panel,commands,prompts,$:id=>document.getElementById(id),consent:v=>{consent=v;}};
}
const text=row=>row.children[0].textContent;
const button=row=>row.children[1];
// Pure validators: masks, keys and whole replies fail closed.
assert.deepEqual(scopeNames(mask),['status.read','network.manage']);
for(const bad of [0,-1,1024,1.5,'1',null])assert.throws(()=>scopeNames(bad));
assert.equal(keyHex(toolKey).length,64);
for(const bad of [key(1).slice(1),[...key(1).slice(1),256],'00'.repeat(32),null])assert.throws(()=>keyHex(bad));
for(const bad of [null,{},[{...pending()[0],id:'short'}],[{...pending()[0],kind:'owner'}],[{...pending()[0],scopes:0}],[{...pending()[0],expires_in_seconds:301}],Array(17).fill(pending()[0])])assert.throws(()=>pendingInput(bad));
for(const bad of [null,[{...ownerRow,active:'yes'}],[{...ownerRow,kind:'root'}],[{...ownerRow,public_key:[1]}]])assert.throws(()=>clientsInput(bad));
// Open the window, list the request with full key and scope names, approve echoing it exactly.
{
 let approved=false;
 const f=fixture(async c=>({open_pairing:{ok:true,expires_in_seconds:300},pending_pairings:approved?[]:pending(),clients:approved?[ownerRow,toolRow()]:[ownerRow],approve:{...toolRow()}})[c.op]??(()=>{throw Error('unexpected');})());
 assert.match(f.$('clients-state').textContent,/最近5分钟/);
 assert.equal(await f.panel.openPairing(),true);
 assert.deepEqual(f.commands.map(c=>c.op),['open_pairing','pending_pairings','clients']);
 assert.match(f.prompts[0],/5分钟/);
 assert.match(f.$('clients-state').textContent,/已允许新设备配对5分钟/);
 const row=f.$('clients-pending').children[0];
 assert.match(text(row),new RegExp(`申请编号：${pairingId}`));
 assert.match(text(row),new RegExp(`公钥：${keyHex(toolKey)}`));
 assert.match(text(row),/status\.read（读取状态）、network\.manage（管理网络（含改 Wi-Fi））/);
 assert.match(text(row),/类型：client/);
 // The owner itself is never offered for revocation.
 assert.equal(f.$('clients-list').children.length,1);assert.equal(f.$('clients-list').children[0].children.length,1);
 approved=true;
 f.commands.length=0;await button(row).onclick();await new Promise(r=>setTimeout(r,0));
 assert.deepEqual(f.commands[0],{op:'approve',pairing_id:pairingId,public_key:toolKey,kind:'client',scopes:['status.read','network.manage']});
 assert.match(f.prompts[1],new RegExp(keyHex(toolKey)));
 assert.match(f.$('clients-state').textContent,/已批准/);
 const listed=f.$('clients-list').children[0];
 assert.match(text(listed),new RegExp(keyHex(toolKey)));assert.equal(button(listed).textContent,'撤销');
}
// Declining a confirmation sends nothing.
{
 const f=fixture(async c=>({pending_pairings:pending(),clients:[ownerRow]})[c.op]);
 f.consent(false);assert.equal(await f.panel.openPairing(),true);assert.equal(f.commands.length,0);
 f.consent(true);await f.panel.refresh();f.consent(false);
 await button(f.$('clients-pending').children[0]).onclick();await new Promise(r=>setTimeout(r,0));
 assert.deepEqual(f.commands.map(c=>c.op),['pending_pairings','clients']);
}
// A rejected window (stale owner login) is reported, nothing listed, no retry.
{
 const f=fixture(async c=>{if(c.op==='open_pairing')throw Error('核心拒绝操作；请检查登录、权限和设备状态');throw Error('unexpected');});
 assert.equal(await f.panel.openPairing(),false);
 assert.deepEqual(f.commands.map(c=>c.op),['open_pairing']);
 assert.match(f.$('clients-state').textContent,/核心拒绝操作.*最近5分钟/);
 assert.equal(f.$('clients-open-pairing').disabled,false);
}
// A malformed list renders nothing clickable.
{
 const f=fixture(async c=>({pending_pairings:[{...pending()[0],public_key:[1,2]}],clients:[ownerRow]})[c.op]);
 assert.equal(await f.panel.refresh(),false);assert.equal(f.$('clients-pending').children.length,0);
}
// An approve whose reply is lost is never resent; the next refresh reconciles from Core.
{
 let lost=true;const f=fixture(async c=>{if(c.op==='approve'){if(lost)throw Error('offline');}return {pending_pairings:[],clients:[ownerRow,toolRow()]}[c.op];});
 f.panel.render(f.panel.generation,pendingInput(pending()),clientsInput([ownerRow]));
 await button(f.$('clients-pending').children[0]).onclick();await new Promise(r=>setTimeout(r,0));
 assert.deepEqual(f.commands.map(c=>c.op),['approve']);
 assert.match(f.$('clients-state').textContent,/批准结果未确认.*不会自动重试/);
 await f.panel.refresh();
 assert.deepEqual(f.commands.map(c=>c.op),['approve','pending_pairings','clients']);
 assert.match(f.$('clients-state').textContent,/上次批准已生效/);
}
// An approve reply naming another key/scopes is not reported as success.
{
 const f=fixture(async c=>({approve:{...toolRow(),public_key:otherKey},pending_pairings:[],clients:[ownerRow]})[c.op]);
 f.panel.render(f.panel.generation,pendingInput(pending()),clientsInput([ownerRow]));
 await button(f.$('clients-pending').children[0]).onclick();await new Promise(r=>setTimeout(r,0));
 assert.match(f.$('clients-state').textContent,/不一致/);assert.doesNotMatch(f.$('clients-state').textContent,/已批准/);
 await f.panel.refresh();assert.match(f.$('clients-state').textContent,/未确认.*不会自动重试/);
}
// Revoke sends only the listed key; an unconfirmed revoke is re-read, not resent.
{
 let revoked=false,fail=true;
 const f=fixture(async c=>{if(c.op==='revoke'){if(fail)throw Error('offline');revoked=true;return {ok:true};}return {pending_pairings:[],clients:[ownerRow,toolRow(!revoked)]}[c.op];});
 await f.panel.refresh();const listed=f.$('clients-list').children[0];
 await button(listed).onclick();await new Promise(r=>setTimeout(r,0));
 assert.deepEqual(f.commands.at(-1),{op:'revoke',public_key:toolKey});
 assert.match(f.$('clients-state').textContent,/撤销结果未确认/);
 await f.panel.refresh();assert.match(f.$('clients-state').textContent,/仍显示为有效/);
 fail=false;await button(f.$('clients-list').children[0]).onclick();await new Promise(r=>setTimeout(r,0));
 assert.equal(f.commands.filter(c=>c.op==='revoke').length,2);
 assert.match(f.$('clients-state').textContent,/已撤销/);
 assert.equal(f.$('clients-list').children[0].children.length,1);
}
// Logout mid-request: the late reply renders nothing and buttons from the old view are dead.
{
 const gate=deferred();const f=fixture(async c=>c.op==='pending_pairings'?gate.promise:[ownerRow,toolRow()]);
 const load=f.panel.refresh();f.panel.reset();gate.resolve(pending());assert.equal(await load,false);
 assert.equal(f.$('clients-pending').children.length,0);assert.equal(f.$('clients-list').children.length,0);
 const old=fixture(async c=>({pending_pairings:pending(),clients:[ownerRow]})[c.op]);await old.panel.refresh();
 const stale=button(old.$('clients-pending').children[0]);old.panel.reset();old.commands.length=0;
 await stale.onclick();await new Promise(r=>setTimeout(r,0));assert.equal(old.commands.length,0);
}
// One operation at a time.
{
 const gate=deferred();const f=fixture(async c=>c.op==='pending_pairings'?gate.promise:[ownerRow]);
 const first=f.panel.refresh();assert.equal(await f.panel.openPairing(),false);assert.equal(f.$('clients-refresh').disabled,true);
 gate.resolve([]);assert.equal(await first,true);assert.deepEqual(f.commands.map(c=>c.op),['pending_pairings','clients']);
}
console.log('web clients: pairing window, exact approval echo, revoke, unconfirmed and stale paths ok');
