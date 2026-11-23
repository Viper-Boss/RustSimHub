// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {UpdatePackage} from '../crates/edge/src/web/update-package.js';

const html = await readFile(new URL('../crates/edge/src/web/index.html', import.meta.url), 'utf8');
const ids = Array.from(html.matchAll(/id="(update-package[^"]*)"/g), ([, id]) => id);
const initialFile = {name:'release.rshapp',size:512};
const manifest = 'b'.repeat(64);
const wait = () => new Promise(setImmediate);
const makeElement = () => ({append(child) { this.button = child; }, replaceChildren(...children) { this.children = children; }});
const version = (epoch, char) => ({epoch, manifest_sha256:char.repeat(64)});
const oldReceipt = {state:'received',request_id:'12345678-1234-4234-8234-123456789abc',phase:'restored',...version(122,'a')};
const preview = {ok:true,json:async()=>({verified:true,epoch:123,schema_min:6,schema_max:6,manifest_sha256:manifest})};

function fixture({native = false} = {}) {
  const elements = new Map(ids.map(id => [id, makeElement()]));
  const element = suffix => elements.get('update-package' + suffix);
  let time = 0;
  let timer;
  const state = {token:'a'.repeat(64), consent:true, receipt:{...oldReceipt}, requests:[], calls:[],
    preview, canDelete:false, reconnects:0};
  const panel = new UpdatePackage({
    document:{getElementById:id=>elements.get(id),createElement:makeElement},
    getToken:()=>state.token,
    request:async command=>{
      state.requests.push(command);
      if (state.denied) { state.denied = false; throw Object.assign(Error(),{coreCode:'denied'}); }
      if (command.op === 'delete_update_backup') return {deletion_queued:true,epoch:command.backup.epoch};
      if (command.op === 'discard_update_upload') {
        assert.equal(command.request_id,state.receipt.request_id);
        state.receipt = null;
        return {discarded:true,request_id:command.request_id};
      }
      return {available:true,can_submit:true,can_delete_backup:state.canDelete,receipt:state.receipt,retention:state.retention};
    },
    reconnect:async()=>{state.reconnects++;},
    confirm:native ? undefined : ()=>state.consent,
    fetch:native ? undefined : async (url, options)=>{
      state.calls.push({url,options});
      if (url.endsWith('/preview')) return state.preview;
      state.receipt = {state:'received',epoch:123,manifest_sha256:manifest,
        request_id:options.headers['X-RustSimHub-Update-Request']};
      if (state.lost) throw Error('lost response');
      return {status:202,json:async()=>({...state.receipt})};
    },
    now:()=>time,schedule:(fn,delay)=>{timer={fn,delay};return timer;},cancel:()=>{timer=null;}
  });
  return {panel,element,state, select:async file=>{element('').files=[file];await element('').onchange();},
    async tick(){assert.ok(timer,'status polling scheduled');const next=timer;time+=next.delay;next.fn();await wait();}};
}

{
  const {panel,element,state,select} = fixture();
  state.token = null;
  panel.reset();
  assert.equal(element('').disabled,true);
  assert.equal(element('-submit').disabled,true);
  state.token = 'a'.repeat(64);
  for (const file of [{name:'bad.zip',size:512},{name:'empty.rshapp',size:0},{name:'big.rshapp',size:26*1024*1024}]) {
    await select(file);
    assert.equal(element('-submit').disabled,true);
  }
  assert.equal(state.calls.length,0);
  state.preview={ok:false,status:422};
  await select(initialFile);
  assert.equal(element('-preview').hidden,false);
  assert.match(element('-state').textContent,/验证失败/);
  state.preview=preview;
  await element('-preview').onclick();
  assert.equal(element('-submit').textContent,'安装 123');
  assert.equal(element('-submit').disabled,false);
  assert.equal(element('-epoch').textContent,'123');
  assert.equal(element('-schema').textContent,'6–6');
  assert.equal(state.calls.at(-1).options.body,initialFile);
  assert.equal(state.calls.at(-1).options.headers.Authorization,'Bearer ' + state.token);
  let finish;
  state.preview=new Promise(resolve=>{finish=resolve;});
  const action=select(initialFile);
  const signal=state.calls.at(-1).options.signal;
  state.token=null;
  panel.reset();
  assert.equal(signal.aborted,true);
  finish(preview);
  await action;
  assert.equal(element('-submit').disabled,true);
}
console.log('PASS: file selection verifies automatically, invalid packages never enable installation, reset fences stale requests.');

