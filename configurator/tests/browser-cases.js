// SPDX-License-Identifier: AGPL-3.0-only
// Synthetic Core-format ports only, excluded from the static artifact.
async () => {
 const {PendingStore}=await import('/RustSimHub/configurator/src/pending-store.js');
 const {RecoveryController}=await import('/RustSimHub/configurator/src/recovery-controller.js');
 const {isStaticRequest,BASE}=await import('/RustSimHub/configurator/src/static-policy.js');
 const assert=(ok,message)=>{if(!ok)throw Error(message);};
 const p='a'.repeat(64),id='11111111-1111-4111-8111-111111111111';
 const row={request_id:id,action:'begin_wifi',trial_id:null,submit_started:true};
 const initial={contract:1,device_pin:p,revision:0,requests:[row]};
 const a=new PendingStore(indexedDB,'synthetic-cas'),b=new PendingStore(indexedDB,'synthetic-cas');
 const raced=await Promise.all([a.save(initial,null),b.save({...initial,requests:[]},null)]);
 assert(raced.filter(Boolean).length===1,'CAS permits exactly one initial writer across connections');
 const winner=await a.load(p);assert(winner.revision===0,'winner revision');assert(await a.save({...winner,revision:1},0),'CAS increment');assert(await b.save({...winner,revision:1},0)===false,'stale writer rejected');
 let rejected=false;try{await a.save({...winner,password:'MUST_NOT_PERSIST'},1);}catch{rejected=true;}assert(rejected,'unknown secret field rejected');
 const store=new PendingStore(indexedDB,'synthetic-lost-reply');let writes=0,queries=0;
 const client={connectFromGesture:()=>Promise.resolve(),disconnect:async()=>{},readStatus:async()=>({contract:1,source:'core',wifi:'unknown',busy:false,recovery_available:true}),
  beginWifi:async args=>{const saved=await store.load(p);assert(saved.requests.some(r=>r.request_id===args.requestId&&r.submit_started),'durable marker before call');writes++;throw Error('transport_lost');},
  queryOperation:async args=>{queries++;return {contract:1,source:'core',target_request_id:args.targetRequestId,lookup:'not_found',operation:null};}};
 const c=new RecoveryController({client,store,hidden:()=>false});await c.connect(p);try{await c.write('begin_wifi',{ssid:'SYNTHETIC_SSID',passphrase:'SYNTHETIC_PASSWORD'});}catch{}
 const stored=await store.load(p);assert(stored.requests.length===1,'lost reply retained');assert(!JSON.stringify(stored).includes('SYNTHETIC_'),'no credentials in IDB');
 const resumed=new RecoveryController({client,store,hidden:()=>false});await resumed.connect(p);await resumed.query(stored.requests[0].request_id);assert(writes===1&&queries===1,'resume only queries');assert((await store.load(p)).requests.length===1,'not_found never clears');
 assert(!isStaticRequest(new Request(location.origin+BASE+'index.html',{method:'POST'}),location.origin),'POST never cached');
 assert(!isStaticRequest(new Request(location.origin+BASE+'index.html?password=synthetic'),location.origin),'query never cached');
 assert(!isStaticRequest(new Request(location.origin+BASE+'index.html',{headers:{Authorization:'synthetic'}}),location.origin),'credentials never cached');
 assert(!isStaticRequest(new Request(location.origin+BASE+'tests/mock-provider.js'),location.origin),'mock never cached');
 const {mount}=await import('/RustSimHub/configurator/src/app.js');let cancelCalls=0;
 const trial='d'.repeat(64),uiStore=new PendingStore(indexedDB,'synthetic-ui-cancel');
 const result=(id,action,state)=>({contract:1,source:'core',request_id:id,action,trial_id:trial,state,authoritative:true,settings_saved:false,rollback_verified:state==='rolled_back',expires_in_ms:40000});
 mount({document,store:uiStore,client:{connectFromGesture:async()=>{},readStatus:async()=>({contract:1,source:'core',wifi:'unknown',busy:false,recovery_available:true}),beginWifi:async a=>result(a.requestId,'begin_wifi','connecting'),cancelWifi:async a=>{cancelCalls++;return result(a.requestId,'cancel_wifi','rolled_back');}}});
 document.getElementById('pin').value=p;await document.getElementById('connect').onclick();document.getElementById('ssid').value='synthetic';document.getElementById('password').value='abcdefgh';await document.getElementById('begin').onclick();
 assert(document.getElementById('confirm').disabled,'cannot save connecting trial');assert(!document.getElementById('cancel').disabled,'connecting trial has explicit cancel');await document.getElementById('cancel').onclick();assert(cancelCalls===1&&document.getElementById('cancel').disabled,'cancelled trial cannot restart from stale connecting view');
 return {cas_winners:raced.filter(Boolean).length,writes,queries,credentials_persisted:false};
}
