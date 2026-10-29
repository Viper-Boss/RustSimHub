# SPDX-License-Identifier: AGPL-3.0-only
"""Local-only Chromium checks; no device, business request or external resources."""
import argparse
import http.server
import pathlib
import re
import threading
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[3]
FILES = {'/': pathlib.Path(__file__).with_name('index.html'),
         '/fixture.js': pathlib.Path(__file__).with_name('fixture.js'),
         '/style.css': ROOT / 'crates/edge/src/web/style.css',
         '/test-center.js': ROOT / 'crates/edge/src/web/test-center.js'}
CSP = re.search(r'\("content-security-policy","([^"]+)"\)',
                (ROOT / 'crates/edge/src/linux.rs').read_text()).group(1)

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        path = FILES.get(self.path)
        if path is None:
            self.send_error(404)
            return
        content = path.read_bytes()
        self.send_response(200)
        mime = {'.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html'}[path.suffix]
        self.send_header('Content-Type', mime + '; charset=utf-8')
        self.send_header('Content-Security-Policy', CSP)
        self.send_header('Content-Length', str(len(content)))
        self.end_headers()
        self.wfile.write(content)

    def log_message(self, *_):
        pass

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(executable_path='/usr/bin/chromium', headless=True, args=['--no-sandbox'])
            try:
                for width in (1440, 390, 320):
                    page = browser.new_page(viewport={'width': width, 'height': 900})
                    errors = []
                    page.on('pageerror', lambda error: errors.append(str(error)))
                    origin = f'http://127.0.0.1:{server.server_port}'
                    page.route('**/*', lambda route: route.continue_() if route.request.url.startswith(origin + '/') else route.abort())
                    page.goto(origin, wait_until='networkidle')
                    page.wait_for_function('window.ready === true')
                    assert page.evaluate('window.cspViolations') == [], page.evaluate('window.cspViolations')
                    assert page.locator('.test-center-actions').evaluate("e=>getComputedStyle(e).display") == 'flex'
                    assert page.locator('[data-task="check"]').evaluate("e=>parseFloat(getComputedStyle(e).minHeight)") >= 44
                    assert page.evaluate('window.calls.length') == 0
                    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), width
                    assert page.locator('details').get_attribute('open') is None
                    assert page.locator('[data-task="outgoing"]').is_disabled()
                    assert '合成禁止原因' in page.locator('#synthetic-test-center-outgoing-reason').inner_text()
                    # Native keyboard navigation skips disabled submit controls.
                    page.keyboard.press('Tab')
                    assert page.evaluate('document.activeElement.dataset.task') == 'check'
                    assert page.locator('[data-task="check"]').evaluate("e=>getComputedStyle(e).outlineStyle") == 'solid'
                    reason_id = page.locator('[data-task="check"]').get_attribute('aria-describedby')
                    assert page.locator('#' + reason_id).count() == 1
                    page.keyboard.press('Enter')
                    assert page.evaluate('window.calls') == ['check']
                    page.keyboard.press('Tab')
                    assert page.evaluate('document.activeElement.dataset.task') == 'recover'
                    page.keyboard.press('Tab')
                    assert page.evaluate('document.activeElement.tagName') == 'SUMMARY'
                    page.keyboard.press('Enter')
                    assert page.locator('details').get_attribute('open') is not None
                    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), width
                    page.screenshot(path=str(args.output / f'test-center-{width}.png'), full_page=True)
                    assert not errors, errors
                    page.close()
                    print(f'PASS: Chromium {width}px under actual Edge CSP, external stylesheet/focus/44px controls, no overflow or policy violations; one explicit check callback')
            finally:
                browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)

if __name__ == '__main__':
    main()
