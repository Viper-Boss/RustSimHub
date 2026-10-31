#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real native stdio -> frozen T5 fields -> actual web parser/text projection.

This verifies unavailable frontend interoperability, not a working NKN SDK,
trusted provider, production authentication or a binary provenance signature.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess

from test_native import ID, decode, frame

ROOT = Path(__file__).resolve().parents[2]
NODE_CHECK = r"""
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const {status,contract}=JSON.parse(readFileSync(0,'utf8'));
const source=readFileSync(process.argv[1]);
const {parseNknStatus,nknStatusText,NKN_FIELDS}=await import(
  'data:text/javascript;base64,'+source.toString('base64'));
assert.deepEqual(NKN_FIELDS,contract.fields);
assert.deepEqual(Object.keys(status).sort(),[...contract.fields].sort());
const parsed=parseNknStatus(status);
assert.equal(parsed.source,'unavailable');
assert.equal(parsed.last_error,'engine_unavailable');
for(const key of ['enabled','service','path','dht'])assert.equal(parsed[key],'unknown');
for(const key of ['age_ms','rx_bytes','tx_bytes','rtt_ms','peer_count'])assert.equal(parsed[key],null);
for(const age of [0,NaN]){
  const text=nknStatusText(parsed,age);
  assert.match(text,/状态源不可用/);
  assert.match(text,/开关：未知\n服务：未知\n链路：未知\nDHT：未知/);
  assert.match(text,/连接节点数：未知/);
  assert.doesNotMatch(text,/接收字节：0|发送字节：0|连接节点数：0/);
}
assert.throws(()=>parseNknStatus({...status,token:'SYNTHETIC_SECRET'}));
const missing={...status};delete missing.sequence;
assert.throws(()=>parseNknStatus(missing));
console.log(JSON.stringify({cases:['frozen_14_fields','real_native_unavailable_parser',
 'unavailable_never_zero_or_ready','extra_or_missing_field_rejected'],passed:4,
 nkn_sdk_linked:false,production_ready:false}));
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--expected-source', required=True)
    args = parser.parse_args()
    if not re.fullmatch('[0-9a-f]{40}', args.expected_source):
        parser.error('expected source must be a full Git SHA')
    binary = str(args.binary.resolve(strict=True))
    version = subprocess.run([binary, '--version'], capture_output=True, check=True, timeout=5)
    reported = json.loads(version.stdout)
    assert reported['source_commit'] == args.expected_source
    assert reported['nkn_sdk_linked'] is False
    process = subprocess.run([binary, '--stdio'], input=frame(
        {'protocol': 1, 'id': ID, 'op': 'status'}), capture_output=True, check=True, timeout=5)
    assert not process.stderr and len(process.stdout) <= 2052
    replies = decode(process.stdout)
    assert len(replies) == 1 and replies[0]['id'] == ID
    assert replies[0]['result']['nkn_sdk_linked'] is False
    frozen = json.loads((ROOT / 'docs/team/interfaces/t5-offline-v1.json').read_text())
    assert frozen['production_ready'] is False and frozen['crypto_wire_frozen'] is False
    projected = subprocess.run(
        ['node', '--input-type=module', '--eval', NODE_CHECK,
         str(ROOT / 'crates/edge/src/web/nkn-status.js')],
        input=json.dumps({'status': replies[0]['result']['status'],
                          'contract': frozen['nkn_status']}).encode(),
        capture_output=True, check=True, timeout=5)
    assert not projected.stderr
    print(projected.stdout.decode().strip())


if __name__ == '__main__':
    main()
