// SPDX-License-Identifier: AGPL-3.0-only
// An external port may settle normally during synchronous AbortSignal cleanup.
// Expiry must win; otherwise an aborted call can become a successful receipt.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {RecoveryController} from '../configurator/src/recovery-controller.js';
const load=async name=>import('data:text/javascript;base64,'+(await readFile(new URL(`../crates/edge/src/web/${name}.js`,import.meta.url))).toString('base64'));
const {UsageStatsPanel}=await load('usage-stats');
const {ConfigurationState}=await load('configuration-state');
const {DiagnosticPanel}=await load('diagnostics');
class Element{constructor(){this.style={};this.children=[];this.value='';}setAttribute(){}replaceChildren(...v){this.children=v;}append(...v){this.children.push(...v);}set innerHTML(_){throw Error('text only');}}
const document={createElement:()=>new Element()},id='11111111-1111-4111-8111-111111111111';
const failures=[];
async function scenario(name,test){
 const originalSet=globalThis.setTimeout,originalClear=globalThis.clearTimeout,tasks=new Map();let next=0,aborted=0,calls=0;
 globalThis.setTimeout=(fn,ms)=>{const key=++next;tasks.set(key,{fn,ms});return key;};globalThis.clearTimeout=key=>tasks.delete(key);
 const port=value=>({signal})=>{calls++;return new Promise(resolve=>signal.addEventListener('abort',()=>{aborted++;resolve(value);},{once:true}));};
 const expire=async ms=>{for(let n=0;n<50&&!tasks.size;n++)await Promise.resolve();assert.equal(tasks.size,1,'one bounded deadline');const [key,task]=[...tasks][0];assert.equal(task.ms,ms);tasks.delete(key);task.fn();};
 try{await test({port,expire});assert.equal(calls,1,'no automatic retry');assert.equal(aborted,1,'port is still cancelled');assert.equal(tasks.size,0,'deadline cleared');console.log(`PASS: ${name}`);}catch(error){failures.push(`${name}: ${error.message}`);}finally{globalThis.setTimeout=originalSet;globalThis.clearTimeout=originalClear;}
}
await scenario('usage deadline refuses success returned during Abort cleanup',async({port,expire})=>{
 const panel=new UsageStatsPanel({document,root:new Element(),read:port({status:'success',entry:{window:'24h',count:42,source:'read_only_transport',observed_at_secs:1793203200,succeeded_at_secs:1793203201,expires_at_secs:1793203801,freshness_revoked:false}})});
 const pending=panel.refresh();await expire(10000);await pending;assert.match(panel.state.textContent,/未确认/);assert.ok(panel.rows.children.some(e=>e.textContent==='未知'));assert.equal(panel.button.disabled,false);
});
await scenario('configuration deadline refuses verified running returned during Abort cleanup',async({port,expire})=>{
 const panel=new ConfigurationState({document,root:new Element(),read:port({source:'forwarding',age_ms:0,data:{configuration_applied:true,process_state_verified:true,enabled:true,state:'running'}})});
 const pending=panel.refresh();await expire(3000);await pending;assert.equal(panel.snapshot,null);assert.match(panel.output.textContent,/未确认/);assert.equal(panel.button.disabled,false);
});
await scenario('diagnostic deadline preserves original unknown receipt',async({port,expire})=>{
 const read=port({operation:{id,type:'call',state:'submitted',submit_started:true}}),panel=new DiagnosticPanel({document,root:new Element(),queryOperation:(_,options)=>read(options)});panel.update({operation:{id,type:'call',state:'unknown',submit_started:true}});
 const pending=panel.query();await expire(3000);await pending;assert.equal(panel.summary.operation.state,'unknown');assert.match(panel.message.textContent,/未确认/);assert.equal(panel.queryButton.disabled,false);
});
await scenario('recovery deadline retains write marker and rejects Abort-time ready receipt',async({port,expire})=>{
 let record=null;const store={load:async()=>structuredClone(record),save:async(next,revision)=>{assert.equal(record?.revision??null,revision);record=structuredClone(next);return true;}};
 const begin=port({contract:1,source:'core',request_id:id,action:'begin_wifi',trial_id:'b'.repeat(64),state:'ready',authoritative:true,settings_saved:false,rollback_verified:false,expires_in_ms:40000});
 const client={connectFromGesture:async()=>{},readStatus:async()=>({contract:1,source:'core',wifi:'disconnected',busy:false,recovery_available:true}),beginWifi:begin};
 const controller=new RecoveryController({client,store,hidden:()=>false,uuid:()=>id});await controller.connect('a'.repeat(64));
 const pending=controller.write('begin_wifi',{ssid:'synthetic',passphrase:'abcdefgh'});const rejected=assert.rejects(pending,/unknown/);await expire(3000);await rejected;
 assert.deepEqual(record.requests,[{request_id:id,action:'begin_wifi',trial_id:null,submit_started:true}]);assert.equal(controller.operations.has(id),false);assert.equal(controller.busy,false);
});
assert.deepEqual(failures,[]);
