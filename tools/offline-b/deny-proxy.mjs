// SPDX-License-Identifier: AGPL-3.0-only
// An owned loopback sink, never a forwarding proxy.
import {createServer} from 'node:http';

export async function denyProxy(){
  const stats={denied:0,disconnected:0,unexpected:[]},sockets=new Set();
  function failure(error){
    if(['ECONNRESET','EPIPE'].includes(error.code))stats.disconnected++;
    else if(stats.unexpected.length<16)stats.unexpected.push(error.code||'UNKNOWN');
  }
  const server=createServer((_req,res)=>{
    stats.denied++;res.writeHead(403,{'Connection':'close'});res.end();
  });
  server.on('connection',socket=>{
    sockets.add(socket);socket.once('close',()=>sockets.delete(socket));
    // CONNECT upgrades transfer socket ownership away from HTTP's error handler.
    // A client resetting a refused tunnel must not crash the fixture process.
    socket.on('error',failure);socket.setTimeout(2000,()=>socket.destroy());
  });
  server.on('connect',(_req,socket)=>{
    stats.denied++;socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
  });
  server.maxConnections=32;server.headersTimeout=2000;server.requestTimeout=2000;
  await new Promise((resolve,reject)=>{
    server.once('error',reject);server.listen(0,'127.0.0.1',resolve);
  });
  server.on('error',failure);
  return {stats,origin:`http://127.0.0.1:${server.address().port}`,
    close:async()=>{
      for(const socket of sockets)socket.destroy();
      await new Promise(resolve=>server.close(resolve));
    }};
}
