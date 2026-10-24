#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Disposable real-UID Core IPC test. No modem/system bus/network/service operations.

Run as Root with a dev-fixtures build. Core, browser and worker run under
separate unprivileged numeric UIDs, with no accounts created. All credentials
and messages are synthetic and stay in a private temporary directory/stdin.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import uuid

EDGE, CORE, WORKER, OTHER = 65529, 65530, 65531, 65532


def client(path):
    data = sys.stdin.buffer.read(65537)
    if not data or len(data) > 65536:
        raise ValueError('bounded input required')
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(3)
        stream.connect(path)
        _, uid, _ = struct.unpack('3i', stream.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != CORE:
            raise ValueError('unexpected Core UID')
        stream.sendall(struct.pack('!I', len(data)) + data)

        def read(n):
            result = b''
            while len(result) < n:
                part = stream.recv(n - len(result))
                if not part:
                    raise ValueError('connection closed')
                result += part
            return result

        size, = struct.unpack('!I', read(4))
        if not 0 < size <= 65536:
            raise ValueError('bad frame size')
        response = json.loads(read(size))
        request = json.loads(data)
        if response['protocol'] != request['protocol'] or response['id'] != request['id']:
            raise ValueError('bad response binding')
        print(json.dumps(response['result']))


def test(binary):
    assert os.geteuid() == 0, 'Root is required only for disposable UID isolation'
    with tempfile.TemporaryDirectory(prefix='rustsimhub-forwarding-ipc-') as temporary:
        base = Path(temporary)
        assert base.resolve().parent == Path(tempfile.gettempdir()).resolve()
        base.chmod(0o711)
        child_script = base / 'client.py'
        shutil.copyfile(__file__, child_script)
        child_script.chmod(0o755)
        program = base / 'core'
        shutil.copyfile(binary, program)
        program.chmod(0o755)
        for name, mode in [('state', 0o700), ('identity', 0o700), ('runtime', 0o750)]:
            folder = base / name
            folder.mkdir(mode=mode)
            os.chown(folder, CORE, CORE)
        path = str(base / 'runtime/core.sock')
        process = None
        counter = 0

        def identity(uid):
            def drop():
                # OTHER also gets filesystem access: its rejection must come
                # from the actual SO_PEERCRED admission gate, not permissions.
                os.setgroups([CORE])
                os.setgid(CORE)
                os.setuid(uid)
            return drop

        def start():
            nonlocal process, counter
            counter += 1
            output = base / f'core-{counter}.out'
            errors = base / f'core-{counter}.err'
            with output.open('wb') as out, errors.open('wb') as err:
                output.chmod(0o600)
                errors.chmod(0o600)
                process = subprocess.Popen([str(program), '--forwarding-ipc-fixture', path,
                    str(EDGE), str(WORKER), str(base / 'state'), str(base / 'identity')],
                    stdout=out, stderr=err, preexec_fn=identity(CORE), cwd=base)
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError('fixture Core exited before readiness')
                text = output.read_text()
                if '\n' in text:
                    ready = json.loads(text.splitlines()[0])
                    assert ready['fixture_ready']
                    return ready
                time.sleep(0.02)
            raise AssertionError('fixture Core readiness deadline')

        def stop():
            nonlocal process
            if process is None:
                return
            if process.poll() is None:
                process.terminate()
            try:
                code = process.wait(timeout=5)
                assert code == 0, 'Core must exit cleanly'
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
                raise AssertionError('fixture Core did not stop within budget')
            finally:
                process = None
            assert not Path(path).exists(), 'owned socket must be cleaned'

        def rpc(uid, command, protocol=4, allowed=True):
            request = {'protocol': protocol, 'id': str(uuid.uuid4()), 'command': command}
            result = subprocess.run([sys.executable, str(child_script), '--client', path],
                input=json.dumps(request), text=True, capture_output=True, timeout=5,
                preexec_fn=identity(uid), cwd=base)
            value = json.loads(result.stdout) if result.returncode == 0 else {'error': 'transport_closed'}
            if allowed:
                assert result.returncode == 0 and 'error' not in value, 'expected authorized IPC result'
            else:
                assert 'error' in value, 'forbidden caller/protocol was accepted'
            return value

        try:
            ready = start()
            assert ready['new']
            state = rpc(0, {'op': 'state'})
            assert state['enabled'] and state['generation'] == 1
            assert state['process_state_verified'] is False
            for uid in (EDGE, WORKER, OTHER):
                rpc(uid, {'op': 'grant'}, allowed=False)
            rpc(0, {'op': 'identity'}, protocol=2, allowed=False)
            rpc(WORKER, {'op': 'identity'}, protocol=2, allowed=False)
            lease = rpc(0, {'op': 'grant'})['lease']
            rpc(0, {'op': 'acquire', 'lease': lease}, allowed=False)
            first = rpc(WORKER, {'op': 'acquire', 'lease': lease})['claim']
            assert first and first['request']['body']
            rpc(WORKER, {'op': 'acquire', 'lease': lease}, allowed=False)
            rpc(0, {'op': 'revoke'})
            rpc(WORKER, {'op': 'acquire', 'lease': lease}, allowed=False)
            new = rpc(0, {'op': 'grant'})['lease']
            acquired = rpc(WORKER, {'op': 'acquire', 'lease': new})
            second = acquired['claim']
            assert second['id'] != first['id']
            for stage in ('before_connect', 'before_tls', 'before_submit'):
                assert rpc(WORKER, {'op': 'check', 'lease': new,
                    'attempt_id': second['attempt_id'], 'claim_sha256': acquired['claim_sha256'],
                    'stage': stage})['allowed']
            # Synthetic transport acknowledgement: this tests IPC state
            # transitions only, never claims that an HTTPS server was contacted.
            receipt = rpc(WORKER, {'op': 'finish', 'lease': new, 'attempt_id': second['attempt_id'],
                'outcome': {'kind': 'confirmed', 'status': 200}})
            assert receipt['state'] == 'confirmed'
            assert rpc(WORKER, {'op': 'acquire', 'lease': new})['claim'] is None
            disabled = rpc(EDGE, {'op': 'set_forwarding_enabled', 'token': ready['owner_token'],
                'expected_generation': 1, 'request_id': str(uuid.uuid4()), 'enabled': False}, protocol=2)
            assert disabled['enabled'] is False and disabled['generation'] == 2
            assert disabled['unknown'] == 1
            rpc(WORKER, {'op': 'acquire', 'lease': new}, allowed=False)
            rpc(0, {'op': 'grant'}, allowed=False)
            stop()
            reopened = start()
            assert reopened['new'] is False
            state = rpc(0, {'op': 'state'})
            assert state['enabled'] is False and state['generation'] == 2
            assert state['unknown'] == 1 and state['lease_active'] is False
            rpc(WORKER, {'op': 'acquire', 'lease': new}, allowed=False)
            rpc(0, {'op': 'grant'}, allowed=False)
            stop()
        finally:
            stop()
    print(json.dumps({'passed': True, 'actual_uid_roles': 4, 'reopen_preserved_disabled': True,
        'unknown_retained': True, 'external_http_requests': 0, 'device_access': False,
        'process_stop_scope': 'disposable fixture Core and short-lived clients only; not installed forwarding service'}))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] == '--client':
        try:
            client(sys.argv[2])
        except (OSError, ValueError, KeyError):
            sys.exit(2)
    else:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument('--core', required=True, type=Path)
        test(parser.parse_args().core.resolve(strict=True))
