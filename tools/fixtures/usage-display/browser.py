# SPDX-License-Identifier: AGPL-3.0-only
import argparse,http.server,mimetypes,pathlib,threading
from playwright.sync_api import sync_playwright
ROOT=pathlib.Path(__file__).resolve().parents[3];WEB=ROOT/'crates/edge/src/web'
FILES={'/'+p.name:p for p in WEB.iterdir() if p.is_file() and p.suffix in ('.js','.css','.html')}
FILES['/']=WEB/'index.html';FILES['/usage-fixture']=pathlib.Path(__file__).with_name('index.html')
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path=FILES.get(self.path)
        if path is None:self.send_error(404);return
        self.send_response(200);self.send_header('Content-Type',mimetypes.guess_type(path)[0] or 'application/octet-stream');self.end_headers();self.wfile.write(path.read_bytes())
    def do_POST(self):
        self.send_response(403);self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(b'{"error":"synthetic-unavailable"}')
    def log_message(self,*_):pass

def main():
    p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    s=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);t=threading.Thread(target=s.serve_forever,daemon=True);t.start()
    origin=f'http://127.0.0.1:{s.server_port}'
    try:
        with sync_playwright() as p:
            b=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            try:
                for width in [1440,390,320]:
                    context=b.new_context(viewport={'width':width,'height':900});page=context.new_page();errors=[];posts=[];count_reads=[]
                    page.on('pageerror',lambda e:errors.append(str(e)))
                    def request_sent(request):
                        if request.method!='POST':
                            return
                        if request.url==origin+'/api/usage-count/v1':
                            count_reads.append(request.post_data_json)
                        else:
                            posts.append(request.post_data_json.get('command',{}).get('op'))
                    page.on('request',request_sent)
                    page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort())
                    page.goto(origin+'/usage-fixture',wait_until='networkidle');page.wait_for_function('window.ready===true');assert page.evaluate('calls')==0;assert page.evaluate('document.documentElement.scrollWidth<=innerWidth');page.keyboard.press('Tab');assert page.evaluate('document.activeElement.tagName')=='SELECT';page.keyboard.press('Tab');page.keyboard.press('Enter');page.wait_for_function('calls===1');assert '缓存已过期' in page.locator('#stats').inner_text();assert '42' in page.locator('#stats').inner_text();page.screenshot(path=str(a.output/f'usage-{width}.png'),full_page=True);assert not errors
                    page.goto(origin+'/',wait_until='networkidle');page.wait_for_function("document.getElementById('test-state').textContent.includes('初始化未确认')");assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),f'actual daily page overflow at {width}'
                    assert page.locator('#test-center').is_visible();assert page.locator('#account-panel').get_attribute('open') is None
                    for id in ['phone-panel','sms-panel','ap-wifi-panel']:page.locator('#'+id+' > summary').click();assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),(width,id)
                    page.screenshot(path=str(a.output/f'daily-{width}.png'),full_page=True)
                    assert all(op in ['identity','challenge'] for op in posts),posts
                    assert count_reads==[{}],count_reads
                    assert not errors,errors;context.close();print(f'PASS: Chromium {width}px actual daily closed/expanded panels and isolated usage window/cache/manual keyboard display; no business actions')
            finally:b.close()
    finally:s.shutdown();s.server_close();t.join(timeout=5)
if __name__=='__main__':main()
