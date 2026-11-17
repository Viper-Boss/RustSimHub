// SPDX-License-Identifier: AGPL-3.0-only
// Reuse A's exact finite loopback backend; replace only its unavailable Python
// browser client with this package's existing-Node-Playwright client.
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {createServer} from 'node:http';
import {createHash} from 'node:crypto';
import {readFile,mkdtemp} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';

const backend=new URL('../test-web-app-assembly.mjs',import.meta.url);
const raw=await readFile(backend);
assert.equal(createHash('sha1').update(`blob ${raw.length}\0`).update(raw).digest('hex'),
  '50a2a8e0f59b6c4f32516baa43c67aad16138c11','Review changed backend before adapting its driver');
const artifacts=await mkdtemp(join(tmpdir(),'offline-b-browser-'));
let denied=0,launched=0;
// Browser background HTTP/CONNECT requests are denied locally, never forwarded.
const proxy=createServer((req,res)=>{denied++;res.writeHead(403,{'Connection':'close'});res.end();});
proxy.on('connect',(_req,socket)=>{denied++;socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');});
await new Promise(resolve=>proxy.listen(0,'127.0.0.1',resolve));
const original=childProcess.spawn;
childProcess.spawn=(command,args,options)=>{
  assert.equal(launched++,0,'Only the fixed browser child may be started by this adapter');
  assert.equal(command,process.env.WEB3_WEB_BROWSER_PYTHON||'python3');
  assert.equal(args.length,2);
  assert.equal(args[0],fileURLToPath(new URL('../fixtures/web3-web/assembly-browser.py',import.meta.url)));
  assert.match(args[1],/^http:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}$/);
  return original(process.execPath,[fileURLToPath(new URL('./browser-driver.mjs',import.meta.url)),args[1]],
    {...options,env:{...options.env,OFFLINE_B_BROWSER_ARTIFACTS:artifacts,
      OFFLINE_B_DENY_PROXY:`http://127.0.0.1:${proxy.address().port}`}});
};
syncBuiltinESMExports();
try{
  await import(backend.href);
  assert.equal(launched,1);
  console.log(`PASS: B Node browser subset completed; original backend unchanged; background proxy denied=${denied}; artifacts=${artifacts}`);
}finally{
  childProcess.spawn=original;syncBuiltinESMExports();
  await new Promise(resolve=>proxy.close(resolve));
}
