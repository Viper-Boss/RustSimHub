#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the actual built native frontend with synthetic finite pipe inputs."""
import argparse
import fcntl
import json
from pathlib import Path
import signal
import struct
import subprocess
import time

ID = '11111111-1111-4111-8111-111111111111'


def frame(value):
    data = value if isinstance(value, bytes) else json.dumps(value).encode()
    return struct.pack('>I', len(data)) + data


def decode(data):
    replies = []
    while data:
        assert len(data) >= 4
        size = struct.unpack('>I', data[:4])[0]
        assert 0 < size <= 2048 and len(data) >= 4 + size
        replies.append(json.loads(data[4:4 + size]))
        data = data[4 + size:]
    return replies


def check(binary, output):
    request = {'protocol': 1, 'id': ID, 'op': 'status'}
    evidence = []

    def run(data, expected):
        p = subprocess.run([binary, '--stdio'], input=data, capture_output=True, timeout=5)
        assert p.returncode == expected, (p.returncode, expected)
        assert b'SYNTHETIC_SECRET' not in p.stdout + p.stderr
        return decode(p.stdout)

    assert run(b'', 0) == []
    evidence.append('clean_pipe_eof')
    response = run(frame(request), 0)[0]
    status = response['result']['status']
    assert response['id'] == ID and not response['result']['nkn_sdk_linked']
    assert status['source'] == 'unavailable' and status['enabled'] == 'unknown'
    assert all(status[k] is None for k in ['age_ms', 'rx_bytes', 'tx_bytes', 'rtt_ms', 'peer_count'])
    (output / 'native-status.json').write_text(json.dumps(status, indent=2) + '\n')
    evidence.append('real_unavailable_status_no_fabricated_counts')
    stop = dict(request, op='stop')
    assert len(run(frame(stop) + frame(request), 0)) == 1
    evidence.append('stop_ack_then_native_exit_no_further_frame')
    for data in [b'\x00', b'\x00\x00\x00\x00', struct.pack('>I', 1025), frame(b'{')[:-1]]:
        assert run(data, 2)[0]['error'] == 'invalid_frame'
    evidence.append('truncated_header_body_and_oversized_frame')
    for data in [dict(request, op='start'), dict(request, seed='SYNTHETIC_SECRET'),
                 dict(request, protocol=2), dict(request, id='SYNTHETIC_SECRET')]:
        reply = run(frame(data), 0)[0]
        assert reply['id'] is None and 'error' in reply
    evidence.append('unsupported_start_private_field_version_and_id_rejected')
    duplicate = b'{"protocol":1,"protocol":1,"id":"' + ID.encode() + b'","op":"status"}'
    assert run(frame(duplicate), 0)[0]['error'] == 'invalid_request'
    assert run(frame(request) * 33, 2)[-1]['error'] == 'frame_limit'
    evidence.append('duplicate_keys_and_32_frame_budget')
    p = subprocess.Popen([binary, '--stdio'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        p.stdin.write(b'\x00')
        p.stdin.flush()
        started = time.monotonic()
        assert p.wait(timeout=4) == 2 and time.monotonic() - started < 3.5
    finally:
        if p.poll() is None:
            p.kill()
        p.communicate(timeout=2)
    evidence.append('actual_partial_pipe_read_deadline')
    p = subprocess.Popen([binary, '--stdio'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        p.stdin.write(b'\x00')
        p.stdin.flush()
        p.send_signal(signal.SIGTERM)
        code = p.wait(timeout=3)
        assert code in (130, -signal.SIGTERM)  # signal may precede handler setup
    finally:
        if p.poll() is None:
            p.kill()
        p.communicate(timeout=2)
    evidence.append('actual_sigterm_exit_only_owned_process')
    p = subprocess.Popen([binary, '--stdio'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        fcntl.fcntl(p.stdout.fileno(), fcntl.F_SETPIPE_SZ, 4096)
        p.stdin.write(frame(request) * 32)
        p.stdin.flush()
        assert p.wait(timeout=4) == 2
    finally:
        if p.poll() is None:
            p.kill()
        p.communicate(timeout=2)
    evidence.append('actual_unread_small_output_pipe_write_deadline')
    result = {'schema': 1, 'native_only': True, 'cases': evidence, 'passed': len(evidence),
              'nkn_sdk_linked': False, 'arm_device_run': False, 'external_network_tested': False}
    (output / 'native-tests.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', required=True)
    p.add_argument('--output', required=True)
    a = p.parse_args()
    destination = Path(a.output)
    destination.mkdir(exist_ok=False)
    check(str(Path(a.binary).absolute()), destination)
