# SPDX-License-Identifier: AGPL-3.0-only
"""Real Chromium against a loopback synthetic device; no RF/Root claims."""
import json
import os
import sys
import urllib.request
from playwright.sync_api import sync_playwright

origin = sys.argv[1]
assert origin.startswith('http://127.0.0.1:')
checks = 0

def fixture(mode=None, reset=False):
    data = None if mode is None else json.dumps({'mode': mode, 'reset': reset}).encode()
    with urllib.request.urlopen(urllib.request.Request(origin + '/fixture', data=data, headers={'Content-Type': 'application/json'})) as response:
        return json.load(response)

def count(op):
    return sum(c['op'] == op for c in fixture()['calls'])

def login(page, create=False):
    page.goto(origin + '/setup.html')
    page.locator('#setup-account').wait_for(state='visible')
    page.fill('#setup-username', 'BrowserOwner')
    page.fill('#setup-password', 'synthetic-owner-password')
    if create:
        page.fill('#setup-confirm', 'synthetic-owner-password')
    page.click('#setup-account-go')
    page.locator('#setup-wifi').wait_for(state='visible', timeout=60000)

def refresh(page):
    page.once('dialog', lambda d: d.accept())
    page.click('#setup-networks-rescan')

def ready(page):
    page.wait_for_function("document.querySelector('#setup-networks-note').textContent.includes('扫描完成')")

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.getenv('WEB3_AP_CHROMIUM', '/usr/bin/chromium'), headless=True, args=['--no-sandbox', '--disable-dev-shm-usage'])
    for width in [320, 390, 1280]:
        fixture('ready', True)
        context = browser.new_context(viewport={'width': width, 'height': 900})
        page = context.new_page()
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        login(page, True)
        assert count('setup_ap_scan_begin') == 0
        assert page.get_by_role('button', name='刷新附近 Wi-Fi', exact=True).count() == 1
        assert page.locator('#setup-networks-refresh').count() == 0
        assert page.locator('#setup-networks option').count() == 2
        refresh(page)
        ready(page)
        assert count('setup_ap_scan_begin') == 1
        if width == 390 and os.getenv('WEB3_AP_BROWSER_ARTIFACT_DIR'):
            page.screenshot(path=os.path.join(os.environ['WEB3_AP_BROWSER_ARTIFACT_DIR'], 'setup-mobile.png'), full_page=True)
        assert page.locator('#setup-networks option').nth(1).inner_text().startswith('Fresh <synthetic>')
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), f'overflow at {width}'
        fixture('empty')
        refresh(page)
        page.wait_for_function("document.querySelector('#setup-networks').options.length === 1")
        assert count('setup_ap_scan_begin') == 2
        # Trial -> reload -> same-ID query, reenter secret -> explicit save.
        page.fill('#ap-wifi-ssid', 'Home')
        page.fill('#ap-wifi-passphrase', 'wifi-password')
        page.click('#ap-wifi-begin')
        page.wait_for_function("document.querySelector('#ap-wifi-state').dataset.phase === 'trial_ready'")
        login(page)
        assert page.input_value('#ap-wifi-passphrase') == ''
        assert page.locator('#ap-wifi-reenter').is_visible()
        assert count('setup_ap_wifi_begin') == 1
        page.fill('#ap-wifi-ssid', 'Home')
        page.fill('#ap-wifi-passphrase', 'wifi-password')
        page.click('#ap-wifi-confirm')
        page.wait_for_function("document.querySelector('#ap-wifi-state').dataset.phase === 'saved'")
        assert count('setup_ap_wifi_confirm') == 1
        assert page.input_value('#ap-wifi-passphrase') == ''
        assert not page.evaluate("JSON.stringify(localStorage).includes('wifi-password') || JSON.stringify(localStorage).includes('synthetic-owner-password')")
        assert not errors, errors
        checks += 1
        context.close()
    for mode in ['unsupported', 'cooldown', 'refused']:
        fixture(mode, True)
        context = browser.new_context(viewport={'width': 390, 'height': 900})
        page = context.new_page()
        login(page, True)
        refresh(page)
        page.wait_for_function("!document.querySelector('#setup-networks-rescan').disabled" if mode != 'cooldown' else "document.querySelector('#setup-networks-note').textContent.includes('冷却')")
        assert count('setup_ap_scan_begin') == (1 if mode == 'refused' else 0)
        assert page.locator('#setup-networks option').count() == 2
        text = page.locator('#setup-networks-note').inner_text()
        assert ('手动' in text or '冷却' in text), text
        if mode == 'cooldown':
            assert page.locator('#setup-networks-rescan').is_disabled()
            page.wait_for_timeout(2200)
            assert page.locator('#setup-networks-rescan').is_enabled()
            assert count('setup_ap_scan_begin') == 0, 'timer must not submit a queued scan'
            assert '7/8' in page.locator('#setup-scan-facts').inner_text()
        checks += 1
        context.close()
    # Lost Begin and query outage survive a closed page; a second tab queries only.
    fixture('offline', True)
    context = browser.new_context(viewport={'width': 320, 'height': 900})
    page = context.new_page()
    login(page, True)
    refresh(page)
    page.get_by_role('button', name='查询扫描结果', exact=True).wait_for()
    page.wait_for_function("!document.querySelector('#setup-networks-rescan').disabled")
    assert count('setup_ap_scan_begin') == 1
    page.click('#setup-networks-rescan')
    page.wait_for_function("document.querySelector('#ap-wifi-state').textContent.includes('进度已保留')")
    assert count('setup_ap_scan_begin') == 1
    page.close()
    fixture('ready')
    page = context.new_page()
    login(page)
    ready(page)
    assert count('setup_ap_scan_begin') == 1
    assert count('wifi_networks') == 1, 'cache must not overwrite a recovered scan'
    assert page.locator('#setup-networks option').nth(1).inner_text().startswith('Fresh')
    checks += 1
    context.close()
    # Cancel a scan after a lost reply using the original operation ID.
    fixture('lost', True)
    context = browser.new_context()
    page = context.new_page()
    login(page, True)
    refresh(page)
    cancel = page.get_by_role('button', name='取消本次扫描', exact=True)
    cancel.wait_for()
    page.wait_for_function("!document.querySelector('#ap-wifi-cancel').disabled")
    cancel.click()
    page.wait_for_function("document.querySelector('#setup-networks-note').textContent.includes('已取消')")
    calls = fixture()['calls']
    begins = [c for c in calls if c['op'] == 'setup_ap_scan_begin']
    cancels = [c for c in calls if c['op'] == 'setup_ap_scan_cancel']
    assert len(begins) == len(cancels) == 1 and begins[0]['scan_id'] == cancels[0]['scan_id']
    checks += 1
    context.close()
    # Two real tabs: tab 2 finishes the original scan while tab 1's Begin reply is held.
    fixture('held', True)
    context = browser.new_context(viewport={'width': 390, 'height': 900})
    first = context.new_page()
    login(first, True)
    refresh(first)
    first.wait_for_function("document.querySelector('#setup-networks-rescan').disabled")
    second = context.new_page()
    login(second)
    ready(second)
    assert count('setup_ap_scan_begin') == 1
    fixture('ready')
    first.wait_for_function("!document.querySelector('#setup-networks-rescan').disabled")
    assert first.locator('#setup-networks option').nth(1).inner_text().startswith('Home'), 'stale tab published after another tab cleared the record'
    assert second.locator('#setup-networks option').nth(1).inner_text().startswith('Fresh')
    assert count('setup_ap_scan_begin') == 1
    checks += 1
    context.close()
    # No verified AP restoration: preserve receipt, query only, keep save blocked.
    fixture('restore-unknown', True)
    context = browser.new_context()
    page = context.new_page()
    login(page, True)
    refresh(page)
    page.wait_for_function("!document.querySelector('#setup-networks-rescan').disabled")
    page.click('#setup-networks-rescan')
    page.wait_for_function("document.querySelector('#setup-networks-note').textContent.includes('热点恢复尚未确认')")
    assert page.locator('#ap-wifi-begin').is_disabled()
    assert page.locator('#ap-wifi-confirm').is_disabled()
    login(page)
    assert count('setup_ap_scan_begin') == 1
    assert page.locator('#setup-networks-rescan').inner_text() == '查询扫描结果'
    assert count('setup_ap_wifi_confirm') == 0
    checks += 1
    context.close()
    # Wrong password remains a failed trial, never a saved configuration.
    fixture('ready', True)
    context = browser.new_context()
    page = context.new_page()
    login(page, True)
    page.fill('#ap-wifi-ssid', 'Home')
    page.fill('#ap-wifi-passphrase', 'wrong-password')
    page.click('#ap-wifi-begin')
    page.wait_for_function("document.querySelector('#ap-wifi-state').dataset.phase === 'failed'")
    assert '检查密码' in page.locator('#ap-wifi-state').inner_text()
    assert not page.locator('#ap-wifi-confirm').is_visible()
    assert count('setup_ap_wifi_confirm') == 0
    checks += 1
    context.close()
    browser.close()
print(f'WEB3-AP Chromium: {checks} scenarios passed (320/390/1280, refresh/empty, same-ID reopen/save, capability/cooldown/refusal, offline recovery, exact cancel, two tabs, unverified restoration, wrong password).')
