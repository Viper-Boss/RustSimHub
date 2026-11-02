#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run ONLY in a disposable --network=none container as Root, with owned keys."""
import base64
import ctypes
import hashlib
import http.server
import json
import os
from pathlib import Path
import select
import signal
import socket
import struct
import sys
import threading
import time


def exact(stream, size):
    result = bytearray()
    while len(result) < size:
        part = stream.recv(size - len(result))
        if not part:
            raise EOFError('owned fixture stream closed')
        result.extend(part)
    return bytes(result)


def ws_write(conn, data):
    data = json.dumps(data).encode()
    header = bytes([0x81, len(data)]) if len(data) < 126 else b'\x81\x7e' + struct.pack('>H', len(data))
    conn.sendall(header + data)


class Fixture:
    def __init__(self, scenario):
        self.scenario = scenario
        self.rpc_calls = self.ws_connections = 0
        self.sockets = []
        self.ready = threading.Event()
        fixture = self

        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'

            def log_message(self, *args):
                pass

            def do_POST(self):
                fixture.rpc_calls += 1
                size = int(self.headers.get('Content-Length', '0'))
                assert 0 < size <= 4096
                request = json.loads(self.rfile.read(size))
                assert request['method'] == 'getwsaddr'
                result = {'addr': f'127.0.0.1:{fixture.server.server_port}', 'pubkey': '11' * 32, 'id': '22' * 32}
                data = json.dumps({'result': result}).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                fixture.ws_connections += 1
                fixture.sockets.append(self.connection)
                key = self.headers['Sec-WebSocket-Key']
                accept = base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
                self.send_response(101)
                self.send_header('Upgrade', 'websocket')
                self.send_header('Connection', 'Upgrade')
                self.send_header('Sec-WebSocket-Accept', accept)
                self.end_headers()
                if fixture.scenario == 'malformed-before-ready':
                    ws_write(self.connection, {})
                    return
                if fixture.scenario == 'startup-eof':
                    fixture.ready.set()
                    time.sleep(0.3)
                    return
                ws_write(self.connection, {'Action': 'setClient', 'Error': 0, 'Result': {'sigChainBlockHash': '33' * 32}})
                fixture.ready.set()
                try:
                    while self.connection.recv(4096):
                        pass
                except OSError:
                    pass

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def start(self):
        self.thread.start()

    def close(self):
        for conn in self.sockets:
            try:
                conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            conn.close()
        self.server.shutdown()
        self.thread.join(timeout=2)
        self.server.server_close()


def wait_child(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.02)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    raise AssertionError('actual owned SDK child did not exit within budget')


