# SPDX-License-Identifier: AGPL-3.0-only
"""Actual homepage/CSP; loopback-only synthetic denial and signed Owner login."""
import argparse,http.server,json,mimetypes,pathlib,re,threading
from playwright.sync_api import sync_playwright
ROOT=pathlib.Path(__file__).resolve().parents[3];WEB=ROOT/'crates/edge/src/web'
SOURCE=(ROOT/'crates/edge/src/linux.rs').read_text()
FILES={'/':WEB/'index.html',**{'/'+name:WEB/name for name in re.findall(r'include_str!\("web/([^"/]+)"\)',SOURCE) if name!='index.html'}}
CSP=re.search(r'\("content-security-policy","([^"]+)"\)',SOURCE).group(1)
class Handler(http.server.BaseHTTPRequestHandler):
    operations=[]
    def do_GET(self):
        file=FILES.get(self.path)
        if file is None:self.send_error(404);return
        data=file.read_bytes();self.send_response(200);self.send_header('Content-Type',mimetypes.guess_type(file)[0] or 'application/octet-stream');self.send_header('Content-Security-Policy',CSP);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        size=int(self.headers.get('Content-Length','0'))
        if self.path!='/api/v2' or not 0<size<=65536:self.send_error(400);return
        value=json.loads(self.rfile.read(size));type(self).operations.append(value.get('command',{}).get('op'))
        self.send_response(403);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(b'{"error":"synthetic-auth-unavailable"}')
    def log_message(self,*_):pass

