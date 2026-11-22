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
let reply = {ok:true, json:async()=>({verified:true,epoch:114,schema_min:6,schema_max:9})};
const panel = new UpdatePackage({
  document: {getElementById:id=>elements.get(id)},
  getToken: () => token,
  fetch: (url, options) => {
    sent.push({url, options});
    return reply;
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

