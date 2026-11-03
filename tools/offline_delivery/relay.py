# SPDX-License-Identifier: AGPL-3.0-only
"""C30: finite private TURN templates; placeholders deliberately prevent launch."""
import re
from .common import canonical, digest, exact, need, number, write_bundle, require_completed


def render(config):
    exact(config, ('schema', 'hostname', 'tls_port', 'relay_min', 'relay_max', 'max_allocations', 'lifetime_seconds'))
    need(type(config['schema']) is int and config['schema'] == 1, 'schema')
    host = config['hostname']
    need(isinstance(host, str) and len(host) <= 253 and '.' in host and host == host.lower(), 'hostname')
    need(all(re.fullmatch(r'[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?', label) for label in host.split('.')), 'hostname')
    need(not all(label.isdecimal() for label in host.split('.')), 'hostname')
    port = number(config['tls_port'], 1024, 65535)
    low = number(config['relay_min'], 49152, 65535)
    high = number(config['relay_max'], low, min(low + 31, 65535))
    need(not low <= port <= high, 'port_overlap')
    quota = number(config['max_allocations'], 1, 8)
    need(high - low + 1 >= quota * 2, 'relay_port_budget')
    lifetime = number(config['lifetime_seconds'], 60, 600)
    turn = f'''# coturn 4.6.1-2; candidate only; substitute private placeholders outside Git.
# TCP/TLS control, UDP relay only, matching the existing Rust TURN client.
realm={host}
server-name={host}
listening-ip=@PRIVATE_LISTEN_IP@
relay-ip=@PRIVATE_RELAY_IP@
external-ip=@PRIVATE_PUBLIC_IP@
tls-listening-port={port}
no-udp
no-tcp
no-dtls
no-tcp-relay
no-rfc5780
no-stun
no-tlsv1
no-tlsv1_1
use-auth-secret
static-auth-secret=@PRIVATE_AUTH_SECRET@
cert=@PRIVATE_CERT_CHAIN_PATH@
pkey=@PRIVATE_KEY_PATH@
min-port={low}
max-port={high}
total-quota={quota}
user-quota=1
max-allocate-lifetime={lifetime}
max-allocate-timeout=10
max-bps=64000
bps-capacity=256000
stale-nonce=60
no-cli
no-multicast-peers
denied-peer-ip=0.0.0.0-0.255.255.255
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=100.64.0.0-100.127.255.255
denied-peer-ip=127.0.0.0-127.255.255.255
denied-peer-ip=169.254.0.0-169.254.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=::1
denied-peer-ip=fc00::-fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff
denied-peer-ip=fe80::-febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff
'''.encode()
    service = b'''# Review-only template, not installed/enabled by this tool.
[Unit]
Description=RustSimHub finite private TURN experiment
[Service]
Type=simple
User=turnserver
Group=turnserver
ExecStart=/usr/bin/turnserver -c /run/rustsimhub-private-turn/turnserver.conf
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
MemoryMax=128M
TasksMax=16
CPUQuota=200%
TimeoutStopSec=5s
Restart=no
'''
    dns = f'{host}. 300 IN A @PRIVATE_PUBLIC_IPV4@\n'.encode()
    plan = {'schema': 1, 'configuration_complete': False, 'deployed': False,
            'coturn_version': '4.6.1-2', 'budget': {'memory_mib': 128, 'tasks': 16, 'cpu_percent': 200, 'stop_seconds': 5, 'allocations': quota, 'allocation_seconds': lifetime, 'session_bytes_per_second': 64000, 'server_bytes_per_second': 256000, 'allocate_timeout_seconds': 10},
            'certificate': {'hostname': host, 'minimum_tls': '1.2', 'checks': ['SAN matches hostname', 'chain validates pinned CA', 'expiry exceeds experiment window', 'private key mode 0600 outside repository']},
            'stop_cleanup': ['stop rustsimhub-private-turn.service', 'verify unit inactive and experiment sockets absent', 'remove only experiment credentials and configuration from /run/rustsimhub-private-turn', 'remove experiment DNS record after stopping; preserve public test receipts'],
            'authorization_verified': False}
    files = [('turnserver.conf.template', turn, 0o600), ('rustsimhub-private-turn.service.template', service, 0o600), ('dns.zone.template', dns, 0o600), ('private-plan.json', canonical(plan), 0o600)]
    report = {'schema': 1, 'kind': 'private-relay-template', 'configuration_complete': False, 'deployed': False, 'authorization_verified': False,
              'files': {p: digest(data) for p, data, _ in files}, 'config': config}
    return files, report


def compose(config, output):
    files, report = render(config)
    return write_bundle(output, files, report)


def validate(config, directory):
    files, report = render(config)
    for path, expected, _ in files:
        need(directory.read(path, 65536, 0o600) == expected, 'template_changed')
    need(directory.read('report.json', 65536) == canonical(report), 'report_changed')
    require_completed(directory)
    return report