def authenticated_configuration(page,width,output):
    # Only the external API is synthetic. The app, IndexedDB key, proof.js,
    # session setup, reader and ConfigurationState execute unchanged.
    page.evaluate("window.fixtureDevice=crypto.subtle.generateKey({name:'Ed25519'},false,['sign','verify'])")
    device=page.evaluate("async()=>Array.from(new Uint8Array(await crypto.subtle.exportKey('raw',(await fixtureDevice).publicKey)))")
    commands=[];proof={};state={'enabled':True,'verified':True}
    def api(route):
        body=route.request.post_data_json;c=body['command'];op=c['op'];commands.append(op)
        if op=='challenge':
            result=page.evaluate("""async client=>{const pair=await fixtureDevice;const id='1'.repeat(64),device=Array.from(new Uint8Array(await crypto.subtle.exportKey('raw',pair.publicKey)));const transcript=Array.from(new TextEncoder().encode(JSON.stringify({domain:'RustSimHub.Auth.v1',purpose:'login',instance:'2'.repeat(64),challenge_id:id,device_public_key:device,client_public_key:client,kind:'owner',scopes:1023,generation:1})));return {id,expires_in_seconds:60,transcript,device_signature:Array.from(new Uint8Array(await crypto.subtle.sign('Ed25519',pair.privateKey,new Uint8Array(transcript))))};}""",c['public_key'])
            proof.update(client=c['public_key'],transcript=result['transcript'])
        elif op=='authenticate':
            assert c['challenge_id']=='1'*64
            assert page.evaluate("""async x=>{const key=await crypto.subtle.importKey('raw',new Uint8Array(x.client),{name:'Ed25519'},false,['verify']);return crypto.subtle.verify('Ed25519',key,new Uint8Array(x.signature),new Uint8Array(x.transcript));}""",{**proof,'signature':c['signature']})
            result={'token':'c'*64,'expires_in_seconds':900}
        else:
            assert c.get('token')=='c'*64,'Unexpected session in local fixture'
            if op=='forwarding_status':result={'available':True,'configuration_api':True,'service_management_api':True,'service_action':'completed','process_state_verified':state['verified'],'enabled':state['enabled'],'state':'running' if state['enabled'] else 'disabled','generation':1,'configuration_applied':True}
            elif op=='line_status':result={'status':'ready','line':7}
            elif op=='call_status':result={'available':True,'live_calls':[],'active_calls':0}
            elif op=='inbox':result={'messages':[],'cursor':None}
            elif op=='calls':result={'calls':[],'cursor':None}
            elif op=='health':result={'history_read_api':False,'wifi_provisioning_api':False}
            elif op in ['network_status','network_diagnostics','sms_management_status']:result={'available':False}
            elif op=='configuration_indicator':
                assert c['active'] is False
                result={}
            else:raise AssertionError(f'Unexpected operation: {op}')
        route.fulfill(status=200,content_type='application/json',body=json.dumps({'protocol':2,'id':body['id'],'result':result}))
    page.route('**/api/v2',api)
    page.locator('#pin').evaluate("e=>{for(let p=e.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;}")
    page.locator('#pin').fill(bytes(device).hex())
    page.evaluate("document.querySelectorAll('details[data-configuration]').forEach(e=>e.open=false)")
    if not page.locator('#account-panel').evaluate('e=>e.open'):page.locator('#account-panel > summary').click()
    page.locator('#daily-login').click();page.wait_for_function("() => !document.querySelector('#support-configuration button').disabled")
    assert commands.count('challenge')==1 and commands.count('authenticate')==1
    assert commands.count('forwarding_status')==0,'Login/render must not fetch configuration'
    for enabled in [True,False]:
        state['enabled']=enabled;page.locator('#support-configuration button').click()
        expected='服务运行已核对' if enabled else '服务关闭已核对'
        page.wait_for_function('(text) => document.querySelector("#support-configuration [role=status]").textContent.includes(text)',arg=expected)
        assert '本次保存意图尚未确认' in page.locator('#support-configuration').inner_text()
    assert commands.count('forwarding_status')==2
    page.wait_for_function("() => document.querySelector('#support-configuration [role=status]').textContent.includes('陈旧或未知')",timeout=7000)
    assert commands.count('forwarding_status')==2,'Expiry must not poll or retry'
    state['verified']=False;page.locator('#support-configuration button').click();page.wait_for_function("() => document.querySelector('#support-configuration [role=status]').textContent.includes('查询未确认')")
    assert commands.count('forwarding_status')==3
    assert page.locator('#support-usage button').is_disabled();assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
    page.screenshot(path=str(output/f'configuration-reader-{width}.png'),full_page=True)
    page.locator('#daily-logout').click();assert page.locator('#support-configuration button').is_disabled()
    assert commands.count('forwarding_status')==3
    print(f'PASS: actual homepage {width}px, real Ed25519 synthetic Owner login, manual forwarding read ON/OFF/unknown, 5-second staleness without polling, no save inference or forwarding/communications writes')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=pathlib.Path,required=True);args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    assert '/test-center.js' in FILES,'apply reviewed static-route proposal first'
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start();origin=f'http://127.0.0.1:{server.server_port}'
    try:
        with sync_playwright() as p:
            browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            try:
                for width in [1440,390,320]:
                    Handler.operations=[];context=browser.new_context(viewport={'width':width,'height':900});page=context.new_page();errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
                    page.add_init_script("window.copies=[];Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>copies.push(text)},configurable:true});window.cspViolations=[];addEventListener('securitypolicyviolation',e=>cspViolations.push(e.effectiveDirective));")
                    page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort());page.goto(origin,wait_until='networkidle')
                    page.wait_for_function("() => document.querySelector('#test-component-host [role=status]').textContent.includes('初始化未确认')")
                    assert Handler.operations==['identity'],Handler.operations
                    page.evaluate("""async()=>{const q=indexedDB.open('rustsimhub-client-v1',1);const db=await new Promise((resolve,reject)=>{q.onsuccess=()=>resolve(q.result);q.onerror=reject;});const t=db.transaction('state','readwrite'),s=t.objectStore('state');s.put({type:'sms',id:'11111111-1111-4111-8111-111111111111',state:'unknown',submit_started:true,recipient:'MUST-NOT-COPY',text:'MUST-NOT-COPY',token:'MUST-NOT-COPY'},'pending');s.put('a'.repeat(64),'pin');await new Promise((resolve,reject)=>{t.oncomplete=resolve;t.onabort=reject;});db.close();}""")
                    page.reload(wait_until='networkidle');page.wait_for_function("() => document.querySelector('#test-component-host [role=status]').textContent.includes('初始化未确认')")
                    assert Handler.operations==['identity','identity'],Handler.operations
                    for id in ['test-check','test-start','test-incoming-start','test-sms','test-forwarding','test-stop','test-recover']:assert not page.locator('#'+id).is_visible()
                    assert page.locator('#test-component-host button').count()==7;assert page.locator('[data-task=outgoing]').is_disabled();assert page.locator('[data-task=stop]').is_disabled();assert page.locator('#test-copy').is_visible()
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth');assert '10010 / CXYE' in page.locator('#test-center').inner_text()
                    page.keyboard.press('Tab');assert page.evaluate('document.activeElement.dataset.task')=='check';assert page.locator('[data-task=check]').evaluate('e=>getComputedStyle(e).outlineStyle')=='solid'
                    page.keyboard.press('Enter');page.wait_for_function("() => document.querySelector('#test-component-host [role=status]').textContent.includes('测试停止')")
                    assert Handler.operations==['identity','identity','challenge'],Handler.operations
                    assert page.locator('#test-component-host button').first.evaluate('e=>parseFloat(getComputedStyle(e).minHeight)')>=44
                    # Native observer projects current host display, without a business request.
                    page.evaluate("document.getElementById('test-state').textContent='合成当前显示';document.getElementById('ap-wifi-state').textContent+='合成更新';")
                    page.wait_for_function("() => document.querySelector('#test-component-host [role=status]').textContent==='合成当前显示'")
                    assert Handler.operations==['identity','identity','challenge']
                    page.locator('#support-panel > summary').click()
                    assert '11111111-1111-4111-8111-111111111111' in page.locator('#support-receipt').inner_text()
                    assert page.locator('#support-receipt button').nth(0).is_disabled();assert page.locator('#support-receipt button').nth(1).is_disabled()
                    assert page.locator('#support-configuration button').is_disabled();assert page.locator('#support-usage button').is_disabled();assert '未知' in page.locator('#support-usage').inner_text()
                    assert page.locator('#support-diagnostics button').first.is_disabled();page.locator('#support-diagnostics button').nth(1).click();page.wait_for_function('() => copies.length===1')
                    copied=page.evaluate('copies[0]');assert 'MUST-NOT-COPY' not in copied and 'a'*64 not in copied and '127.0.0.1' not in copied
                    diagnostic=json.loads(copied);assert diagnostic['session']=='unknown' and diagnostic['certificate']=='unknown';assert diagnostic['operation']['id']=='11111111-1111-4111-8111-111111111111'
                    assert Handler.operations==['identity','identity','challenge'],Handler.operations
                    for id in ['phone-panel','sms-panel','ap-wifi-panel']:page.locator('#'+id+' > summary').click();assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    assert all(op in ['identity','challenge'] for op in Handler.operations),Handler.operations
                    assert not errors,errors;assert page.evaluate('cspViolations')==[]
                    page.screenshot(path=str(args.output/f'assembled-center-{width}.png'),full_page=True);print(f'PASS: actual assembled homepage {width}px, strict CSP, single task controls, real retained receipt/redacted copy, unavailable readers, keyboard/login/observer/manual panels; no submit')
                    authenticated_configuration(page,width,args.output)
                    assert not errors,errors;assert page.evaluate('cspViolations')==[];context.close()
            finally:browser.close()
    finally:server.shutdown();server.server_close();thread.join(timeout=5)
if __name__=='__main__':main()
