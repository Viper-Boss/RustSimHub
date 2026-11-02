#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run actual Chromium/WASM/WebCrypto against a bounded synthetic native peer."""
import argparse
import hashlib
import json
from pathlib import Path
import select
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from playwright.sync_api import sync_playwright

REPO = Path(__file__).resolve().parents[1]


def run(binary, wasm, output):
    peer = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def send(self, data, mime):
            self.send_response(200)
            self.send_header('Content-Type', mime)
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Content-Security-Policy', "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; object-src 'none'; base-uri 'none'")
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            if self.path == '/':
                return self.send(b'<!doctype html><script type="module" src="/lab.js"></script>', 'text/html')
            if self.path == '/lab.js':
                return self.send((REPO / 'tools/test-web-recovery-session.mjs').read_bytes(), 'text/javascript')
            if self.path in ['/configurator/src/crypto/secure-session.js', '/configurator/src/security/recovery-binding.js', '/configurator/src/production-backend.js', '/configurator/src/gatt-framing.js', '/configurator/src/contracts.js']:
                path = (REPO / self.path.lstrip('/')).resolve()
                if path.is_relative_to(REPO / 'configurator/src') and not any(p.is_symlink() for p in [path, *path.parents]) and path.is_file():
                    return self.send(path.read_bytes(), 'text/javascript')
            if self.path in ['/wasm/rustsimhub_recovery_session.js', '/wasm/rustsimhub_recovery_session_bg.wasm']:
                path = wasm / self.path.rsplit('/', 1)[1]
                return self.send(path.read_bytes(), 'application/wasm' if path.suffix == '.wasm' else 'text/javascript')
            self.send_error(404)

        def do_POST(self):
            try:
                length = int(self.headers.get('Content-Length', '0'))
                assert self.path == '/peer' and 0 < length <= 8192
                data = self.rfile.read(length)
                json.loads(data)
                with lock:
                    peer.stdin.write(data + b'\n')
                    peer.stdin.flush()
                    assert select.select([peer.stdout], [], [], 3)[0], 'owned native peer deadline'
                    reply = peer.stdout.readline(8193)
                    assert len(reply) <= 8192
                self.send(reply, 'application/json')
            except Exception:
                self.send(b'{"error":"fixture rejected"}', 'application/json')

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(executable_path='/usr/bin/chromium', headless=True, args=['--no-sandbox'])
            page = browser.new_page()
            page.goto(f'http://127.0.0.1:{server.server_port}/')
            page.wait_for_function('typeof runRecoverySessionChecks === "function"')
            report = page.evaluate('runRecoverySessionChecks()')
            report['browser'] = browser.version
            browser.close()
        report['wasm_sha256'] = hashlib.sha256((wasm / 'rustsimhub_recovery_session_bg.wasm').read_bytes()).hexdigest()
        report['wasm_bytes'] = (wasm / 'rustsimhub_recovery_session_bg.wasm').stat().st_size
        report['javascript_bytes'] = (wasm / 'rustsimhub_recovery_session.js').stat().st_size
        assert max(report['wasm_bytes'], report['javascript_bytes']) <= 512 * 1024
        assert report['wasm_bytes'] + report['javascript_bytes'] <= 2 * 1024 * 1024
        (output / 'browser.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'passed': report['passed'], 'wasm_bytes': report['wasm_bytes'], 'browser': report['browser']}))
    finally:
        server.shutdown()
        server.server_close()
        peer.stdin.close()
        try:
            peer.wait(timeout=3)
        except subprocess.TimeoutExpired:
            peer.kill()
            peer.wait(timeout=3)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--wasm', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    destination = Path(args.output).absolute()
    assert destination.is_relative_to(REPO / 'work') and not any(p.is_symlink() for p in [destination, *destination.parents])
    destination.mkdir(mode=0o700)
    run(Path(args.binary).absolute(), Path(args.wasm).absolute(), destination)
