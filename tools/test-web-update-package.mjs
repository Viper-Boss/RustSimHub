// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {UpdatePackage} from '../crates/edge/src/web/update-package.js';

const html = await readFile(new URL('../crates/edge/src/web/index.html', import.meta.url), 'utf8');
const elements = new Map(Array.from(html.matchAll(/id="(update-package[^"]*)"/g), ([, id]) => [id, {}]));
const element = suffix => elements.get('update-package' + suffix);
const file = {name:'release.rshapp', size:512};
const sent = [];
let token = null;
let consent = false;
let accepted = null;
const manifest = 'b'.repeat(64);
const queried = [];
let reply = {ok:true, json:async()=>({verified:true,epoch:114,schema_min:6,schema_max:9,manifest_sha256:manifest})};
const panel = new UpdatePackage({
  document: {getElementById:id=>elements.get(id)},
  getToken: () => token,
  confirm: () => consent,
  request: async command => {
    queried.push(command);
    if (command.op === 'discard_update_upload') {
      assert.equal(command.request_id, accepted?.request_id);
      accepted = null;
      return {discarded:true,request_id:command.request_id};
    }
    return {available:true,receipt:accepted};
  },
  fetch: (url, options) => {
    sent.push({url, options});
    return typeof reply === 'function' ? reply(options) : reply;
  }
});

assert.equal(element('').disabled, true);
assert.equal(element('-preview').disabled, true);
assert.equal(sent.length, 0);

token = 'a'.repeat(64);
panel.reset();
assert.equal(element('').disabled, false);
for (const badFile of [{name:'release.zip',size:512},{name:'empty.rshapp',size:0},{name:'large.rshapp',size:26*1024*1024}]) {
  element('').files = [badFile];
  element('').onchange();
  assert.equal(element('-preview').disabled, true);
  await element('-preview').onclick();
}
assert.equal(sent.length, 0);

element('').files = [file];
element('').onchange();
assert.equal(sent.length, 0);
await element('-preview').onclick();
assert.equal(sent.length, 1);
assert.equal(sent[0].url, '/api/update/v1/preview');
assert.equal(sent[0].options.body, file);
assert.equal(sent[0].options.headers['Authorization'], 'Bearer ' + token);
assert.equal(sent[0].options.headers['Content-Type'], 'application/octet-stream');
assert.equal(element('-result').hidden, false);
assert.equal(element('-epoch').textContent, '114');
assert.equal(element('-schema').textContent, '6–9');

reply = {ok:false,status:403};
await element('-preview').onclick();
assert.equal(element('-result').hidden, true);
assert.match(element('-state').textContent, /所有者身份/);
assert.equal(sent.length, 2);

let finish;
reply = new Promise(resolve => { finish = resolve; });
const pending = element('-preview').onclick();
await element('-preview').onclick();
assert.equal(sent.length, 3);
token = null;
panel.reset();
assert.equal(sent[2].options.signal.aborted, true);
finish({ok:true,json:async()=>({verified:true,epoch:115,schema_min:6,schema_max:6})});
await pending;
assert.equal(element('-result').hidden, true);
assert.equal(element('-preview').disabled, true);

console.log('PASS: signed update selection, explicit bounded upload, verified result, denied and logged-out replies.');


token = 'a'.repeat(64);
panel.reset();
element('').files = [file];
element('').onchange();
reply = {ok:true,json:async()=>({verified:true,epoch:114,schema_min:6,schema_max:9,manifest_sha256:manifest})};
await element('-preview').onclick();
assert.equal(element('-submit').disabled,false);
const previous = sent.length;
await element('-submit').onclick();
assert.equal(sent.length,previous,'declining confirmation sends nothing');

consent = true;
reply = ({headers}) => {
  accepted = {state:'received',request_id:headers['X-RustSimHub-Update-Request'],manifest_sha256:manifest,epoch:114};
  throw Error('synthetic lost reply');
};
await element('-submit').onclick();
assert.equal(sent.length,previous+1);
assert.equal(sent.at(-1).url,'/api/update/v1/submit');
assert.equal(sent.at(-1).options.body,file);
assert.equal(sent.at(-1).options.headers['X-RustSimHub-Update-Manifest'],manifest);
assert.match(element('-state').textContent,/未确认/);
await element('-submit').onclick();
assert.equal(sent.length,previous+1,'an uncertain result cannot auto-retry');
panel.reset();
await element('-query').onclick();
assert.deepEqual(queried.at(-1),{op:'update_upload_status'});
assert.match(element('-state').textContent,/已受理应用 114/);
assert.equal(element('-discard').disabled,false);
accepted.phase = 'running';
await element('-query').onclick();
assert.match(element('-state').textContent,/正在安装应用 114/);
assert.equal(element('-discard').disabled,true);
await element('-discard').onclick();
assert.equal(accepted.phase, 'running');
accepted.phase = 'confirmed';
await element('-query').onclick();
assert.match(element('-state').textContent,/应用 114 已安装并通过健康检查/);
accepted.phase = 'restored';
await element('-query').onclick();
assert.match(element('-state').textContent,/设备已恢复原版本/);
await element('-discard').onclick();
assert.equal(accepted,null);
assert.equal(element('-discard').hidden,true);
assert.match(element('-state').textContent,/已移除/);
console.log('PASS: explicit install confirmation, lost reply, durable receipt lookup, and removal of the exact upload.');

// Chromium's Window methods reject a component object as their receiver.
const originalFetch = globalThis.fetch;
const originalConfirm = globalThis.confirm;
let nativeRequests = 0;
let nativePrompts = 0;
try {
  globalThis.confirm = function (message) {
    assert.equal(this, globalThis);
    assert.match(message, /116/);
    nativePrompts++;
    return true;
  };
  globalThis.fetch = function (url, {headers}) {
    assert.equal(this, globalThis);
    nativeRequests++;
    if (url.endsWith('/preview')) {
      return {ok:true, json:async()=>({verified:true, epoch:116, schema_min:6,
        schema_max:9, manifest_sha256:manifest})};
    }
    return {status:202, json:async()=>({state:'received', epoch:116, manifest_sha256:manifest,
      request_id:headers['X-RustSimHub-Update-Request']})};
  };
  const browserPanel = new UpdatePackage({
    document:{getElementById:id=>elements.get(id)}, getToken:()=>token, request:async()=>null
  });
  element('').files = [file];
  element('').onchange();
  await browserPanel.preview();
  assert.equal(element('-submit').disabled, false);
  await browserPanel.submit();
  assert.match(element('-state').textContent, /已受理应用 116/);
  assert.equal(nativeRequests, 2);
  assert.equal(nativePrompts, 1);
} finally {
  globalThis.fetch = originalFetch;
  if (originalConfirm === undefined) delete globalThis.confirm;
  else globalThis.confirm = originalConfirm;
}
console.log('PASS: default preview, confirmation, and submit use the Window API receiver.');

const backup = (epoch, char) => ({epoch, manifest_sha256:char.repeat(64)});
const firstBackup = backup(117, 'b');
const secondBackup = backup(115, 'c');
const currentBackupStatus = {format:1, current:backup(118, 'a'), backups:[firstBackup,secondBackup]};
let retentionReply = {available:true, receipt:null, can_submit:false, retention:currentBackupStatus};
const backupCommands = [];
element('-backups').replaceChildren = (...items) => { element('-backups').children = items; };
const retentionPanel = new UpdatePackage({
  document:{getElementById:id=>elements.get(id), createElement:()=>({append(child) {this.button = child;}})},
  getToken:()=>token,
  confirm:()=>consent,
  request:async command => {
    if (command.op === 'update_upload_status') return retentionReply;
    backupCommands.push(command);
    return {deletion_queued:true, epoch:command.backup.epoch};
  }
});
await retentionPanel.query();
assert.match(element('-retention-state').textContent, /当前应用 118/);
assert.deepEqual(element('-backups').children.map(row=>row.button.textContent),
  ['删除应用 117','删除应用 115']);
assert.ok(element('-backups').children.every(row=>row.button.disabled));
await retentionPanel.removeBackup(firstBackup);
assert.equal(backupCommands.length, 0);
retentionReply = {...retentionReply, can_submit:true};
await retentionPanel.query();
await retentionPanel.removeBackup(currentBackupStatus.current);
assert.equal(backupCommands.length, 0);
consent = false;
await element('-backups').children[1].button.onclick();
assert.equal(backupCommands.length, 0);
consent = true;
await element('-backups').children[1].button.onclick();
await retentionPanel.removeBackup(firstBackup);
assert.deepEqual(backupCommands, [{op:'delete_update_backup', backup:secondBackup}]);
assert.ok(element('-backups').children.every(row=>row.button.disabled));
retentionReply = {...retentionReply, retention:{...currentBackupStatus, backups:[firstBackup]}};
await retentionPanel.query();
assert.equal(element('-backups').children.length, 1);
assert.equal(element('-backups').children[0].button.disabled, false);
retentionPanel.reset();
console.log('PASS: two verified backup rows, fresh-owner confirmation, one exact request, receipt reconciliation.');
