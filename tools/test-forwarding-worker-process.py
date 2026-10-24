#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real worker/Core/DNS/TLS process test, in a disposable network namespace.

Run as Root AFTER `unshare --net`; only its loopback is brought up. No host,
NAS, modem, external endpoint, system trust or installed service is touched.
Root only creates disposable identities/processes; the worker has no caps.
"""
import argparse
import json
import multiprocessing
import os
from pathlib import Path
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import time
import uuid

EDGE, CORE, WORKER = 65529, 65530, 65531


def drop(uid):
    def apply():
        os.setgroups([CORE])
        os.setgid(CORE)
        os.setuid(uid)
    return apply


def wait_for(check, seconds=5):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.025)
    raise AssertionError('bounded condition did not become true')


def dns_server(listener, control):
    # Deliberately tiny test fixture codec. Production uses Hickory, not this.
    count = 0
    listener.settimeout(0.1)
    try:
        while not control.poll():
            try:
                data, peer = listener.recvfrom(4097)
            except socket.timeout:
                continue
            assert len(data) <= 4096 and len(data) >= 12
            assert struct.unpack('!H', data[4:6])[0] == 1
            offset, labels = 12, []
            while data[offset]:
                size = data[offset]
                assert size <= 63
                labels.append(data[offset + 1:offset + 1 + size])
                offset += size + 1
            end = offset + 5
            assert end == len(data) and b'.'.join(labels) == b'fixture.test'
            assert data[offset + 1:end] == b'\x00\x01\x00\x01'
            reply = data[:2] + struct.pack('!5H', 0x8180, 1, 1, 0, 0)
            reply += data[12:end] + b'\xc0\x0c' + struct.pack('!2HIH', 1, 1, 0, 4)
            reply += socket.inet_aton('127.0.0.1')
            listener.sendto(reply, peer)
            count += 1
        assert control.recv() == 'stop'
        control.send({'dns_queries': count})
    finally:
        listener.close()


def tls_server(listener, control, cert, key, hold):
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    listener.settimeout(0.1)
    ids, handshake_failures = [], 0
    try:
        while not control.poll():
            try:
                stream, _ = listener.accept()
            except socket.timeout:
                continue
            stream.settimeout(6)
            try:
                stream = context.wrap_socket(stream, server_side=True)
            except ssl.SSLError:
                stream.close()
                handshake_failures += 1
                continue
            with stream:
                data = b''
                while b'\r\n\r\n' not in data:
                    part = stream.recv(1024)
                    assert part and len(data) + len(part) <= 32768
                    data += part
                header, body = data.split(b'\r\n\r\n', 1)
                lines = header.split(b'\r\n')
                assert lines[0] == b'POST /synthetic-only HTTP/1.1'
                fields = {}
                for line in lines[1:]:
                    name, value = line.split(b':', 1)
                    assert name.lower() not in fields
                    fields[name.lower()] = value.strip()
                assert fields[b'authorization'] == b'Bearer synthetic'
                size = int(fields[b'content-length'])
                assert 0 < size <= 32768
                while len(body) < size:
                    part = stream.recv(size - len(body))
                    assert part
                    body += part
                assert len(body) == size
                json.loads(body)
                assert b'synthetic body' not in body, 'notice must not share full text'
                delivery = fields[b'x-rustsimhub-delivery-id'].decode('ascii')
                assert str(uuid.UUID(delivery)) == delivery and delivery not in ids
                ids.append(delivery)
                control.send({'event': 'posted', 'count': len(ids)})
                if hold:
                    # No HTTP response: the real worker must close its owned
                    # TLS transport when it receives SIGTERM after admission.
                    try:
                        assert stream.recv(1) == b''
                    except ssl.SSLEOFError:
                        pass
                    control.send({'event': 'peer_closed'})
                else:
                    stream.sendall(b'HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
        assert control.recv() == 'stop'
        control.send({'http_requests': len(ids), 'handshake_failures': handshake_failures})
    finally:
        listener.close()


def stop_process(process, clean=False):
    if process is None:
        return
    if process.poll() is None:
        process.terminate()
    try:
        code = process.wait(timeout=6)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)
        raise AssertionError('owned process failed bounded stop')
    if clean:
        assert code == 0, 'owned process did not exit cleanly'


def run_case(core_binary, worker_binary, mode):
    with tempfile.TemporaryDirectory(prefix='rustsimhub-worker-process-') as temporary:
        base = Path(temporary)
        assert base.resolve().parent == Path(tempfile.gettempdir()).resolve()
        base.chmod(0o711)
        for name, binary in [('core', core_binary), ('worker', worker_binary),
                             ('client.py', Path(__file__).with_name('test-forwarding-worker-ipc.py'))]:
            shutil.copyfile(binary, base / name)
            (base / name).chmod(0o755)
        for name, permission in [('state', 0o700), ('identity', 0o700), ('runtime', 0o750)]:
            folder = base / name
            folder.mkdir(mode=permission)
            os.chown(folder, CORE, CORE)

        def openssl(*args):
            subprocess.run(['openssl', *args], check=True, capture_output=True, timeout=10, cwd=base)

        openssl('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
                '-subj', '/CN=Disposable forwarding test root', '-keyout', 'ca.key',
                '-out', 'ca.pem', '-addext', 'basicConstraints=critical,CA:TRUE')
        openssl('req', '-new', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=fixture.test',
                '-keyout', 'server.key', '-out', 'server.csr')
        (base / 'extensions').write_text('basicConstraints=critical,CA:FALSE\n'
            'subjectAltName=DNS:fixture.test\nextendedKeyUsage=serverAuth\n')
        openssl('x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key',
                '-CAcreateserial', '-days', '1', '-extfile', 'extensions', '-out', 'server.pem')
        for name in ['ca.key', 'server.key']:
            (base / name).chmod(0o600)
        (base / 'ca.pem').chmod(0o644)
        if mode == 'wrong_ca':
            openssl('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
                    '-subj', '/CN=Untrusted disposable root', '-keyout', 'other.key',
                    '-out', 'other.pem', '-addext', 'basicConstraints=critical,CA:TRUE')
            (base / 'other.key').chmod(0o600)
            (base / 'other.pem').chmod(0o644)
        dns = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        dns.bind(('127.0.0.1', 0))
        tcp = socket.socket()
        tcp.bind(('127.0.0.1', 0))
        tcp.listen(4)
        dns_port, tls_port = dns.getsockname()[1], tcp.getsockname()[1]
        ctx = multiprocessing.get_context('fork')
        dc, dchild = ctx.Pipe()
        tc, tchild = ctx.Pipe()
        ds = ctx.Process(target=dns_server, args=(dns, dchild))
        ts = ctx.Process(target=tls_server, args=(tcp, tchild, str(base / 'server.pem'),
                                                str(base / 'server.key'), mode == 'lost_ack'))
        core = worker = None
        events = []
        started = []
        path = str(base / 'runtime/core.sock')

        def rpc(uid, command, protocol=4):
            request = {'protocol': protocol, 'id': str(uuid.uuid4()), 'command': command}
            result = subprocess.run([sys.executable, str(base / 'client.py'), '--client', path],
                input=json.dumps(request), text=True, capture_output=True, timeout=5,
                preexec_fn=drop(uid), cwd=base)
            assert result.returncode == 0, 'real Core transport failed'
            value = json.loads(result.stdout)
            assert 'error' not in value, 'real Core refused operation'
            return value

        def event(name, count=None):
            while tc.poll():
                try:
                    events.append(tc.recv())
                except (EOFError, ConnectionResetError):
                    break
            return any(v.get('event') == name and (count is None or v.get('count') == count)
                       for v in events)

        def receipts():
            return rpc(EDGE, {'op': 'forwarding_receipts', 'token': owner,
                'after': '0', 'limit': 32}, 2)['receipts']

        def count_receipts(state):
            return sum(v['state'] == state for v in receipts())

        try:
            # No threads in this Root parent: numeric UID changes occur only
            # in freshly forked children, never a multithreaded preexec path.
            for server in [ds, ts]:
                server.start()
                started.append(server)
            dns.close()
            tcp.close()
            dchild.close()
            tchild.close()
            with (base / 'core.out').open('wb') as out, (base / 'core.err').open('wb') as err:
                os.chmod(base / 'core.out', 0o600)
                os.chmod(base / 'core.err', 0o600)
                core = subprocess.Popen([str(base / 'core'), '--forwarding-ipc-fixture', path,
                    str(EDGE), str(WORKER), str(base / 'state'), str(base / 'identity'),
                    f'https://fixture.test:{tls_port}/synthetic-only'],
                    stdout=out, stderr=err, preexec_fn=drop(CORE), cwd=base)

            def ready():
                assert core.poll() is None, 'fixture Core exited before readiness'
                lines = (base / 'core.out').read_text().splitlines()
                return json.loads(lines[0]) if lines else None
            owner = wait_for(ready, 8)['owner_token']
            lease = rpc(0, {'op': 'grant'})['lease']
            settings = {'core_socket': path, 'core_uid': CORE, 'lease': lease,
                'route': {'interface': 'lo', 'ifindex': 99 if mode == 'wrong_route' else 1,
                    'ipv4': '127.0.0.1', 'ipv6': None, 'dns_servers': [f'127.0.0.1:{dns_port}']},
                'ca_bundle': str(base / ('other.pem' if mode == 'wrong_ca' else 'ca.pem'))}
            with (base / 'worker.err').open('wb') as err:
                os.chmod(base / 'worker.err', 0o600)
                worker = subprocess.Popen([str(base / 'worker'), '--worker'], stdin=subprocess.PIPE,
                    stdout=subprocess.DEVNULL, stderr=err, preexec_fn=drop(WORKER), cwd=base)
                worker.stdin.write(json.dumps(settings).encode())
                worker.stdin.close()
            if mode in ('confirmed', 'lost_ack'):
                wait_for(lambda: event('posted', 2 if mode == 'confirmed' else 1), 8)
                status = Path(f'/proc/{worker.pid}/status').read_text().splitlines()
                for field in ['CapEff:', 'CapPrm:', 'CapAmb:']:
                    assert int(next(v.split()[1] for v in status if v.startswith(field)), 16) == 0
                assert worker.poll() is None
            else:
                assert worker.wait(timeout=8) == 1
            if mode == 'confirmed':
                wait_for(lambda: count_receipts('confirmed') == 2)
            disabled = rpc(EDGE, {'op': 'set_forwarding_enabled', 'token': owner,
                'expected_generation': 1, 'request_id': str(uuid.uuid4()), 'enabled': False}, 2)
            assert disabled['enabled'] is False
            stopped_at = time.monotonic()
            stop_process(worker, clean=mode in ('confirmed', 'lost_ack'))
            assert time.monotonic() - stopped_at < 6
            if mode == 'lost_ack':
                wait_for(lambda: event('peer_closed'), 2)
            rpc(0, {'op': 'revoke'})
            state = rpc(0, {'op': 'state'})
            assert state['enabled'] is False and state['lease_active'] is False
            assert state['process_state_verified'] is False  # No installed Root supervisor yet.
            if mode == 'confirmed':
                assert count_receipts('confirmed') == 2 and state['unknown'] == 0
            elif mode == 'lost_ack':
                assert state['unknown'] == 1 and count_receipts('confirmed') == 0
            elif mode == 'wrong_ca':
                assert count_receipts('failed') == 1 and count_receipts('confirmed') == 0
            else:
                assert count_receipts('confirmed') == 0
            stop_process(core, clean=True)
            assert not Path(path).exists()
            dc.send('stop')
            tc.send('stop')
            assert dc.poll(2)
            dns_result = dc.recv()
            wait_for(lambda: drain_result(tc, events), 3)
            tls_result = next(v for v in events if 'http_requests' in v)
            for server in started:
                server.join(timeout=3)
                assert server.exitcode == 0, 'fixture server failed or leaked'
            expected = {'confirmed': 2, 'lost_ack': 1, 'wrong_route': 0, 'wrong_ca': 0}[mode]
            assert tls_result['http_requests'] == expected
            assert dns_result['dns_queries'] == (2 if mode == 'confirmed' else 0 if mode == 'wrong_route' else 1)
            if mode == 'wrong_ca':
                assert tls_result['handshake_failures'] == 1
            return {'case': mode, 'passed': True, 'http_requests': expected,
                    'actual_worker_reaped': True, 'external_network': False}
        finally:
            stop_process(worker)
            stop_process(core)
            for server in started:
                if server.is_alive():
                    server.terminate()
                server.join(timeout=3)
                if server.is_alive():
                    server.kill()
                    server.join(timeout=3)
            for connection in [dc, tc, dchild, tchild]:
                connection.close()
            dns.close()
            tcp.close()


def drain_result(connection, events):
    while connection.poll():
        try:
            events.append(connection.recv())
        except (EOFError, ConnectionResetError):
            break
    return any('http_requests' in v for v in events)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core', type=Path, required=True)
    parser.add_argument('--worker', type=Path, required=True)
    args = parser.parse_args()
    assert os.geteuid() == 0 and socket.if_nameindex() == [(1, 'lo')], 'require fresh unshare --net'
    subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
    results = [run_case(args.core.resolve(strict=True), args.worker.resolve(strict=True), mode)
               for mode in ['confirmed', 'lost_ack', 'wrong_route', 'wrong_ca']]
    print(json.dumps({'passed': True, 'cases': results, 'device_access': False,
                      'installed_service_verified': False}))
