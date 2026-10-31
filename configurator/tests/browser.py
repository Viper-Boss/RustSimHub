# SPDX-License-Identifier: AGPL-3.0-only
"""Actual Chromium IDB CAS, frozen-path SW/offline/update guards and responsive UI."""
import argparse,http.server,mimetypes,pathlib,shutil,sys,tempfile,threading,urllib.parse
from playwright.sync_api import sync_playwright
ROOT=pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/site-build'))
from build import assemble,FILES,DASHBOARD
class Handler(http.server.BaseHTTPRequestHandler):
    artifact=None
    def do_GET(self):
        parsed=urllib.parse.urlparse(self.path);name=parsed.path.removeprefix('/RustSimHub/')
        if parsed.path=='/RustSimHub/configurator/':name='configurator/index.html'
        if not parsed.path.startswith('/RustSimHub/') or parsed.query or name not in set(DASHBOARD)|{'configurator/'+f for f in FILES}:self.send_error(404);return
        data=(type(self).artifact/name).read_bytes();self.send_response(200);self.send_header('Content-Type',mimetypes.guess_type(name)[0] or 'application/octet-stream');self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(data)
    def log_message(self,*_):pass

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=pathlib.Path,required=True);a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rsh-config-browser-') as tmp:
        tmp=pathlib.Path(tmp);first=tmp/'site1';assemble(first)
        source=tmp/'source';shutil.copytree(ROOT/'configurator',source/'configurator');shutil.copytree(ROOT/'docs/progress',source/'docs/progress');target=source/'configurator/src/contracts.js';target.write_text(target.read_text()+'\n// synthetic static update test\n');second=tmp/'site2';assemble(second,source)
        Handler.artifact=first;server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        origin=f'http://127.0.0.1:{server.server_port}';url=origin+'/RustSimHub/configurator/'
        try:
            with sync_playwright() as p:
                browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
                try:
                    for width in (1440,390,320):
                        context=browser.new_context(viewport={'width':width,'height':900});page=context.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)));page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort())
                        page.goto(url,wait_until='networkidle');page.wait_for_function("document.getElementById('state').textContent.includes('尚未安装')")
                        assert page.locator('#connect').is_disabled();assert page.locator('#begin').is_disabled();assert page.evaluate('document.documentElement.scrollWidth<=innerWidth');page.keyboard.press('Tab');assert page.evaluate('document.activeElement.id')=='pin'
                        page.screenshot(path=str(a.output/f'configurator-{width}.png'),full_page=True);assert not errors
                        await_ready="async()=>{await navigator.serviceWorker.ready;return true;}";page.evaluate(await_ready);page.reload(wait_until='networkidle');context.set_offline(True);page.reload(wait_until='domcontentloaded');page.wait_for_selector('#state');assert '尚未安装' in page.locator('#state').inner_text();context.set_offline(False)
                        if width==320:
                            result=page.evaluate((ROOT/'configurator/tests/browser-cases.js').read_text());assert result=={'cas_winners':1,'writes':1,'queries':1,'credentials_persisted':False};print('PASS: actual Chromium IndexedDB transaction CAS and lost-reply/reload query-only')
                            # Verify native gesture reaches the injected selection callback directly.
                            for code,label in [('pin_mismatch','公钥不匹配'),('not_preapproved','未提前登记'),('revoked','已撤销')]:
                                page.evaluate("""async code=>{const {mount}=await import('./src/app.js');window.gesture=false;mount({client:{connectFromGesture:()=>{window.gesture=navigator.userActivation.isActive;return Promise.reject({contract:1,code});}},store:{load:async()=>null}});document.getElementById('pin').value='a'.repeat(64);} """,code)
                                page.click('#connect');page.wait_for_function('(label)=>document.getElementById("state").textContent.includes(label)',arg=label);assert page.evaluate('window.gesture')
                            page.reload(wait_until='networkidle')
                            page.evaluate("""async()=>{const {PendingStore}=await import('./src/pending-store.js');const s=new PendingStore(indexedDB);await s.save({contract:1,device_pin:'c'.repeat(64),revision:0,requests:[{request_id:'22222222-2222-4222-8222-222222222222',action:'begin_wifi',trial_id:null,submit_started:true}]},null);window.noReloadMarker=true;}""")
                            Handler.artifact=second
                            page.evaluate('async()=>{const r=await navigator.serviceWorker.getRegistration();await r.update();}')
                            page.wait_for_function('async()=>!!(await navigator.serviceWorker.getRegistration()).waiting')
                            page.click('#update');page.wait_for_function("document.getElementById('offline').textContent.includes('原请求')")
                            page.evaluate("async()=>{(await navigator.serviceWorker.getRegistration()).waiting.postMessage({type:'activate_static_update'});}")
                            page.wait_for_timeout(200);assert page.evaluate('async()=>!!(await navigator.serviceWorker.getRegistration()).waiting');assert page.evaluate('window.noReloadMarker===true')
                            # Only this fresh browser profile's synthetic record is cleared by the harness.
                            page.evaluate("""async()=>{const {PendingStore}=await import('./src/pending-store.js');const s=new PendingStore(indexedDB);await s.save({contract:1,device_pin:'c'.repeat(64),revision:1,requests:[]},0);} """)
                            other=context.new_page();other.goto(url,wait_until='networkidle');page.evaluate("async()=>{(await navigator.serviceWorker.getRegistration()).waiting.postMessage({type:'activate_static_update'});}");page.wait_for_timeout(200);assert page.evaluate('async()=>!!(await navigator.serviceWorker.getRegistration()).waiting');other.close()
                            # Delay only the external SW registry read. Real app/controller,
                            # IDB and waiting worker remain active throughout this race.
                            page.evaluate("""async()=>{const {mount}=await import('./src/app.js');window.registryRead=navigator.serviceWorker.getRegistration.bind(navigator.serviceWorker);window.registryBlocked=false;window.registryRelease=null;navigator.serviceWorker.getRegistration=async(...args)=>{window.registryBlocked=true;await new Promise(resolve=>window.registryRelease=resolve);return window.registryRead(...args);};window.raceController=mount({client:{connectFromGesture:async()=>{},readStatus:async()=>({contract:1,source:'core',wifi:'disconnected',busy:false,recovery_available:true}),disconnect:async()=>{}}});document.getElementById('pin').value='a'.repeat(64);}""")
                            page.click('#update');page.wait_for_function('window.registryBlocked===true');assert page.locator('#connect').is_disabled()
                            page.evaluate("async()=>{await raceController.connect('a'.repeat(64));await raceController.disconnect();registryRelease();}")
                            page.wait_for_function("document.getElementById('offline').textContent.includes('保留现有页面')");page.evaluate('navigator.serviceWorker.getRegistration=window.registryRead')
                            assert page.evaluate('async()=>!!(await navigator.serviceWorker.getRegistration()).waiting');assert page.evaluate('window.noReloadMarker===true');assert not page.locator('#connect').is_disabled()
                            # A fresh explicit click may activate; the page stays loaded and
                            # prevents a new device connection until a manual reopen.
                            page.click('#update');page.wait_for_function("document.getElementById('offline').textContent.includes('手动重新打开')");page.wait_for_function('async()=>!(await navigator.serviceWorker.getRegistration()).waiting');assert page.evaluate('window.noReloadMarker===true');assert page.locator('#connect').is_disabled();assert page.locator('#update').is_disabled();print('PASS: real waiting SW retains pending/other-window/session-race guards; explicit activation pauses new UI connections without forced reload')
                            Handler.artifact=first
                        context.close();print(f'PASS: Chromium {width}px default-deny UI, keyboard, no overflow and cached offline reopening')
                finally:browser.close()
        finally:server.shutdown();server.server_close();thread.join(timeout=5)
if __name__=='__main__':main()
