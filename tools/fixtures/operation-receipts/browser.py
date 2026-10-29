# SPDX-License-Identifier: AGPL-3.0-only
import argparse,http.server,pathlib,threading
from playwright.sync_api import sync_playwright
ROOT=pathlib.Path(__file__).resolve().parents[3]
FILES={'/':pathlib.Path(__file__).with_name('index.html'),**{'/'+n:ROOT/'crates/edge/src/web'/n for n in ['operation-receipt.js','configuration-state.js']}}
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        p=FILES.get(self.path)
        if p is None:self.send_error(404);return
        data=p.read_bytes();self.send_response(200);self.send_header('Content-Type','text/javascript; charset=utf-8' if p.suffix=='.js' else 'text/html; charset=utf-8');self.end_headers();self.wfile.write(data)
    def log_message(self,*_):pass

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--output',type=pathlib.Path,required=True);a=parser.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    try:
        with sync_playwright() as p:
            b=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            try:
                for width in [1440,390,320]:
                    page=b.new_page(viewport={'width':width,'height':900});errors=[];page.on('pageerror',lambda e:errors.append(str(e)));origin=f'http://127.0.0.1:{server.server_port}'
                    page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort());page.goto(origin,wait_until='networkidle');page.wait_for_function('window.ready===true')
                    assert page.evaluate('calls.length')==0
                    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
                    assert page.locator('#receipt button').nth(1).is_disabled()
                    page.keyboard.press('Tab');assert page.evaluate('document.activeElement.textContent')=='查询原操作';page.keyboard.press('Enter');page.wait_for_function('calls.length===1')
                    page.keyboard.press('Tab');assert page.evaluate('document.activeElement.textContent')=='只读查询配置状态';page.keyboard.press('Enter');page.wait_for_function('calls.length===2');assert '服务进程状态未确认' in page.locator('#configuration').inner_text()
                    page.screenshot(path=str(a.output/f'receipts-{width}.png'),full_page=True);assert not errors;page.close();print(f'PASS: receipt/configuration Chromium {width}px, long help, no overflow, keyboard actions and disabled archive')
            finally:b.close()
    finally:server.shutdown();server.server_close();thread.join(timeout=5)
if __name__=='__main__':main()
