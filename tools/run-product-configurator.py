#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual built candidate in Chromium: CSP, WASM, native crypto and offline SW."""
import argparse,hashlib,http.server,json,mimetypes,pathlib,select,subprocess,threading,time
from playwright.sync_api import sync_playwright
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE='/RustSimHub/configurator/'
def run(site,binary,output):
    output.mkdir(mode=0o700)
    peer=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    lock=threading.Lock()
    allowed={('/RustSimHub/'+str(p.relative_to(site))):p for p in site.rglob('*') if p.is_file()}
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*_):pass
        def send(self,body,mime):
            self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Content-Length',str(len(body)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
        def do_GET(self):
            name=BASE+'index.html' if self.path==BASE else self.path
            if name in allowed:return self.send(allowed[name].read_bytes(),mimetypes.guess_type(name)[0] or 'application/octet-stream')
            if name=='/csp-probe.js':return self.send(b"try{eval('1');window.cspBlocked=false}catch{window.cspBlocked=true}",'text/javascript')
            if name=='/product-crypto.js':
                source=(ROOT/'tools/test-web-recovery-session.mjs').read_text().replace("'/wasm/","'"+BASE+"wasm/").replace("'/configurator/src/","'"+BASE+"src/")
                return self.send(source.encode(),'text/javascript')
            self.send_error(404)
        def do_POST(self):
            assert self.path=='/peer';n=int(self.headers.get('Content-Length','0'));assert 0<n<8192;body=self.rfile.read(n);json.loads(body)
            with lock:
                peer.stdin.write(body+b'\n');peer.stdin.flush();assert select.select([peer.stdout],[],[],3)[0];reply=peer.stdout.readline(8193);assert len(reply)<=8192
            self.send(reply,'application/json')
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
    try:
        with sync_playwright() as p:
            browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            context=browser.new_context(viewport={'width':390,'height':900})
            context.add_init_script("window.chooserCount=0;Object.defineProperty(navigator,'bluetooth',{value:{requestDevice(){window.chooserCount++;window.chooserGesture=navigator.userActivation.isActive;return Promise.reject(Error('unsupported'));}}});")
            page=context.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
            def wait(expression):
                deadline=time.monotonic()+10
                while not page.evaluate(expression):
                    assert time.monotonic()<deadline,expression
                    page.wait_for_timeout(25)
            origin=f'http://127.0.0.1:{server.server_port}'
            page.goto(origin+BASE,wait_until='networkidle');wait("!document.getElementById('connect').disabled")
            assert page.evaluate('chooserCount')==0
            assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
            page.add_script_tag(url=origin+'/csp-probe.js');assert page.evaluate('cspBlocked'),'CSP accepted dynamic JS'
            page.locator('summary').click();page.fill('#pin','1'*64);page.fill('#binding-realm','11111111-1111-4111-8111-111111111111');page.click('#binding-key');wait("document.getElementById('binding-public').value.length===64")
            page.fill('#binding-grant','22222222-2222-4222-8222-222222222222');page.fill('#binding-generation','1');page.click('#binding-save');wait("document.getElementById('binding-state').textContent.includes('回执已保存')")
            page.click('#connect');wait('chooserCount===1');assert page.evaluate('chooserGesture')
            page.evaluate("async()=>{await navigator.serviceWorker.ready;}");page.reload(wait_until='networkidle');context.set_offline(True);page.reload(wait_until='networkidle');wait("!document.getElementById('connect').disabled");assert page.evaluate('chooserCount')==0
            cached=page.evaluate("async()=>{const rows=[];for(const key of await caches.keys())for(const r of await(await caches.open(key)).keys())rows.push(new URL(r.url).pathname);return rows;}")
            assert BASE+'wasm/rustsimhub_recovery_session_bg.wasm' in cached and all(u.startswith(BASE) and '/peer' not in u for u in cached)
            context.set_offline(False)
            # Same candidate resources, actual native/WASM/WebCrypto and native IDB.
            page.add_script_tag(url=origin+'/product-crypto.js',type='module');wait('typeof runRecoverySessionChecks==="function"')
            crypto=page.evaluate('runRecoverySessionChecks()');assert crypto['passed']>=18
            assert not errors,errors
            report={'built_candidate':True,'csp_no_dynamic_javascript':True,'wasm_loaded':True,'static_only_offline_cache':True,'gesture_before_await':True,'automatic_device_selection':False,'native_crypto':crypto,'browser':browser.version,'physical_radio':False,'published':False,'cached_files':len(cached)}
            (output/'report.json').write_text(json.dumps(report,indent=2)+'\n');page.screenshot(path=str(output/'candidate.png'),full_page=True);browser.close();print(json.dumps({'browser':report['browser'],'native_crypto_passed':crypto['passed'],'offline_csp_candidate':True}))
    finally:
        server.shutdown();server.server_close();peer.stdin.close()
        try:peer.wait(timeout=3)
        except subprocess.TimeoutExpired:peer.kill();peer.wait(timeout=3)
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--site',type=pathlib.Path,required=True);p.add_argument('--native-peer',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();run(a.site.absolute(),a.native_peer.absolute(),a.output.absolute())
