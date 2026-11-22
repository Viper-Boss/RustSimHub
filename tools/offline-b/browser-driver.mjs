// SPDX-License-Identifier: AGPL-3.0-only
// Independent B subset, not a claim to have run A's full Python browser matrix.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {writeFile} from 'node:fs/promises';
import {join} from 'node:path';
const require=createRequire(import.meta.url);
const {chromium}=require('playwright'); // already installed; no install/download path
const version=require('playwright/package.json').version;
const origin=process.argv[2],proxy=process.env.OFFLINE_B_DENY_PROXY;
assert.match(origin,/^http:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}$/);
assert.match(proxy,/^http:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}$/);
const output=process.env.OFFLINE_B_BROWSER_ARTIFACTS;assert(output);
async function fixture(mode,reset=false){
  const response=await fetch(origin+'/fixture',mode?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({mode,reset})}:{});
  assert.equal(response.status,200);return response.json();
}
const count=async op=>(await fixture()).calls.filter(c=>c.op===op).length;
async function until(predicate){
  const deadline=performance.now()+4000;
  while(performance.now()<deadline){if(await predicate(await fixture()))return;await new Promise(r=>setTimeout(r,20));}
  throw Error('Finite backend did not observe required request');
}
async function show(page){await page.evaluate(()=>{const root=document.getElementById('c-legacy');root.hidden=false;root.classList.add('debug');for(const n of root.children)n.classList.remove('page-off');document.getElementById('support-panel').open=true;});}
async function login(page,pin){
  await show(page);
  await page.locator('#pin').evaluate(e=>{for(let p=e.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;});
  await page.fill('#pin',pin);
  await page.evaluate(()=>{document.querySelectorAll('details[data-configuration]').forEach(e=>e.open=false);document.getElementById('account-panel').open=true;});
  await page.click('#daily-login');
  await page.waitForFunction(()=>document.querySelector('#notice').textContent.includes('已登录')&&document.getElementById('c-legacy').hidden);
  await show(page);
}
async function idb(page,value,write=false,key='nkn-control-pending'){
  return page.evaluate(async({value,write,key})=>{
    const opening=indexedDB.open('rustsimhub-client-v1',1);
    const db=await new Promise((r,j)=>{opening.onsuccess=()=>r(opening.result);opening.onerror=()=>j(opening.error);});
    try{
      if(write){const tx=db.transaction('state','readwrite');tx.objectStore('state').put(value,key);await new Promise((r,j)=>{tx.oncomplete=r;tx.onabort=tx.onerror=()=>j(tx.error);});return;}
      const req=db.transaction('state').objectStore('state').get(key);return await new Promise((r,j)=>{req.onsuccess=()=>r(req.result);req.onerror=()=>j(req.error);});
    }finally{db.close();}
  },{value,write,key});
}
const browser=await chromium.launch({channel:'msedge',headless:true,chromiumSandbox:true,
  proxy:{server:proxy,bypass:'127.0.0.1'},
  args:['--disable-background-networking','--disable-component-update','--no-first-run',
    '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1']});
const evidence={format:1,kind:'offline-b-browser-subset',playwright:version,browser:browser.version(),
  widths:[],missing_module_negative:false,device_backend:'unchanged finite loopback fixture',
  additional_cases_per_width:['participation-grant-revoke-reload','corrupt-pending-no-request','offline-read-clears-old-status'],
  original_python_matrix_run:false,hardware_tested:false,public_network_tested:false,
  os_network_namespace_proven:false,installable:false};
try{
  for(const width of [320,390,1280]){
    await fixture('ready',true);
    const context=await browser.newContext({viewport:{width,height:900}});
    try{
      const page=await context.newPage(),errors=[];page.setDefaultTimeout(5000);
      page.on('pageerror',error=>errors.push(error.message));
      await context.route('**/*',route=>route.request().url().startsWith(origin+'/')?route.continue():route.abort());
      await page.addInitScript(()=>{window.offlineBCsp=[];addEventListener('securitypolicyviolation',e=>offlineBCsp.push(e.effectiveDirective));});
      const response=await page.goto(origin);
      assert.equal(response.headers()['content-security-policy'],(await fixture()).csp);
      await page.waitForFunction(()=>document.querySelector('#test-state').textContent.includes('可以检查状态'));
      await show(page);
      const panel=page.locator('#support-nkn'),usage=page.locator('#support-usage');
      assert.equal(await panel.getByRole('button').count(),10);
      assert(await usage.getByRole('button',{name:'只读刷新统计'}).isDisabled());
      assert.equal(await count('nkn_status'),0);assert.equal(await count('set_nkn_enabled'),0);
      assert((await fixture()).calls.every(c=>c.op==='identity'));
      const pin=Buffer.from((await fixture()).device).toString('hex');
      await login(page,pin);
      assert.equal(await count('nkn_status'),0);assert.equal(await count('usage_stats_query'),0);
      await fixture('network-denied');await page.locator('#network-refresh').evaluate(e=>e.click());
      await page.waitForFunction(()=>document.querySelector('#network-policy-save').disabled&&document.querySelectorAll('#network-interface option').length===0);
      assert(await page.locator('#network-test-connectivity').isDisabled());
      assert(await page.locator('#network-test-download').isDisabled());assert.equal(await count('network_test'),0);
      await fixture('ready');await show(page);
      await usage.getByRole('button',{name:'只读刷新统计'}).click();
      await page.waitForFunction(()=>document.querySelector('#support-usage').textContent.includes('本次只读查询已结束'));
      assert((await usage.innerText()).includes('42'));
      const permission=page.locator('#support-usage-participation');
      assert.equal(await count('usage_participation_status'),0);
      assert.equal(await count('usage_participation_set'),0);
      for(const enabled of [true,false]){
        await show(page);await permission.getByRole('checkbox').setChecked(enabled);
        page.once('dialog',dialog=>dialog.accept()); // synthetic consent, never a production permission
        await permission.getByRole('button',{name:'保存参与授权（暂不上报）',exact:true}).click();
        await page.waitForFunction(enabled=>document.querySelector('#support-usage-participation')
          .textContent.includes(enabled?'参与授权已保存':'不参与的授权已保存'),enabled);
        assert.equal(await count('usage_participation_set'),enabled?1:2);
        assert.equal(await count('set_nkn_enabled'),0);
      }
      const receipts=await idb(page,undefined,false,'usage-participation-receipts');
      assert.equal(receipts.length,2);
      assert.deepEqual(receipts.map(r=>[r.state,r.record.enabled,r.generation]),[['saved',true,'1'],['saved',false,'2']]);
      assert.notEqual(receipts[0].record.id,receipts[1].record.id);
      assert.equal(await idb(page,undefined,false,'usage-participation-pending'),null);
      await page.reload();
      await page.waitForFunction(()=>document.querySelector('#test-state').textContent.includes('可以检查状态'));
      await login(page,pin);
      assert.equal(await count('usage_participation_set'),2,'Reload/login must not replay a consent write');
      await permission.getByRole('button',{name:'登录并读取上报授权',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#support-usage-participation').textContent.includes('当前不参与安装上报'));
      await show(page);
      assert(!(await permission.getByRole('checkbox').isChecked()));
      assert.deepEqual(await idb(page,undefined,false,'usage-participation-receipts'),receipts);
      assert.equal(await count('usage_participation_set'),2);
      assert.equal(await count('nkn_status'),0);assert.equal(await count('set_nkn_enabled'),0);
      await panel.getByRole('button',{name:'管理登录后读取',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#support-nkn').textContent.includes('已只读核对'));
      const record={format:1,id:'11111111-1111-4111-8111-111111111111',owner:'remote',enabled:true,expected_revision:'0',core_instance:'a'.repeat(64)};
      await idb(page,record,true);await page.reload();
      await page.waitForFunction(()=>document.querySelector('#test-state').textContent.includes('可以检查状态'));
      await login(page,pin);
      await panel.getByRole('button',{name:'管理登录后读取',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#support-nkn').textContent.includes('上次请求待核对'));
      await show(page);
      await panel.getByRole('button',{name:'核对上次回执',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#support-nkn').textContent.includes('不证明未执行'));
      assert.deepEqual(await idb(page),record);assert.equal(await count('set_nkn_enabled'),0);
      const bad={...record,expected_revision:'00',secret:'SYNTHETIC_OFFLINE_B_MUST_NOT_LEAK'};
      await idb(page,bad,true);const beforeBad=await count('nkn_status');
      await panel.getByRole('button',{name:'只读核对引擎状态',exact:true}).click();
      await page.waitForFunction(()=>document.querySelector('#support-nkn').textContent.includes('操作或读取未获确认'));
      assert.equal(await count('nkn_status'),beforeBad);assert.deepEqual(await idb(page),bad);
      assert(!(await panel.innerText()).includes(bad.secret));
      assert(!(await panel.innerText()).includes('服务：运行中'));
      assert(await panel.getByRole('button',{name:'请求开启远程功能',exact:true}).isDisabled());
      await idb(page,null,true);await fixture('offline');const beforeOffline=await count('nkn_status');
      await panel.getByRole('button',{name:'只读核对引擎状态',exact:true}).click();
      await until(s=>s.calls.filter(c=>c.op==='nkn_status').length===beforeOffline+1);
      await page.waitForFunction(()=>document.querySelector('#support-nkn').textContent.includes('操作或读取未获确认')
        &&!Array.from(document.querySelectorAll('#support-nkn button')).find(b=>b.textContent==='只读核对引擎状态').disabled);
      assert(!(await panel.innerText()).includes('服务：运行中'));assert.equal(await count('set_nkn_enabled'),0);
      await fixture('held');
      await panel.getByRole('button',{name:'只读核对引擎状态',exact:true}).click();await until(s=>s.held);
      await page.click('#daily-logout');await show(page);await fixture('ready');await page.waitForTimeout(100);
      assert(!(await panel.innerText()).includes('服务：运行中'));
      assert(await panel.getByRole('button',{name:'只读核对引擎状态',exact:true}).isDisabled());
      assert(await usage.getByRole('button',{name:'只读刷新统计'}).isDisabled());
      assert(!(await usage.innerText()).includes('42'));
      assert(!(await permission.getByRole('checkbox').isChecked()));
      assert(!(await permission.innerText()).includes('未开始上报'));
      assert.equal(await count('set_nkn_enabled'),0);assert.deepEqual((await fixture()).failures,[]);
      assert.deepEqual(errors,[]);assert.deepEqual(await page.evaluate(()=>offlineBCsp),[]);
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await page.screenshot({path:join(output,`homepage-${width}.png`)});
      evidence.widths.push(width);console.log(`PASS: B actual Edge ${width}px; ESM/CSP/IDB, synthetic signed login, permission refusal, participation grant/revoke/reload, corrupt pending, offline read, original ID and logout/late reply`);
    }finally{await context.close();}
  }
  await fixture('ready',true);
  const context=await browser.newContext();
  try{
    const page=await context.newPage();
    await context.route('**/*',route=>route.request().url().startsWith(origin+'/')?route.continue():route.abort());
    await page.route('**/nkn-product.js',route=>route.fulfill({status:404,contentType:'text/javascript',body:''}));
    await page.goto(origin);await page.waitForTimeout(250);
    assert.notEqual(await page.locator('#support-nkn button').count(),10);
    assert.deepEqual((await fixture()).calls,[]);
    evidence.missing_module_negative=true;
  }finally{await context.close();}
  await writeFile(join(output,'browser-evidence.json'),JSON.stringify(evidence,null,2)+'\n',{flag:'wx'});
  console.log('PASS: B missing-real-module negative fails the positive mounting condition before API calls');
}finally{await browser.close();}
