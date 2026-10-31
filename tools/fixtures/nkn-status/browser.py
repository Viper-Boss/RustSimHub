# SPDX-License-Identifier: AGPL-3.0-only
"""Actual production module/Chromium; external status provider is explicitly synthetic."""
import argparse
import http.server
import pathlib
import threading
from playwright.sync_api import sync_playwright

ROOT=pathlib.Path(__file__).resolve().parents[3]
FILES={**{'/'+name:pathlib.Path(__file__).with_name(name) for name in ['fixture.js','style.css']},
       '/':pathlib.Path(__file__).with_name('index.html'),
       '/nkn-status.js':ROOT/'crates/edge/src/web/nkn-status.js'}

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        target=FILES.get(self.path)
        if target is None:self.send_error(404);return
        data=target.read_bytes();self.send_response(200)
        self.send_header('Content-Type','text/javascript; charset=utf-8' if target.suffix=='.js' else 'text/css; charset=utf-8' if target.suffix=='.css' else 'text/html; charset=utf-8')
        self.send_header('Content-Security-Policy',"default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'")
        self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def log_message(self,*_):pass

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=pathlib.Path,required=True);args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start();origin=f'http://127.0.0.1:{server.server_port}'
    try:
        with sync_playwright() as p:
            browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            try:
                for width in [1440,390,320]:
                    page=browser.new_page(viewport={'width':width,'height':900});errors=[];page.on('pageerror',lambda e:errors.append(str(e)));page.add_init_script("window.violations=[];addEventListener('securitypolicyviolation',e=>violations.push(e.effectiveDirective));")
                    page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort());page.goto(origin,wait_until='networkidle')
                    assert page.locator('#unavailable button').is_disabled();assert page.evaluate('synthetic.calls')==0
                    page.keyboard.press('Tab');assert page.evaluate("document.activeElement.textContent==='只读查询 NKN 状态'")
                    assert page.locator('#status button').evaluate('e=>getComputedStyle(e).outlineStyle')=='solid'
                    page.keyboard.press('Enter');page.wait_for_function('synthetic.calls===1');assert '离线模拟' in page.locator('#status').inner_text();assert '18446744073709551615 B' in page.locator('#status').inner_text();assert '连接节点数：2（不是用户数或安装数）' in page.locator('#status').inner_text()
                    assert page.locator('#status button').evaluate('e=>e.getBoundingClientRect().height>=44');assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    page.wait_for_function("()=>document.querySelector('#status [role=status]').textContent.includes('陈旧或年龄未知')",timeout=7000);assert page.evaluate('synthetic.calls')==1
                    page.evaluate("synthetic.data={...synthetic.data,sequence:2,enabled:'off',path:'relay',dht:'connected',peer_count:0}");page.locator('#status button').click();page.wait_for_function('synthetic.calls===2');text=page.locator('#status').inner_text();assert '开关：关闭\n服务：运行中' in text;assert '链路：中继' in text and 'DHT：已连接' in text
                    page.evaluate("synthetic.data={...synthetic.data,token:'SYNTHETIC_SECRET'}");page.locator('#status button').click();page.wait_for_function("()=>document.querySelector('#status [role=status]').textContent.includes('状态格式或上下文未确认')");assert 'SYNTHETIC_SECRET' not in page.locator('#status').inner_text()
                    page.evaluate("delete synthetic.data.token;synthetic.data={...synthetic.data,generation:1,sequence:9}");page.locator('#status button').click();page.wait_for_function("()=>document.querySelector('#status button').disabled");count=page.evaluate('synthetic.calls');page.evaluate('panel.reset()');assert page.evaluate('synthetic.calls')==count
                    page.locator('#status button').click();page.wait_for_function('(count)=>synthetic.calls===count+1',arg=count);assert '开关：关闭' in page.locator('#status').inner_text()
                    assert not errors,errors;assert page.evaluate('violations')==[];assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    page.screenshot(path=str(args.output/f'nkn-status-{width}.png'),full_page=True);page.evaluate('panel.dispose()');page.close();print(f'PASS NKN status {width}px: actual module, exact u64, independent states, native stale timer, keyboard/44px/CSP, manual-only synthetic reads, context reset',flush=True)
            finally:browser.close()
    finally:server.shutdown();server.server_close();thread.join(timeout=5)

if __name__=='__main__':main()
