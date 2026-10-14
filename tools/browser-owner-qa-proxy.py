#!/usr/bin/env python3
"""Loopback-only development proxy; pinned upstream TLS, no credential logging."""
import argparse,http.client,json,pathlib,socketserver,ssl,threading
from http.server import BaseHTTPRequestHandler,HTTPServer
ROOT=pathlib.Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--fixture',type=pathlib.Path,required=True);p.add_argument('--certificate',type=pathlib.Path,required=True);p.add_argument('--port',type=int,default=18448);p.add_argument('--source-client',action='store_true');a=p.parse_args()
assert 1024<=a.port<=65535
fixture=json.loads(a.fixture.read_text(encoding='utf-8'));assert fixture['hardware_access_disabled'] and not fixture['original_data_read']
address=fixture['wifi_address'];upstream=fixture['upstream_origin'];origin='http://127.0.0.1:'+str(a.port)
assert upstream=='https://'+address+':8448'
context=ssl.create_default_context(cafile=str(a.certificate));assert context.check_hostname and context.verify_mode==ssl.CERT_REQUIRED
slots=threading.BoundedSemaphore(8)
class Handler(BaseHTTPRequestHandler):
 protocol_version='HTTP/1.1'
 def log_message(self,*args):pass
 def answer(self,status,body=b'',content='application/json'):
  self.send_response(status);self.send_header('Content-Type',content);self.send_header('Content-Length',str(len(body)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
 def do_GET(self):self.forward()
 def do_POST(self):self.forward()
 def forward(self):
  if len(self.headers.get_all('Host',[]))!=1 or self.headers.get('Host')!='127.0.0.1:'+str(a.port) or self.path not in ['/','/app.js','/proof.js','/style.css','/api/v2']:return self.answer(403)
  if self.headers.get_all('Transfer-Encoding',[]):return self.answer(400)
  if self.command=='POST' and (self.path!='/api/v2' or self.headers.get_all('Origin',[])!=[origin] or self.headers.get_all('X-RustSimHub-Request',[])!=['2'] or self.headers.get_all('Content-Type',[])!=['application/json']):return self.answer(403)
  if self.command=='GET' and self.path=='/api/v2':return self.answer(405)
  if not slots.acquire(blocking=False):return self.answer(503)
  try:
   self.connection.settimeout(5)
   lengths=self.headers.get_all('Content-Length',[])
   if len(lengths)>1:return self.answer(400)
   length=int(lengths[0]) if lengths else 0
   if not 0<=length<=65536:return self.answer(413)
   if self.command=='GET' and length:return self.answer(400)
   body=self.rfile.read(length);assert len(body)==length
   if a.source_client and self.command=='GET' and self.path=='/app.js':
    return self.answer(200,(ROOT/'crates/edge/src/web/app.js').read_bytes(),'text/javascript; charset=utf-8')
   c=http.client.HTTPSConnection(address,8448,context=context,timeout=15)
   try:
    c.request(self.command,self.path,body=body if length else None,headers={'Host':address+':8448','Origin':upstream,'Content-Type':'application/json','X-RustSimHub-Request':'2'})
    r=c.getresponse();data=r.read(65537);assert len(data)<=65536
    self.send_response(r.status)
    for name,value in r.getheaders():
     if name.lower() not in ['connection','transfer-encoding','content-length','date','server']:self.send_header(name,value)
    self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
   finally:c.close()
  except (BrokenPipeError,ConnectionResetError):pass
  except Exception:
   try:self.answer(502)
   except (BrokenPipeError,ConnectionResetError):pass
  finally:slots.release()
class Server(socketserver.ThreadingMixIn,HTTPServer):daemon_threads=True
server=Server(('127.0.0.1',a.port),Handler)
print('Offline first-owner browser QA proxy ready; upstream CA/IP verified; loopback only',flush=True)
server.serve_forever()
