# SPDX-License-Identifier: AGPL-3.0-only
"""Actual local Chromium WASM <-> native snow evaluation; no product traffic."""
import contextlib
import http.server
import json
import os
from pathlib import Path
import selectors
import subprocess
import threading
import time
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent


class Native:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, bufsize=0)

    def call(self, op, body=b'', **extra):
        request = {'op': op, **extra}
        if op not in {'new', 'finish', 'hash', 'close', 'vectors'}:
            request['body'] = body.hex()
        self.process.stdin.write(json.dumps(request).encode() + b'\n')
        self.process.stdin.flush()
        data = b''
        until = time.monotonic() + 5
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while not data.endswith(b'\n'):
                if not selector.select(max(0, until - time.monotonic())):
                    raise AssertionError('native reply timeout')
                chunk = os.read(self.process.stdout.fileno(), 8193)
                assert chunk, 'native fixture exited'
                data += chunk
                assert len(data) <= 8192, 'native reply limit'
        return json.loads(data)

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=2)
        self.process.stdout.close()
        self.process.stderr.close()


def check(binary, pkg):
    """All network requests below go to this process's ephemeral loopback server."""
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path in {'/strict/', '/allowed/'}:
                data = b'<!doctype html><meta charset="utf-8"><script type="module" src="/lab.js"></script>'
                mime = 'text/html; charset=utf-8'
            elif self.path == '/lab.js':
                data = (ROOT / 'lab.js').read_bytes()
                mime = 'text/javascript'
            elif self.path in {'/pkg/noise.js', '/pkg/noise_bg.wasm'}:
                data = (pkg / self.path.rsplit('/', 1)[1]).read_bytes()
                mime = 'application/wasm' if self.path.endswith('.wasm') else 'text/javascript'
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Type', mime)
            self.send_header('Cache-Control', 'no-store')
            self.send_header('X-Content-Type-Options', 'nosniff')
            policy = "default-src 'none'; script-src 'self'"
            if self.path != '/strict/':
                policy += " 'wasm-unsafe-eval'"
            self.send_header('Content-Security-Policy', policy + "; connect-src 'self'; base-uri 'none'; object-src 'none'")
            self.end_headers()
            self.wfile.write(data)

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f'http://127.0.0.1:{server.server_port}'
    results = []
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(executable_path='/usr/bin/chromium', headless=True, args=['--no-sandbox'])
            try:
                with contextlib.ExitStack() as stack:
                    context = browser.new_context()
                    stack.callback(context.close)
                    # Instrument the real browser CSPRNG, retaining the real function.
                    context.add_init_script("""window.cspViolations=[];
                        addEventListener('securitypolicyviolation',e=>cspViolations.push({blocked:e.blockedURI,directive:e.effectiveDirective}));
                        window.rngCalls=0;const rng=Crypto.prototype.getRandomValues;
                        Crypto.prototype.getRandomValues=function(...args){window.rngCalls++;return rng.apply(this,args);};""")
                    page = context.new_page()
                    page.goto(url + '/allowed/')
                    page.wait_for_function('window.noiseBoot !== undefined')
                    assert page.evaluate('noiseBoot.ok'), 'allowed CSP must execute real WASM'
                    assert page.evaluate('noiseLab.vectors()') == 5
                    results.append('browser_upstream_kat_5_messages')

                    def bcall(session, op, body=b''):
                        return page.evaluate('([id,op,body])=>noiseLab.call(id,op,body)', [session, op, body.hex()])

                    @contextlib.contextmanager
                    def pair(initiator=True):
                        native = Native(binary)
                        bid = None
                        try:
                            assert native.call('new', initiator=initiator)['ok']
                            bid = page.evaluate('value=>noiseLab.create(value)', not initiator)
                            for index in range(3):
                                body = f'public synthetic handshake {index}'.encode()
                                native_sends = (index % 2 == 0) == initiator
                                if native_sends:
                                    wire = bytes.fromhex(native.call('write_handshake', body)['body'])
                                    reply = bcall(bid, 'read_handshake', wire)
                                else:
                                    wire = bytes.fromhex(bcall(bid, 'write_handshake', body)['body'])
                                    reply = native.call('read_handshake', wire)
                                assert reply['ok'] and bytes.fromhex(reply['body']) == body
                            assert native.call('finish')['ok'] and bcall(bid, 'finish')['ok']
                            nh = native.call('hash')['body']
                            bh = bcall(bid, 'hash')['body']
                            assert len(nh) == 64 and nh == bh
                            yield native, bid, nh
                        finally:
                            if bid is not None:
                                page.evaluate('id=>noiseLab.dispose(id)', bid)
                            native.close()

                    for initiator in [True, False]:
                        with pair(initiator) as (native, bid, _):
                            for body in [b'', 'native/浏览器'.encode(), bytes(range(256)) * 4]:
                                wire = bytes.fromhex(native.call('write_transport', body)['body'])
                                assert len(wire) == len(body) + 16
                                assert bytes.fromhex(bcall(bid, 'read_transport', wire)['body']) == body
                                wire = bytes.fromhex(bcall(bid, 'write_transport', body)['body'])
                                assert bytes.fromhex(native.call('read_transport', wire)['body']) == body
                        results.append('native_browser_interop_' + ('native_initiator' if initiator else 'browser_initiator'))

                    with pair() as (_, _, first):
                        with pair() as (_, _, second):
                            assert first != second
                    results.append('fresh_random_handshake_transcripts')

                    for case in ['tamper', 'truncate', 'extra', 'replay', 'reorder', 'direction']:
                        with pair() as (native, bid, _):
                            wire = bytes.fromhex(native.call('write_transport', b'synthetic encrypted body')['body'])
                            if case == 'tamper':
                                wire = bytes([wire[0] ^ 1]) + wire[1:]
                            elif case == 'truncate':
                                wire = wire[:-1]
                            elif case == 'extra':
                                wire += b'\0'
                            elif case == 'replay':
                                assert bcall(bid, 'read_transport', wire)['ok']
                            elif case == 'reorder':
                                wire = bytes.fromhex(native.call('write_transport', b'second')['body'])
                            else:
                                wire = bytes.fromhex(bcall(bid, 'write_transport', b'wrong direction')['body'])
                            failed = bcall(bid, 'read_transport', wire)
                            assert not failed['ok'] and failed['closed']
                            assert not bcall(bid, 'write_transport', b'late')['ok']
                        results.append('browser_rejects_' + case)

                    # Reverse direction of the adversarial test also reaches native snow.
                    with pair() as (native, bid, _):
                        wire = bytes.fromhex(bcall(bid, 'write_transport', b'tamper native receive')['body'])
                        bad = bytes([wire[0] ^ 1]) + wire[1:]
                        assert not native.call('read_transport', bad)['ok']
                        assert not native.call('read_transport', wire)['ok']
                    results.append('native_rejects_browser_ciphertext_tamper')

                    with pair() as (native, _, _):
                        old = bytes.fromhex(native.call('write_transport', b'foreign session')['body'])
                        with pair() as (_, bid, _):
                            assert not bcall(bid, 'read_transport', old)['ok']
                    results.append('foreign_session_rejected')

                    with pair() as (native, bid, _):
                        assert not bcall(bid, 'write_transport', bytes(1025))['ok']
                        assert not native.call('write_transport', bytes(1025))['ok']
                    results.append('body_ceiling_retires_both_ends')

                    early = page.evaluate('noiseLab.create(true)')
                    assert not bcall(early, 'write_transport', b'early')['ok']
                    page.evaluate('id=>noiseLab.dispose(id)', early)
                    results.append('no_transport_before_completed_handshake')
                    assert page.evaluate('rngCalls') > 0, 'must use actual Crypto.getRandomValues'
                    results.append('actual_browser_csprng_called')

                    strict = context.new_page()
                    strict.goto(url + '/strict/')
                    strict.wait_for_function('window.noiseBoot !== undefined')
                    failure = strict.evaluate('noiseBoot')
                    assert not failure['ok'], failure
                    assert any(word in failure['error'].lower() for word in ['wasm', 'webassembly']), failure
                    strict.wait_for_function('cspViolations.length > 0')
                    assert strict.evaluate("cspViolations.some(v=>v.directive.startsWith('script-src'))")
                    results.append('strict_csp_rejects_wasm_compilation')

                    denied = browser.new_context()
                    stack.callback(denied.close)
                    denied.add_init_script("Crypto.prototype.getRandomValues=function(){throw new Error('synthetic RNG denial');};")
                    no_rng = denied.new_page()
                    no_rng.goto(url + '/allowed/')
                    no_rng.wait_for_function('window.noiseBoot !== undefined')
                    assert no_rng.evaluate('noiseBoot.ok')
                    refused = no_rng.evaluate("""()=>{try{noiseLab.create(true);return false;}catch{return true;}}""")
                    assert refused, 'no fixed-key or Math.random fallback on RNG failure'
                    results.append('denied_browser_entropy_has_no_fallback')
                    version = browser.version
            finally:
                browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)
    return {'browser': version, 'checks': results, 'passed': len(results), 'production_ready': False,
            'crypto_wire_frozen': False, 'device_pin_or_grant_tested': False}
