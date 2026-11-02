// SPDX-License-Identifier: AGPL-3.0-only
// Validate output from actual Rust dispatcher tests with the unchanged browser
// contract. Only synthetic output DTOs are printed by those tests, never bodies.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {operation, lookup, status, errors} from '../../configurator/src/contracts.js';

assert.equal(process.argv.length, 3, 'usage: node core-recovery-t5-contract-check.mjs <Rust --nocapture log>');
const prefix = 'T5_CORE_RESULT ';
const rows = fs.readFileSync(process.argv[2], 'utf8').split('\n')
  .filter(line => line.startsWith(prefix) || line.startsWith('T5_CORE_STATUS '))
  .map(line => JSON.parse(line.slice(line.indexOf(' ') + 1)));
const counts = {operation: 0, lookup: 0, error: 0, status: 0};
for (const value of rows) {
  if ('code' in value) {
    assert.deepEqual(Object.keys(value).sort(), ['code', 'contract']);
    assert.equal(value.contract, 1);
    assert(errors.includes(value.code));
    counts.error++;
  } else if ('wifi' in value) {
    status(value);
    counts.status++;
  } else if ('lookup' in value) {
    lookup(value, value.target_request_id);
    counts.lookup++;
  } else {
    operation(value, value.request_id, value.action);
    counts.operation++;
  }
  const op = value.operation ?? ('state' in value ? value : null);
  if (op?.state === 'saving') {
    assert.equal(op.settings_saved, null, 'in-flight persistence is unsettled');
    assert.equal(op.rollback_verified, null);
  }
  if (op?.state === 'unknown') {
    assert.equal(op.authoritative, false);
    for (const key of ['settings_saved', 'rollback_verified', 'expires_in_ms']) {
      assert.equal(op[key], null);
    }
  }
}
assert(rows.length >= 25, 'no complete Rust evidence set');
assert(['operation', 'lookup', 'error'].every(k => counts[k] > 0), 'missing operation, lookup or error evidence');
console.log(JSON.stringify({passed: rows.length, failed: 0, counts,
  validator: 'unchanged configurator/src/contracts.js'}, null, 2));
