// SPDX-License-Identifier: AGPL-3.0-only
// Production mount/controller/store; only DOM, IDB and SW service ports are synthetic.
import assert from 'node:assert/strict';
import {mount} from '../src/app.js';
const pin='a'.repeat(64),turn=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
class Element{constructor(){this.value='';this.disabled=false;this.textContent='';}replaceChildren(){}append(){}}
function fixture({rows=[],openFailure=false}={}){
 const open=deferred(),registration=deferred(),selection=deferred(),elements=new Map(),sent=[],calls=[];let registrations=0,closed=0;
 const $=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
 const document={hidden:false,getElementById:$,createElement:()=>new Element(),addEventListener(){}};
 globalThis.indexedDB={open(){const q={};open.promise.then(()=>{if(openFailure){q.onerror?.();return;}q.result={objectStoreNames:{length:1,contains:n=>n==='records'},close(){closed++;},transaction(_store,mode){assert.equal(mode,'readonly');const tx={objectStore:()=>({getAll(){const request={};queueMicrotask(()=>{request.result=rows;request.onsuccess?.();tx.oncomplete?.();});return request;}})};return tx;}};q.onsuccess?.();});return q;}};
 Object.defineProperty(globalThis,'navigator',{configurable:true,value:{serviceWorker:{getRegistration(){registrations++;return registration.promise;}}}});
 const client={connectFromGesture(){calls.push('gesture');return selection.promise;},readStatus:async()=>({contract:1,source:'core',wifi:'disconnected',busy:false,recovery_available:true}),disconnect:async()=>{calls.push('disconnect');}};
 const controller=mount({client,store:{load:async()=>null},document});$('pin').value=pin;
 return {$,document,controller,open,registration,selection,sent,calls,registrations:()=>registrations,closed:()=>closed,worker:{postMessage:value=>sent.push(value)}};
}
for(const phase of ['storage','registration']){
 const f=fixture(),update=f.$('update').onclick();
 if(phase==='registration'){f.open.resolve();await turn();assert.equal(f.registrations(),1);}
 // The host changes the actual controller while an update check is pending.
 const connecting=f.controller.connect(pin);await turn();assert.equal(f.controller.busy,true);
 f.open.resolve();f.registration.resolve({waiting:f.worker});await update;
 assert.equal(f.sent.length,0,`A connection during ${phase} must cancel the old update intent`);
 f.selection.resolve();await connecting;await f.controller.disconnect();assert.match(f.$('offline').textContent,/保留/);
}
for(const phase of ['storage','registration']){
 const f=fixture(),update=f.$('update').onclick();
 if(phase==='registration'){f.open.resolve();await turn();}
 f.selection.resolve();await f.controller.connect(pin);await f.controller.disconnect();assert.equal(f.controller.busy,false);assert.equal(f.controller.connected,false);
 f.open.resolve();f.registration.resolve({waiting:f.worker});await update;
 assert.equal(f.sent.length,0,'A completed connect/disconnect must still invalidate the old update intent');
}
for(const phase of ['storage','registration']){
 const f=fixture(),update=f.$('update').onclick();
 if(phase==='registration'){f.open.resolve();await turn();}
 f.document.hidden=true;f.open.resolve();f.registration.resolve({waiting:f.worker});await update;assert.equal(f.sent.length,0,'Hidden pages cannot request activation');
}
{
 const f=fixture(),update=f.$('update').onclick();await f.$('update').onclick();
 assert.ok(f.$('update').disabled);assert.ok(f.$('connect').disabled);await f.$('connect').onclick();assert.equal(f.calls.length,0,'UI cannot start a connection while update admission is pending');
 f.open.resolve();f.registration.resolve({waiting:f.worker});await update;
 assert.deepEqual(f.sent,[{type:'activate_static_update'}]);assert.ok(f.$('connect').disabled);assert.ok(f.$('update').disabled);await f.$('connect').onclick();await f.$('update').onclick();assert.equal(f.calls.length,0);assert.equal(f.sent.length,1);assert.equal(f.closed(),1);assert.match(f.$('offline').textContent,/手动重新打开/);
}
for(const options of [{rows:[{unexpected:'invalid metadata'}]},{openFailure:true}]){
 const f=fixture(options),update=f.$('update').onclick();f.open.resolve();await update;assert.equal(f.sent.length,0);assert.equal(f.registrations(),0);assert.equal(f.$('update').disabled,false);assert.equal(f.$('connect').disabled,false);
}
{
 const f=fixture(),update=f.$('update').onclick();f.open.resolve();f.registration.resolve({waiting:null});await update;assert.equal(f.sent.length,0);assert.equal(f.$('update').disabled,false);assert.equal(f.$('connect').disabled,false);
}
for(const phase of ['storage','registration']){
 const f=fixture(),update=f.$('update').onclick();
 if(phase==='registration'){f.open.resolve();await turn();assert.equal(f.registrations(),1);}
 await update;assert.equal(f.sent.length,0);assert.equal(f.$('update').disabled,false);assert.equal(f.$('connect').disabled,false);
 f.open.resolve();f.registration.resolve({waiting:f.worker});await turn();assert.equal(f.sent.length,0,'Late completion after the three-second admission budget cannot post activation');
}
console.log('PASS: actual PWA update admission fences storage/registration races, completed session replacement, hidden state, duplicate clicks and new UI connections; no forced refresh or record writes');
