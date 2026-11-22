// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {connect} from 'node:net';
import test from 'node:test';
import {denyProxy} from './deny-proxy.mjs';

test('owned proxy refuses HTTP/CONNECT and survives actual local TCP resets', {timeout:10000},async()=>{
  const proxy=await denyProxy();
  const port=Number(new URL(proxy.origin).port);
  async function request(raw,reset=false){
    return new Promise((resolve,reject)=>{
      const socket=connect({host:'127.0.0.1',port});let response='';
      socket.setTimeout(2000,()=>socket.destroy(Error('local proxy test timeout')));
      socket.on('error',reject);
      socket.on('connect',()=>socket.write(raw));
      socket.on('data',data=>{
        response+=data.toString();
        if(reset)socket.resetAndDestroy();
      });
      socket.on('close',()=>resolve(response));
    });
  }
  try{
    // .invalid addresses are text only: this sink has no DNS/upstream code.
    const get='GET http://offline-b.invalid/ HTTP/1.1\r\nHost: offline-b.invalid\r\n\r\n';
    const tunnel='CONNECT offline-b.invalid:443 HTTP/1.1\r\nHost: offline-b.invalid\r\n\r\n';
    for(let i=0;i<16;i++)assert.match(await request(tunnel,true),/^HTTP\/1\.1 403/);
    assert.match(await request(get),/^HTTP\/1\.1 403/);
    assert.match(await request(tunnel),/^HTTP\/1\.1 403/);
    assert.equal(proxy.stats.denied,18);assert.deepEqual(proxy.stats.unexpected,[]);
    assert(proxy.stats.disconnected>0,'Actual refused connection resets must exercise the error handler');
    console.log('PASS: real TCP reset/refusal regression; 18 local requests, no forwarding');
  }finally{await proxy.close();}
});
