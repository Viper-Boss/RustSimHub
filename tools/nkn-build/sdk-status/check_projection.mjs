// SPDX-License-Identifier: AGPL-3.0-only
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {parseNknStatus,nknStatusText,NKN_FIELDS} from '../../../crates/edge/src/web/nkn-status.js';
const [active,closed]=JSON.parse(await readFile(process.argv[2],'utf8'));
for(const sample of [active,closed]){
 assert.deepEqual(Object.keys(sample).sort(),[...NKN_FIELDS].sort());
 assert.deepEqual(parseNknStatus(sample),sample);
 assert.equal(sample.source,'engine');
 for(const key of ['enabled','service','path','dht'])assert.equal(sample[key],'unknown');
 for(const key of ['rx_bytes','tx_bytes','rtt_ms','peer_count'])assert.equal(sample[key],null);
 const text=nknStatusText(sample,sample.age_ms);
 assert.ok(text.includes('引擎状态源'));
 assert.ok(text.includes('服务：未知'));
 assert.ok(!text.includes('127.0.0.1'));
}
assert.equal(active.last_error,null);
assert.equal(closed.last_error,'engine_unavailable');
console.log('Actual SDK open/closed samples accepted by production T5 parser; missing metrics/process evidence remain unknown.');
