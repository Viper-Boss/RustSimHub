#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Independent stdlib reference for the committed synthetic D23 vectors."""
import hashlib
import hmac
import json
from pathlib import Path
import struct
import uuid


def verify(path):
    vector = json.loads(path.read_text())
    assert vector['format'] == 2
    key_domain = b'RustSimHub.RecoveryExecution.Key.v2\0'
    request_domain = b'RustSimHub.RecoveryExecution.Request.v2\0'
    assert bytes.fromhex(vector['key_domain_hex']) == key_domain
    assert bytes.fromhex(vector['request_domain_hex']) == request_domain
    key = hmac.digest(bytes.fromhex(vector['synthetic_seed_hex']), key_domain, 'sha256')
    assert key.hex() == vector['derived_key_hex']
    grant = vector['binding']

    def part(value):
        if isinstance(value, str):
            value = value.encode('utf-8')
        return struct.pack('<I', len(value)) + value

    common = (request_domain + part(grant['realm']) + bytes(grant['device_public_key'])
              + part(grant['owner_id']) + struct.pack('<Q', grant['owner_generation'])
              + part(grant['grant_id']) + bytes(grant['independent_public_key'])
              + struct.pack('<Q', grant['grant_generation'])
              + uuid.UUID(vector['request_id']).bytes)
    for case in vector['cases']:
        target = case['target_trial']
        transcript = (common + part(case['action']) + struct.pack('<I', grant['max_trial_ms'])
                      + (b'\0' if target is None else b'\1' + part(target))
                      + part(bytes.fromhex(case['body_hex'])))
        assert transcript.hex() == case['transcript_hex']
        assert hmac.digest(key, transcript, hashlib.sha256).hex() == case['fingerprint_hex']
    print(json.dumps({'synthetic_only': True, 'key_matches': True,
                      'transcripts_and_fingerprints_match': len(vector['cases'])}))


if __name__ == '__main__':
    verify(Path(__file__).resolve().parents[2]
           / 'crates/auth/src/recovery_execution/vectors.json')
