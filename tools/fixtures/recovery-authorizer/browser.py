# SPDX-License-Identifier: AGPL-3.0-only
"""Real Chromium/WebCrypto/IndexedDB checks; loopback allowlist, no external requests."""
import http.server
import json
from pathlib import Path
import threading
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[3]
MODULE = ROOT / "configurator/src/security/recovery-binding.js"


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/":
            body, kind = b"<!doctype html><title>Synthetic recovery binding</title>", "text/html"
        elif self.path == "/recovery-binding.js":
            body, kind = MODULE.read_bytes(), "text/javascript"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass


def main():
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = f"http://127.0.0.1:{server.server_port}"
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(executable_path="/usr/bin/chromium", headless=True, args=["--no-sandbox"])
            try:
                context = browser.new_context()
                foreign = []
                def route(request):
                    if request.request.url.startswith(origin + "/"):
                        request.continue_()
                    else:
                        foreign.append(request.request.url)
                        request.abort()
                context.route("**/*", route)
                page = context.new_page()
                errors = []
                page.on("pageerror", lambda e: errors.append(str(e)))
                page.goto(origin)
                first = page.evaluate("""async () => {
                  const b = await import('/recovery-binding.js'); window.b = b;
                  const pin='a'.repeat(64), realm='00000000-0000-4000-8000-000000000001';
                  window.pin=pin; window.realm=realm;
                  const deny=async f=>{try{await f();return false}catch{return true}};
                  const created=await Promise.all([b.getOrCreateBinding(pin,realm),b.getOrCreateBinding(pin,realm)]);
                  if(created[0].public_key!==created[1].public_key)throw Error('CAS key replacement');
                  if(!await deny(()=>b.loadSigningKey(pin,realm,1)))throw Error('unapproved key exposed');
                  const other=await b.getOrCreateBinding('b'.repeat(64),realm);
                  const otherRealm=await b.getOrCreateBinding(pin,'00000000-0000-4000-8000-000000000002');
                  if(new Set([created[0].public_key,other.public_key,otherRealm.public_key]).size!==3)throw Error('namespace overlap');
                  const response={device_pin:pin,realm,public_key:created[0].public_key,
                    grant:{id:'00000000-0000-4000-8000-000000000003',generation:'9007199254740993',active:true}};
                  if(!await deny(()=>b.rememberGrant(pin,realm,1,{...response,device_pin:'b'.repeat(64)})))throw Error('wrong pin');
                  if(!await deny(()=>b.rememberGrant(pin,realm,1,{...response,public_key:other.public_key})))throw Error('wrong key');
                  if(!await deny(()=>b.rememberGrant(pin,realm,1,{...response,grant:{...response.grant,generation:9007199254740993}})))throw Error('lossy generation');
                  const approved=await b.rememberGrant(pin,realm,1,response);
                  const key=await b.loadSigningKey(pin,realm,approved.revision);
                  if(key.extractable||key.type!=='private')throw Error('exportable private key');
                  if(!await deny(()=>crypto.subtle.exportKey('pkcs8',key)))throw Error('private key exported');
                  // Real sign/verify only tests key persistence; no product handshake/transcript is invented.
                  const publicKey=await crypto.subtle.importKey('raw',Uint8Array.from(created[0].public_key.match(/../g),v=>parseInt(v,16)),{name:'Ed25519'},false,['verify']);
                  const bytes=new TextEncoder().encode('synthetic key storage fixture');
                  if(!await crypto.subtle.verify('Ed25519',publicKey,await crypto.subtle.sign('Ed25519',key,bytes),bytes))throw Error('key mismatch');
                  if(!await deny(()=>b.rememberGrant(pin,realm,1,response)))throw Error('stale revision');
                  if(!await deny(()=>b.rememberGrant(pin,realm,2,response)))throw Error('old generation');
                  window.syntheticPublicKey=created[0].public_key;
                  return {public_key:created[0].public_key,revision:approved.revision};
                }""")
                page.reload()
                after = page.evaluate("""async () => {
                  const b=await import('/recovery-binding.js');
                  const pin='a'.repeat(64),realm='00000000-0000-4000-8000-000000000001';
                  const deny=async f=>{try{await f();return false}catch{return true}};
                  const saved=await b.readBinding(pin,realm);
                  const key=await b.loadSigningKey(pin,realm,saved.revision);
                  if(key.extractable)throw Error('key lost non-exportability after reload');
                  const off=await b.rememberGrant(pin,realm,saved.revision,{device_pin:pin,realm,public_key:saved.public_key,
                    grant:{...saved.grant,generation:'9007199254740994',active:false}});
                  if(!await deny(()=>b.loadSigningKey(pin,realm,off.revision)))throw Error('revoked key exposed');
                  const forgotten=await b.forgetBinding(pin,realm,off.revision);
                  if(forgotten.device_revoked!==false||await b.readBinding(pin,realm)!==null)throw Error('deletion semantics');
                  const replacement=await b.getOrCreateBinding(pin,realm);
                  if(replacement.public_key===saved.public_key||replacement.grant!==null)throw Error('old key resurrected');
                  return {public_key:saved.public_key,revision:saved.revision};
                }""")
                assert after == first
                # Corrupt/unavailable storage is preserved and rejected; no clear/recreate fallback.
                page.evaluate("""async () => {
                  const opened=indexedDB.open('rustsimhub-recovery-bindings-v1',1);
                  const db=await new Promise((resolve,reject)=>{opened.onsuccess=()=>resolve(opened.result);opened.onerror=reject});
                  await new Promise((resolve,reject)=>{const t=db.transaction('bindings','readwrite');
                    t.objectStore('bindings').put({format:99},'c'.repeat(64)+':00000000-0000-4000-8000-000000000001');t.oncomplete=resolve;t.onabort=reject});db.close();
                  const b=await import('/recovery-binding.js');let rejected=false;
                  try{await b.getOrCreateBinding('c'.repeat(64),'00000000-0000-4000-8000-000000000001')}catch(e){rejected=e.message==='corrupt_binding'}
                  if(!rejected)throw Error('corruption silently repaired');
                }""")
                page.close()
                # A fresh JS realm with IndexedDB denied really fails instead of using memory/localStorage.
                page = context.new_page()
                page.add_init_script("Object.defineProperty(window,'indexedDB',{value:undefined})")
                page.goto(origin)
                assert page.evaluate("""async()=>{const b=await import('/recovery-binding.js');try{
                  await b.getOrCreateBinding('a'.repeat(64),'00000000-0000-4000-8000-000000000001');return false
                }catch(e){return e.message==='unsupported'}}""")
                assert not errors, errors
                assert not foreign, foreign
                context.close()
                print(json.dumps({"passed": True, "backend": "real-Chromium-WebCrypto-IndexedDB",
                                  "cases": ["concurrent_create", "unapproved", "pin_and_realm_isolation", "wrong_key", "canonical_u64", "nonexportable", "sign_verify", "CAS", "reload", "revocation", "forget", "corrupt_storage", "unavailable_storage"],
                                  "core_authorization": "not_implemented", "external_requests": 0}))
            finally:
                browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main()