{
  const {panel,element,state,select,tick} = fixture();
  await panel.query();
  assert.match(element('-receipt').textContent,/恢复原版本/);
  assert.equal(element('-submit').hidden,undefined);
  await select(initialFile);
  assert.equal(element('-submit').disabled,false);
  state.consent=false;
  await panel.submit();
  assert.equal(state.calls.length,1);
  state.consent=true;
  state.lost=true;
  await panel.submit();
  await wait();
  assert.equal(state.calls.length,2);
  assert.match(element('-receipt').textContent,/已受理应用 123/);
  assert.equal(state.calls[1].options.headers['X-RustSimHub-Update-Manifest'],manifest);
  assert.equal(element('-submit').disabled,true);
  await panel.submit();
  assert.equal(state.calls.length,2);
  state.receipt={...state.receipt,phase:'running'};
  state.denied=true;
  await tick();
  assert.equal(state.reconnects,1);
  assert.match(element('-receipt').textContent,/正在安装/);
  assert.equal(element('-discard').disabled,true);
  state.receipt={...state.receipt,phase:'confirmed'};
  await tick();
  assert.match(element('-receipt').textContent,/应用 123 已安装并通过健康检查/);
  assert.equal(element('-progress').hidden,true);
  assert.equal(state.requests.some(command=>command.op==='discard_update_upload'),false);
  assert.equal(state.calls.length,2);
}
console.log('PASS: a finished upload needs no removal, one confirmation submits, polling resumes after a lost reply and device restart.');

{
  const originalFetch=globalThis.fetch;
  const originalConfirm=globalThis.confirm;
  let requests=0;
  try {
    globalThis.confirm=function(message){assert.equal(this,globalThis);assert.match(message,/123/);return true;};
    globalThis.fetch=function(url,{headers}) {
      assert.equal(this,globalThis);
      requests++;
      return url.endsWith('/preview') ? preview : {status:202,json:async()=>({state:'received',
        request_id:headers['X-RustSimHub-Update-Request'],epoch:123,manifest_sha256:manifest})};
    };
    const {panel,select}=fixture({native:true});
    await select(initialFile);
    await panel.submit();
    assert.equal(requests,2);
    panel.reset();
  } finally {
    globalThis.fetch=originalFetch;
    if (originalConfirm===undefined) delete globalThis.confirm;
    else globalThis.confirm=originalConfirm;
  }
}
console.log('PASS: browser fetch and confirmation use the Window receiver.');

{
  const {panel,element,state,tick}=fixture();
  const backup=version(121,'c');
  state.retention={format:1,current:version(123,'d'),backups:[backup]};
  await panel.query();
  assert.equal(element('-current').textContent,'当前 123');
  assert.equal(element('-backups').children[0].button.disabled,true);
  state.canDelete=true;
  await panel.query();
  state.consent=false;
  await element('-backups').children[0].button.onclick();
  assert.equal(state.requests.some(command=>command.op==='delete_update_backup'),false);
  state.consent=true;
  await element('-backups').children[0].button.onclick();
  assert.deepEqual(state.requests.at(-1),{op:'delete_update_backup',backup});
  state.retention={...state.retention,backups:[]};
  await tick();
  assert.equal(element('-backups').children.length,0);
  await panel.discard();
  assert.equal(state.receipt,null);
}
console.log('PASS: backup deletion remains a separate confirmed action and only targets the selected backup.');
