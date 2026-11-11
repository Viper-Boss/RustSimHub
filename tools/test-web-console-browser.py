# SPDX-License-Identifier: AGPL-3.0-only
"""Fresh ephemeral Chromium profiles, loopback synthetic API, actual HTML and ESM."""
import json, os, sys, urllib.request
from pathlib import Path
from playwright.sync_api import sync_playwright
origin=sys.argv[1]
assert origin.startswith('http://127.0.0.1:')
def fixture(mode=None, **fields):
    data=None if mode is None else json.dumps(dict(mode=mode,**fields)).encode()
    with urllib.request.urlopen(urllib.request.Request(origin+'/fixture',data=data,headers={'Content-Type':'application/json'}),timeout=5) as r:return json.load(r)
def click(page,id):page.locator('#'+id).evaluate('(e)=>e.click()')
def wait_rows(page,n):page.wait_for_function('(n)=>document.querySelectorAll("#c-mail-list button").length===n',arg=n)
def login(page,pin):
    page.evaluate("document.getElementById('c-legacy').hidden=false;document.getElementById('c-legacy').classList.add('debug');for(const n of document.getElementById('c-legacy').children)n.classList.remove('page-off');document.querySelectorAll('details').forEach(e=>e.open=true)")
    page.fill('#pin',pin);page.evaluate("document.querySelectorAll('details[data-configuration]').forEach(e=>e.open=false)");click(page,'daily-login')
    page.wait_for_function("()=>document.querySelector('#notice').textContent.includes('已登录')&&document.getElementById('c-legacy').hidden")
with sync_playwright() as p:
    browser=p.chromium.launch(executable_path=os.getenv('WEB3_WEB_CHROMIUM','/usr/bin/chromium'),headless=True,args=['--no-sandbox','--disable-dev-shm-usage'])
    try:
        for width in [320,390,1280]:
            fixture('ready',reset=True,consoleData=dict(head=40,deleted=[],visibility='0',realm='fixture',line=7))
            context=browser.new_context(viewport=dict(width=width,height=900),timezone_id='Asia/Shanghai')
            page=context.new_page();page.set_default_timeout(8000);errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
            page.add_init_script("window.cspErrors=[];addEventListener('securitypolicyviolation',e=>cspErrors.push(e.effectiveDirective))")
            page.route('**/*',lambda r:r.continue_() if r.request.url.startswith(origin+'/') else r.abort())
            page.goto(origin);page.wait_for_function("()=>document.querySelector('#test-state').textContent.includes('可以检查状态')")
            login(page,bytes(fixture()['device']).hex());wait_rows(page,16)
            page.locator('[data-go=comms]').first.evaluate('(e)=>e.click()')
            assert '收到顺序' in page.locator('.console-order-note').first.text_content();assert page.locator('.console-order-note').evaluate_all('(rows)=>rows.some(e=>e.getBoundingClientRect().height>0)')
            assert '通知40' in page.locator('#c-mail-list button').first.text_content(),'old SMS timestamp must not move newest receipt'
            assert page.locator('#c-mh-q-sms').text_content()=='40'
            assert '已结束' in page.locator('#c-call-list').text_content()
            assert '方向未知' in page.locator('#c-call-list').text_content()
            page.locator('#c-mc-sms button' if width<768 else '#c-mail-list button').first.click()
            page.wait_for_function("()=>document.querySelector('#c-mail-body').textContent.includes('这是很长')")
            assert '2001-01-01 09:02:03' in page.locator('#c-mail-time').text_content()
            assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),f'overflow {width}'
            # Reader back is an actual DOM action; hidden desktop/mobile variations share handlers.
            page.locator('[data-go=comms]').first.evaluate('(e)=>e.click()')
            page.locator('#c-mail-list button').nth(1).evaluate('(e)=>e.click()')
            page.wait_for_function("()=>document.querySelector('#c-mail-time').textContent.includes('时间无效')")
            page.locator('[data-go=comms]').first.evaluate('(e)=>e.click()');click(page,'c-mail-more');wait_rows(page,32)
            fixture('ready',consoleData=dict(deleted=[40],visibility='1'))
            click(page,'c-mail-refresh');wait_rows(page,16)
            assert '通知39' in page.locator('#c-mail-list button').first.text_content()
            assert page.locator('#c-mh-q-sms').text_content()=='39'
            fixture('ready',consoleData=dict(head=65,deleted=[],visibility='2'))
            click(page,'c-mail-refresh');page.wait_for_function("()=>document.querySelector('#c-mail-list button').textContent.includes('通知65')")
            for n in [32,48,64,65]:click(page,'c-mail-more');wait_rows(page,n)
            assert page.locator('#c-mail-more').is_hidden()
            assert len(set(page.locator('#c-mail-list button .p').all_text_contents()))==65
            # Equal-count delete/restore while paging invalidates current view.
            click(page,'c-mail-refresh');wait_rows(page,16)
            fixture('ready',consoleData=dict(visibility='4'))
            click(page,'c-mail-more');wait_rows(page,0)
            assert '已变化' in page.locator('#c-mail-empty').text_content()
            click(page,'c-mail-refresh');wait_rows(page,16)
            fixture('held');click(page,'c-mail-more')
            page.wait_for_timeout(100)
            fixture('ready',consoleData=dict(line=8,realm='new',head=9))
            click(page,'c-mail-refresh');wait_rows(page,9)
            page.wait_for_timeout(100)
            assert page.locator('#c-mail-list button').count()==9
            fixture('ready',consoleData=dict(head=0));click(page,'c-mail-refresh');wait_rows(page,0)
            assert page.locator('#c-mail-empty').text_content()=='没有短信'
            fixture('ready',consoleData=dict(head=9));click(page,'c-mail-refresh');wait_rows(page,9)
            fixture('denied');click(page,'c-mail-refresh');wait_rows(page,0)
            assert '暂不可用' in page.locator('#c-mail-empty').text_content()
            fixture('ready');click(page,'daily-logout');wait_rows(page,0)
            old_key=page.locator('#client-key').text_content()
            fixture('deny-login')
            page.evaluate("document.getElementById('c-legacy').hidden=false;document.getElementById('c-legacy').classList.add('debug');for(const n of document.getElementById('c-legacy').children)n.classList.remove('page-off')")
            page.locator('#account-panel').evaluate('e=>e.open=true');page.locator('#owner-username').fill('fixture-admin');page.locator('#owner-password').fill('fixture-wrong-password')
            click(page,'password-login')
            page.wait_for_function("()=>document.querySelector('#notice').textContent.includes('请求被拒绝或服务暂不可用')")
            assert page.locator('#client-key').text_content()==old_key,'failed password replaced paired key'
            assert page.locator('#owner-password').input_value()==''
            fixture('ready');login(page,bytes(fixture()['device']).hex());wait_rows(page,9)
            assert not errors,errors
            assert page.evaluate('cspErrors')==[]
            assert not fixture()['failures'],fixture()['failures']
            writes=[c for c in fixture()['calls'] if c['op'].startswith(('delete_','submit_','prepare_','set_'))]
            assert writes==[],writes
            out=Path('.local/web5-console');out.mkdir(parents=True,exist_ok=True)
            page.locator('[data-go=comms]').first.evaluate('(e)=>e.click()')
            page.screenshot(path=str(out/f'console-{width}.png'),full_page=True)
            print(f'PASS actual HTML/ESM {width}px: signed Owner/reused IDB key, 65 receipt pages, timestamps, terminal/unknown calls, counts, delete/restore, late refresh/line/db generation, denied read, logout, CSP and no writes')
            context.close()
    finally:browser.close()
