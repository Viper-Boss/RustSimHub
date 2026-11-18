// SPDX-License-Identifier: AGPL-3.0-only
// Real production module, synthetic records only; --before names the frozen old module.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {harness} from './test-web-console.mjs';
const source=await readFile(process.argv[2]||new URL('../crates/edge/src/web/console.js',import.meta.url),'utf8');
const mod=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const Dashboard=mod.ConsoleDashboard, before=!!process.argv[2];
const row=n=>({id:String(n),display:{sender:'长中文发送者'.repeat(8),preview:'迟到短信 '+n,timestamp:n%2?'2001-01-01T01:00:00Z':'非法时间'}});
let passed=0,failed=0;
async function test(name,run){try{await run();passed++;console.log('PASS '+name);}catch(e){failed++;console.log('FAIL '+name+': '+e.message);if(!before)throw e;}}
function fixture(){let ids=Array.from({length:40},(_,i)=>40-i),visibility='0',realm='e',line=7,hold=null;let reads=0;const state={setIds:v=>ids=v,setVisibility:v=>visibility=v,setRealm:v=>realm=v,setLine:v=>line=v,hold:f=>hold=f,get reads(){return reads;}};const x=harness(c=>{if(c.op==='line_status')return {status:'ready',line};if(c.op==='calls_recent')return {calls:[],total:0,cursor:{db_epoch:realm,seq:'0'}};if(c.op==='inbox_recent'){reads++;const selected=ids.filter(n=>!c.before||n<Number(c.before.seq)).slice(0,c.limit),result={messages:selected.map(row),total:ids.length,visibility_seq:visibility,cursor:{db_epoch:realm,seq:String(selected.at(-1)||0)}};if(hold&&c.before)return hold(result);return result;}return {};},{Dashboard});return {...x,state};}
async function continuation(h,state){
 let release;
 state.hold(result=>new Promise(resolve=>{release=()=>resolve(result);}));
 h.go('comms');
 const reading=h.readLists();
 for(let i=0;i<30&&!release;i++)await Promise.resolve();
 assert(release);
 return {reading,release};
}
await test('rapid SMS tab switching shares one in-flight read',async()=>{
 let locked=false,release,hold=true,collisions=0;
 const page={messages:[row(4)],total:1,visibility_seq:'0',cursor:{db_epoch:'e',seq:'4'}};
 const {h}=harness(c=>{
  if(locked){collisions++;throw Error('device busy');}
  if(c.op==='inbox_recent'&&c.limit===32&&hold){
   hold=false;locked=true;
   return new Promise(resolve=>{release=()=>{locked=false;resolve(page);};});
  }
  if(c.op==='inbox_recent')return page;
  if(c.op==='calls_recent')return {calls:[],cursor:{db_epoch:'e',seq:'0'},total:0};
  return {line:7,status:'ready'};
 },{Dashboard});
 await h.signedIn();
 h.go('comms');
 for(let i=0;i<10&&!release;i++)await Promise.resolve();
 assert(release);
 for(let i=0;i<8;i++){h.go('comms',3);h.go('comms',0);}
 await Promise.resolve();release();await h.readLists();
 assert.equal(collisions,0);
 assert.deepEqual(h.mail.map(m=>m.id),['4']);
});
await test('temporary list errors retry without dropping SMS, denied access clears them',async()=>{
 let errors=[],attempts=0;
 const page={messages:[row(3)],total:1,visibility_seq:'0',cursor:{db_epoch:'e',seq:'3'}};
 const {h,$}=harness(c=>{
  if(c.op!=='inbox_recent')return {};
  attempts++;
  if(errors.length)throw errors.shift();
  return page;
 },{Dashboard,readDelay:async()=>{}});
 await h.signedIn();h.go('comms');await h.readLists();
 const busy=()=>Object.assign(Error('busy'),{coreCode:'busy'});
 attempts=0;errors=[busy(),Object.assign(Error('edge'),{httpStatus:503})];
 await h.readLists(true);assert.equal(attempts,3);assert.equal(h.mail.length,1);
 attempts=0;errors=Array.from({length:4},busy);
 await h.readLists(true);assert.equal(attempts,4);assert.equal(h.mail.length,1);
 assert.match($('c-mail-operation').textContent,/暂忙/);
 attempts=0;errors=[Object.assign(Error('revoked'),{coreCode:'denied'})];
 await h.readLists(true);assert.equal(attempts,1);assert.equal(h.mail.length,0);
 assert.match($('c-mail-empty').textContent,/登录或权限/);
});
await test('same-total visibility changes reject a partial history',async()=>{
 const {h,state}=fixture();await h.signedIn();let reads=0;
 state.hold(result=>{reads++;return {...result,visibility_seq:'2'};});
 h.go('comms');await h.readLists();assert.equal(reads,1);assert.equal(h.mail.length,0);
});
await test('automatic history after arrivals or deletion has no hidden gap',async()=>{
 const {h,state}=fixture();await h.signedIn();h.go('comms');await h.readLists();
 state.setIds(Array.from({length:60},(_,i)=>60-i));await h.readLists();
 assert.equal(h.mail.length,60);
 state.setIds(Array.from({length:59},(_,i)=>59-i));state.setVisibility('1');
 await h.readLists();assert.equal(h.mail.length,59);assert.ok(h.mail.every(m=>m.id!=='60'));
});
await test('late continuation cannot reattach after explicit refresh',async()=>{
 const {h,state}=fixture();await h.signedIn();const old=await continuation(h,state);
 state.hold(null);state.setIds([9,8,7]);const latest=h.readLists(true);old.release();await Promise.all([old.reading,latest]);
 assert.deepEqual(h.mail.map(m=>m.id),['9','8','7']);
});
await test('missing visibility fails closed for old servers',async()=>{
 const {h,state}=fixture();state.setVisibility(undefined);await h.signedIn();h.go('comms');await h.readLists();assert.equal(h.mail.length,0);
});
await test('newer explicit head refresh wins a late older refresh',async()=>{let release,block=false;const page=n=>({messages:[row(n)],total:1,visibility_seq:'0',cursor:{db_epoch:'e',seq:String(n)}});const {h}=harness(c=>{if(c.op==='inbox_recent'&&block){block=false;return new Promise(resolve=>release=()=>resolve(page(1)));}if(c.op==='inbox_recent')return page(2);if(c.op==='calls_recent')return {calls:[],total:0,cursor:{db_epoch:'e',seq:'0'}};return {line:7,status:'ready'};},{Dashboard});await h.signedIn();block=true;const older=h.readLists(true);for(let i=0;i<10&&!release;i++)await Promise.resolve();assert(release);const latest=h.readLists(true);release();await Promise.all([older,latest]);assert.deepEqual(h.mail.map(m=>m.id),['2']);});
await test('logout ignores late continuation and resets login counts',async()=>{
 const {h,state,$}=fixture();await h.signedIn();const old=await continuation(h,state);
 h.reset();old.release();await old.reading;assert.equal(h.mail.length,0);assert.equal($('c-mh-q-sms').textContent,'—');
});
await test('invalid calendar is unknown rather than silently normalized',async()=>{assert.equal(mod.shortTime('2026-02-30T12:00:00Z'),'时间未知');});
if(!before){await test('time zones, leap day, raw values and unknown direction',async()=>{const now=new Date('2026-11-07T12:00:00Z');assert.equal(mod.smsTime('2026-11-07T12:00:00+08:00',now).full,'2026-11-07 04:00:00（本地时间）');assert.equal(mod.smsTime('2024-02-29 03:04:05',now).full,'2024-02-29 03:04:05（时区未知）');for(const s of ['',null,'2026-02-29T00:00:00Z','2026-01-01T25:00:00Z','2026-01-01T00:00:00+09:99'])assert.equal(mod.smsTime(s,now).short,'时间未知');const raw='2026-11-07T01:02:03.123456789-0330';assert.equal(mod.smsTime(raw,now).raw,raw);assert.match(mod.callView({state:'ended',direction:99}).label,/方向未知/);assert.equal(mod.callView({state:'garbage',direction:1}).live,false);});}
console.log(JSON.stringify({mode:before?'before-fix':'current',passed,failed}));
if(before)assert(failed>=5,'negative controls must detect at least five independent old-production failures');