def run_one(binary, scenario):
    fixture = Fixture(scenario)
    private_read, private_write = os.pipe()
    input_read, input_write = os.pipe()
    output_read, output_write = os.pipe()
    err_read, err_write = os.pipe()
    root, child = socket.socketpair()
    root.settimeout(12)
    generation = 17
    nonce = os.urandom(32)
    document = {'format': 1, 'generation': generation, 'core_uid': 1001, 'engine_uid': 1002,
                'seed': '23' * 32, 'core_pin': '12' * 32, 'statistics_public_key': '34' * 32,
                'engine_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(), 'policy_sha256': '56' * 32,
                'seed_rpc': [f'http://127.0.0.1:{fixture.server.server_port}'], 'dns_servers': ['127.0.0.1:53'],
                'mode': 'isolated-loopback-fixture'}
    if scenario == 'wrong-executable':
        document['engine_sha256'] = 'ab' * 32
    if scenario == 'identity-reuse':
        document['statistics_public_key'] = document['core_pin']
    raw = json.dumps(document, separators=(',', ':')).encode()
    pid = os.fork()
    if pid == 0:
        try:
            # Duplicate to high temporary descriptors before mapping 0..4.
            copies = [os.dup(fd) for fd in [input_read, output_write, err_write, private_read, child.fileno()]]
            for dest, source in enumerate(copies):
                os.dup2(source, dest, inheritable=True)
            os.closerange(5, 4096)
            if ctypes.CDLL(None, use_errno=True).prctl(38, 1, 0, 0, 0) != 0:
                os._exit(96)
            os.setgroups([])
            os.setgid(1002)
            os.setuid(1002)
            os.execve(binary, [binary, '--root-stdio'], dict(os.environ))
        except BaseException:
            os._exit(97)
    child.close()
    fixture.start()  # Fork/exec precedes all owned server threads.
    for fd in [private_read, input_read, output_write, err_write]:
        os.close(fd)
    deadline_ms = int(time.clock_gettime(time.CLOCK_BOOTTIME) * 1000) + (1800 if scenario == 'expiry' else 20000)
    frame = b'RSHL\x01' + struct.pack('>IQ', generation, deadline_ms) + nonce + hashlib.sha256(raw).digest()
    if scenario == 'wrong-generation':
        frame = frame[:5] + struct.pack('>I', generation + 1) + frame[9:]
    root.sendall(frame)
    os.write(private_write, struct.pack('>I', len(raw)) + raw)
    os.close(private_write)
    rejected = scenario in ['wrong-executable', 'identity-reuse', 'wrong-generation', 'malformed-before-ready', 'startup-eof']
    try:
        if scenario == 'startup-eof':
            assert fixture.ready.wait(3)
            os.close(input_write)
            input_write = None
        if rejected:
            assert wait_child(pid) != 0
        else:
            assert exact(root, 41) == b'RSHR\x01' + struct.pack('>I', generation) + nonce
            request = json.dumps({'protocol': 1, 'id': '11111111-1111-4111-8111-111111111111', 'op': 'status'}).encode()
            os.write(input_write, struct.pack('>I', len(request)) + request)
            # Wrap only a duplicate; preserve the pipe's single owned close.
            stream = os.fdopen(os.dup(output_read), 'rb', buffering=0)
            assert select.select([stream], [], [], 3)[0]
            size = struct.unpack('>I', stream.read(4))[0]
            assert 0 < size <= 2048
            response = json.loads(stream.read(size))
            assert response['result']['nkn_sdk_linked'] is True
            assert response['result']['status']['generation'] == generation
            assert response['result']['status']['service'] == 'unknown'
            assert response['result']['status']['rx_bytes'] is None
            if scenario == 'stop':
                request = request.replace(b'"status"', b'"stop"')
                os.write(input_write, struct.pack('>I', len(request)) + request)
                size = struct.unpack('>I', stream.read(4))[0]
                assert json.loads(stream.read(size))['result']['kind'] == 'closing_this_bridge'
            elif scenario == 'parent-eof':
                root.close()
            elif scenario == 'stdio-eof':
                os.close(input_write)
                input_write = None
            elif scenario == 'websocket-eof':
                fixture.sockets[0].shutdown(socket.SHUT_RDWR)
            elif scenario == 'malformed-after-ready':
                ws_write(fixture.sockets[0], {})
            elif scenario == 'oversized-after-ready':
                ws_write(fixture.sockets[0], {'padding': 'x' * 4096})
            elif scenario != 'expiry':
                raise AssertionError('unknown fixture scenario')
            exitcode = wait_child(pid)
            if scenario == 'stop':
                assert exitcode == 0, exitcode
            stream.close()
        stderr = os.read(err_read, 8192)
        assert stderr == b'', 'SDK diagnostics escaped process redaction'
        assert fixture.ws_connections <= 1, 'SDK autonomously reconnected'
        if scenario in ['wrong-executable', 'identity-reuse', 'wrong-generation']:
            assert fixture.rpc_calls == fixture.ws_connections == 0
        return {'scenario': scenario, 'child_reaped': True, 'rpc_calls': fixture.rpc_calls,
                'websocket_connections': fixture.ws_connections, 'stderr_bytes': len(stderr)}
    finally:
        root.close()
        for fd in [input_write, output_read, err_read]:
            if fd is not None:
                os.close(fd)
        fixture.close()


def main():
    assert os.geteuid() == 0 and Path('/.dockerenv').exists()
    binary = sys.argv[1]
    # PID 1 is intentionally rejected as the authority; use the actual direct
    # Root manager process and kernel SO_PEERCRED, without monkeypatching them.
    pid = os.fork()
    if pid:
        _, status = os.waitpid(pid, 0)
        return os.waitstatus_to_exitcode(status)
    scenarios = ['stop', 'parent-eof', 'stdio-eof', 'websocket-eof', 'malformed-before-ready',
                 'malformed-after-ready', 'oversized-after-ready', 'startup-eof', 'expiry',
                 'wrong-executable', 'identity-reuse', 'wrong-generation']
    try:
        results = [run_one(binary, name) for name in scenarios]
        print(json.dumps({'fixture': 'actual SDK child, real low UID/kernel Root peer; owned loopback node',
                          'public_network': False, 'cgroup_or_egress_acceptance': False, 'results': results}), flush=True)
        os._exit(0)
    except BaseException:
        import traceback
        traceback.print_exc()
        os._exit(1)


if __name__ == '__main__':
    sys.exit(main())
