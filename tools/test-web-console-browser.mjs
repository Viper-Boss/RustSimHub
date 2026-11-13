// SPDX-License-Identifier: AGPL-3.0-only
// Reuse assembly fixture's actual static route/CSP/Auth, extend only synthetic API data.
import assert from 'node:assert/strict';
import {readFile,writeFile,unlink} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const source=new URL('./test-web-app-assembly.mjs',import.meta.url);
let fixture=await readFile(source,'utf8');
function replace(from,to){assert.equal(fixture.split(from).length,2,'fixture seam changed: '+from);fixture=fixture.replace(from,to);}
replace("let calls=[],assets=[],failures=[],challenges=new Map(),sequence=0,mode='ready',held=null;",`let calls=[],assets=[],failures=[],challenges=new Map(),sequence=0,mode='ready',held=null;
let consoleData={head:40,deleted:[],visibility:'0',realm:'fixture',line:7};
let airsimData={enabled:true,pcm_rate:8000,pairings:[],window_seconds:null};
const sms=n=>({id:String(n),line:n%2?7:6,binary:n===8,display:{sender:'长中文服务通知'.repeat(6),timestamp:n===40?'2001-01-01T01:02:03Z':n===39?'2026-02-30T00:00:00Z':'2026-11-11 12:00:00',preview:'通知'+n+'：这是很长的中文合成短信内容。'.repeat(28)}});
function inboxPage(c){const rows=Array.from({length:consoleData.head},(_,i)=>consoleData.head-i).filter(n=>!consoleData.deleted.includes(n));const page=rows.filter(n=>!c.before||n<Number(c.before.seq)).slice(0,16);return {messages:page.map(sms),total:rows.length,visibility_seq:consoleData.visibility,cursor:{db_epoch:consoleData.realm,seq:String(page.at(-1)||0)}};}`);
replace("case 'line_status':return {status:'ready',line:7};","case 'line_status':return {status:'ready',line:consoleData.line};\n case 'airsim_status':return airsimData;");
replace("case 'inbox_recent':return {messages:[],cursor:null,total:0};","case 'inbox_recent':return inboxPage(c);\n case 'sms_detail':return {sender:sms(Number(c.sms_id)).display.sender,timestamp:sms(Number(c.sms_id)).display.timestamp,text:sms(Number(c.sms_id)).display.preview,data:[]};\n case 'notification_read':return {ok:true};");
replace("case 'calls_recent':return {calls:[],cursor:null,total:0};","case 'calls_recent':return {calls:[{id:'ended-fixture',direction:1,state:'ended',availability:'reconciling'},{id:'unknown-fixture',direction:99,state:'unknown'}],total:2,cursor:{db_epoch:consoleData.realm,seq:'1'}};");
replace("const value=JSON.parse(raw);mode=value.mode||'ready';","const value=JSON.parse(raw);mode=value.mode||'ready';if(value.consoleData)Object.assign(consoleData,value.consoleData);if(value.airsimData)Object.assign(airsimData,value.airsimData);");
replace("const result=await handle(body.command);","if(body.command.op==='airsim_status'&&mode==='airsim-denied'){res.writeHead(503);res.end();return;}\n  if((body.command.op==='inbox_recent'&&mode==='denied')||(body.command.op==='challenge'&&mode==='deny-login')){res.writeHead(403);res.end();return;}\n  const result=await handle(body.command);");
replace("if(body.command.op==='nkn_status'&&mode==='held')await new Promise(resolve=>{held=resolve;});","if(body.command.op==='inbox_recent'&&body.command.before&&mode==='held')await new Promise(resolve=>{held=resolve;});");
replace("./fixtures/web3-web/assembly-browser.py","./test-web-console-browser.py");
const temporary=new URL('./test-web-console-fixture-'+process.pid+'.mjs',import.meta.url);
try{await writeFile(temporary,fixture);const child=spawn(process.execPath,[fileURLToPath(temporary)],{stdio:'inherit',env:process.env});assert.equal(await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);}),0);}finally{await unlink(temporary).catch(()=>{});}
