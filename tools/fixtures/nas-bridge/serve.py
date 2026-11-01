#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Loopback-only synthetic NAS panel, using the real module, markup and CSS."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import argparse, json, re

ROOT = Path(__file__).resolve().parents[3]

def markup():
    text = (ROOT / 'crates/edge/src/web/index.html').read_text(encoding='utf8')
    start = text.index('<details id="nas-bridge-panel">')
    depth = 0
    for match in re.finditer(r'<details\b[^>]*>|</details>', text[start:]):
        depth += -1 if match.group() == '</details>' else 1
        if depth == 0:
            return text[start:start+match.end()].replace('<details id="nas-bridge-panel">', '<details id="nas-bridge-panel" open>', 1)
    raise ValueError('unclosed production NAS panel')

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        assets = {'/nas-bridge.js': 'nas-bridge.js', '/clients.js': 'clients.js', '/style.css': 'style.css'}
        if self.path == '/':
            body = ('<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>RustSimHub · NAS本地合成演示</title><link rel="stylesheet" href="/style.css"><script type="module" src="/fixture.js"></script></head><body><main><header><span class="tag">本地合成演示 · 不连接设备</span><h1>NAS 只读接入</h1><p>所有授权、绑定和回执只在本页内存中模拟，不访问棒子、NAS或公网。此页不进入生产静态资源。</p></header>' + markup() + '<section><h2>合成场景</h2><label><input type="checkbox" id="fixture-lost">模拟保存回复丢失（只影响下一次保存）</label><p id="fixture-log" role="status"></p></section></main></body></html>').encode()
            content_type = 'text/html; charset=utf-8'
        elif self.path in assets:
            body = (ROOT / 'crates/edge/src/web' / assets[self.path]).read_bytes()
            content_type = 'text/css' if self.path.endswith('.css') else 'text/javascript'
        elif self.path == '/fixture.js':
            body = (Path(__file__).with_name('fixture.js')).read_bytes()
            content_type = 'text/javascript'
        else:
            self.send_error(404); return
        self.send_response(200)
        for key, value in {'Content-Type': content_type, 'Content-Length': str(len(body)),
            'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff',
            'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"}.items():
            self.send_header(key, value)
        self.end_headers(); self.wfile.write(body)

    def log_message(self, *_):
        pass

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--report', type=Path, required=True)
    args = p.parse_args()
    if args.report.exists():
        p.error('report already exists')
    with ThreadingHTTPServer(('127.0.0.1', 0), Handler) as server:
        report = {'url': f'http://127.0.0.1:{server.server_port}/', 'loopback_only': True, 'synthetic': True}
        args.report.write_text(json.dumps(report), encoding='utf8')
        print(json.dumps(report), flush=True)
        server.serve_forever()
