# SPDX-License-Identifier: AGPL-3.0-only
"""Unchanged production homepage/ESM/default CSP, real IDB and signatures.
Only the external device API is a finite loopback fixture, never an NKN engine.
"""
import json
import os
import sys
import time
import urllib.request
from playwright.sync_api import sync_playwright

origin = sys.argv[1]
assert origin.startswith('http://127.0.0.1:')

def fixture(mode=None, reset=False):
    data = None if mode is None else json.dumps({'mode': mode, 'reset': reset}).encode()
    req = urllib.request.Request(origin + '/fixture', data=data, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.load(response)

def count(op):
    return sum(c['op'] == op for c in fixture()['calls'])

def wait_fixture(page, predicate):
    end = time.monotonic() + 3
    while time.monotonic() < end:
        if predicate(fixture()):
            return
        page.wait_for_timeout(20)
    raise AssertionError('Fixture did not observe expected request')

def store(page, value):
    return page.evaluate('''async value=>{
        const opening=indexedDB.open('rustsimhub-client-v1',1);
        const db=await new Promise((resolve,reject)=>{opening.onsuccess=()=>resolve(opening.result);opening.onerror=reject;});
        const tx=db.transaction('state','readwrite');tx.objectStore('state').put(value,'nkn-control-pending');
        await new Promise((resolve,reject)=>{tx.oncomplete=resolve;tx.onabort=tx.onerror=reject;});db.close();
    }''', value)

def retained(page):
    return page.evaluate('''async()=>{
        const opening=indexedDB.open('rustsimhub-client-v1',1);
        const db=await new Promise((resolve,reject)=>{opening.onsuccess=()=>resolve(opening.result);opening.onerror=reject;});
        const reading=db.transaction('state').objectStore('state').get('nkn-control-pending');
        const value=await new Promise((resolve,reject)=>{reading.onsuccess=()=>resolve(reading.result);reading.onerror=reject;});db.close();return value;
    }''')

def login(page, pin):
    page.locator('#pin').evaluate("e=>{for(let p=e.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;}")
    page.fill('#pin', pin)
    page.evaluate("document.querySelectorAll('details[data-configuration]').forEach(e=>e.open=false)")
    page.locator('#account-panel').evaluate('e=>e.open=true')
    page.click('#daily-login')
    page.wait_for_function("() => document.querySelector('#notice').textContent.includes('已登录')")

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.getenv('WEB3_WEB_CHROMIUM', '/usr/bin/chromium'), headless=True, args=['--no-sandbox', '--disable-dev-shm-usage'])
    try:
        for width in [320, 390, 1280]:
            fixture('ready', True)
            context = browser.new_context(viewport={'width': width, 'height': 900})
            page = context.new_page()
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.add_init_script("window.web3Csp=[];addEventListener('securitypolicyviolation',event=>web3Csp.push(event.effectiveDirective));")
            page.route('**/*', lambda route: route.continue_() if route.request.url.startswith(origin + '/') else route.abort())
            response = page.goto(origin)
            assert response.headers['content-security-policy'] == fixture()['csp']
            page.wait_for_function("() => document.querySelector('#test-state').textContent.includes('可以检查状态')")
            page.locator('#support-panel').evaluate('e=>e.open=true')
            panel = page.locator('#support-nkn')
            assert panel.get_by_role('button').count() == 8, 'Real NKN constructor was not mounted'
            assert panel.get_by_role('button', name='请求开启远程功能', exact=True).is_disabled()
            assert count('nkn_status') == count('set_nkn_enabled') == 0
            assert all(c['op'] == 'identity' for c in fixture()['calls'])
            assert '/nkn-product.js' in fixture()['assets'] and '/nkn-status.js' in fixture()['assets']
            pin = bytes(fixture()['device']).hex()
            login(page, pin)
            assert count('nkn_status') == count('set_nkn_enabled') == 0, 'Login must not turn on or read NKN automatically'
            panel.get_by_role('button', name='管理登录后读取', exact=True).click()
            page.wait_for_function("() => document.querySelector('#support-nkn').textContent.includes('已只读核对')")
            assert count('nkn_status') == 1 and count('set_nkn_enabled') == 0
            assert panel.get_by_role('button', name='请求开启远程功能', exact=True).is_disabled()
            assert '服务：运行中' in panel.inner_text() and '链路：未知' in panel.inner_text()
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), f'Overflow at {width}'
            # Real IDB reload restores the same Unknown record and never replays a mutation.
            record = {'format': 1, 'id': '11111111-1111-4111-8111-111111111111', 'owner': 'remote', 'enabled': True, 'expected_revision': '0', 'core_instance': 'a' * 64}
            store(page, record)
            page.reload()
            page.wait_for_function("() => document.querySelector('#test-state').textContent.includes('可以检查状态')")
            page.locator('#support-panel').evaluate('e=>e.open=true')
            login(page, pin)
            panel.get_by_role('button', name='管理登录后读取', exact=True).click()
            page.wait_for_function("() => document.querySelector('#support-nkn').textContent.includes('上次请求待核对')")
            panel.get_by_role('button', name='核对上次回执', exact=True).click()
            page.wait_for_function("() => document.querySelector('#support-nkn').textContent.includes('不证明未执行')")
            assert retained(page) == record and count('nkn_request') == 1 and count('set_nkn_enabled') == 0
            # Malformed progress fails before RPC and does not echo local private data.
            bad = {**record, 'expected_revision': '00', 'secret': 'MUST-NOT-LEAK'}
            store(page, bad)
            before = count('nkn_status')
            panel.get_by_role('button', name='只读核对引擎状态', exact=True).click()
            page.wait_for_function("() => document.querySelector('#support-nkn').textContent.includes('操作或读取未获确认')")
            assert count('nkn_status') == before and retained(page) == bad
            assert '服务：运行中' not in panel.inner_text() and 'MUST-NOT-LEAK' not in panel.inner_text()
            assert panel.get_by_role('button', name='请求开启远程功能', exact=True).is_disabled()
            store(page, None)
            fixture('offline')
            offline_count = count('nkn_status')
            panel.get_by_role('button', name='只读核对引擎状态', exact=True).click()
            page.wait_for_function("() => document.querySelector('#support-nkn').textContent.includes('操作或读取未获确认')")
            wait_fixture(page, lambda state: sum(c['op'] == 'nkn_status' for c in state['calls']) == offline_count + 1)
            page.wait_for_function("() => !Array.from(document.querySelectorAll('#support-nkn button')).find(b=>b.textContent==='只读核对引擎状态').disabled")
            assert '服务：运行中' not in panel.inner_text()
            fixture('held')
            panel.get_by_role('button', name='只读核对引擎状态', exact=True).click()
            wait_fixture(page, lambda state: state['held'])
            page.click('#daily-logout')
            fixture('ready')
            page.wait_for_timeout(100)
            assert '服务：运行中' not in panel.inner_text()
            # Logout invalidates the session before refreshing the actual controls.
            read_button = panel.get_by_role('button', name='只读核对引擎状态', exact=True)
            assert read_button.is_disabled()
            for button in panel.locator('button').all():
                if button.inner_text() != '管理登录后读取':
                    assert button.is_disabled()
            assert panel.get_by_role('button', name='请求开启远程功能', exact=True).is_disabled()
            assert count('set_nkn_enabled') == 0
            assert not fixture()['failures'], fixture()['failures']
            assert not errors, errors
            assert page.evaluate('web3Csp') == []
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            if os.getenv('WEB3_WEB_BROWSER_ARTIFACT_DIR'):
                page.screenshot(path=os.path.join(os.environ['WEB3_WEB_BROWSER_ARTIFACT_DIR'], f'homepage-{width}.png'), full_page=True)
            print(f'PASS: actual homepage {width}px, ESM/CSP/IDB, signed Owner login, passive/readonly no Remote ON, same-ID reload/query, corrupt pending, offline read and logout/late response.')
            context.close()
        # A missing served module makes the exact same positive mounting assertion fail.
        fixture('ready', True)
        context = browser.new_context()
        page = context.new_page()
        page.route('**/nkn-product.js', lambda route: route.fulfill(status=404, content_type='text/javascript', body=''))
        page.goto(origin)
        page.wait_for_timeout(200)
        failed = False
        try:
            assert page.locator('#support-nkn button').count() == 8, 'Real NKN constructor was not mounted'
        except AssertionError:
            failed = True
        assert failed and fixture()['calls'] == []
        context.close()
        print('PASS negative control: removing the actual NKN module fails the positive mounting check before any API request.')
    finally:
        browser.close()
