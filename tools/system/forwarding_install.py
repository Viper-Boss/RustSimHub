#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build a finite, reviewable first-install maintenance bundle off-device.

No SSH credentials, network access, database restore or account mutation here.
The target executes the bundled POSIX helper; Python is not needed on the stick.
This first installer accepts only the actually validated epoch47 runtime. Later
runtime sources require independent compatibility evidence before extending it.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
MATERIAL = ROOT / 'firmware/system/forwarding'
RUNTIME_SOURCE = 'b4e54a564eda7c64ca7513d1e16b7bdaf0924f0831dfef1cf3e579caf8075e0c'
RUNTIME_MANIFEST = '01ca325373355ef1b34f2a886e2e1b346378402f33eb3217a43192d9719d1153'
BASE_EXEC = ('/opt/rustsimhub/selected/rustsimhub-core --serve-device '
    '/run/rustsimhub/core/core.sock 985 /var/lib/rustsimhub-core/state '
    '/var/lib/rustsimhub-core/identity /run/rustsimhub/core/media.sock 984 '
    '/var/lib/rustsimhub-core/archive /var/lib/rustsimhub-core/access '
    '/var/lib/rustsimhub-core/compat /var/lib/rustsimhub-core/journal '
    '/var/lib/rustsimhub-core/setup')
MANAGED_DROPIN = ('# SPDX-License-Identifier: AGPL-3.0-only\n[Service]\nExecStart=\nExecStart=' +
    BASE_EXEC.replace('--serve-device ', '--serve-device-forwarding-managed ') + ' 983\n').encode()
FILES = [
    ('rustsimhub-forwarder.service', '/etc/systemd/system/rustsimhub-forwarder.service', 0o644),
    ('rustsimhub-forwarder-reconcile.service', '/etc/systemd/system/rustsimhub-forwarder-reconcile.service', 0o644),
    ('rustsimhub-forwarder-status.service', '/etc/systemd/system/rustsimhub-forwarder-status.service', 0o644),
    ('rustsimhub-forwarder-boot.service', '/etc/systemd/system/rustsimhub-forwarder-boot.service', 0o644),
    ('46-rustsimhub-forwarding.rules', '/etc/polkit-1/rules.d/46-rustsimhub-forwarding.rules', 0o644),
    ('forwarder.json', '/etc/rustsimhub/forwarder.json', 0o600),
    ('tmpfiles.conf', '/etc/tmpfiles.d/rustsimhub-forwarding.conf', 0o644),
    ('20-forwarding.conf', '/etc/systemd/system/rustsimhub-app.target.d/20-forwarding.conf', 0o644),
    # Publish the new Core arguments last. No service is restarted by apply.sh.
    (None, '/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf', 0o644),
]
PERSISTENT = '/var/lib/rustsimhub/rustsimhub-v1/system/forwarding'
RUNTIME = '/run/rustsimhub-forwarding'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(value, message):
    if not value:
        raise ValueError(message)


def identities(passwd, group):
    """Strict local + resolved NSS inventory, including aliases/primary groups."""
    def rows(text, count, number_fields):
        require(isinstance(text, str) and len(text) <= 262144, 'account inventory bound')
        result = [line.split(':') for line in text.splitlines() if line]
        require(all(len(x) == count for x in result), 'malformed account record')
        require(len({x[0] for x in result}) == len(result), 'duplicate account name')
        require(all(x[i].isascii() and x[i].isdigit() for x in result for i in number_fields), 'non-numeric identity')
        return result
    users = rows(passwd, 7, [2, 3])
    groups = rows(group, 4, [2])
    for name, uid in [('rustsimhub-core', 986), ('rustsimhub-edge', 985), ('rustsimhub-media', 984)]:
        require([(x[0], int(x[2]), int(x[3])) for x in users if x[0] == name or int(x[2]) == uid] == [(name, uid, uid)], 'service UID missing/aliased')
        require([(x[0], int(x[2])) for x in groups if x[0] == name or int(x[2]) == uid] == [(name, uid)], 'service GID missing/aliased')
        require(all(x[0] == name for x in users if int(x[3]) == uid), 'foreign primary service GID')
    require([(x[0], int(x[2])) for x in groups if x[0] == 'rustsimhub-ipc' or int(x[2]) == 987] == [('rustsimhub-ipc', 987)], 'IPC GID missing/aliased')
    require(not any(int(x[3]) == 987 for x in users), 'IPC primary group')
    require(not any(x[0] == 'rustsimhub-forwarder' or int(x[2]) == 983 or int(x[3]) == 983 for x in users), 'forwarder account/UID/primary GID occupied')
    require(not any(x[0] == 'rustsimhub-forwarder' or int(x[2]) == 983 for x in groups), 'forwarder group/GID occupied')
    require(not any('rustsimhub-forwarder' in x[3].split(',') for x in groups), 'stale forwarding supplementary membership')


def validate(inventory):
    require(inventory.get('format') == 1, 'inventory format')
    for prefix in ['local', 'nss']:
        identities(inventory[prefix + '_passwd'], inventory[prefix + '_group'])
    app = inventory['application']
    require(app['source_sha256'] == RUNTIME_SOURCE and app['manifest_sha256'] == RUNTIME_MANIFEST, 'unvalidated runtime')
    require(app['manifest_format'] == 2 and app['epoch'] == 47 and app['schema_min'] == app['schema_max'] == 6, 'runtime format/schema')
    require(app['confirmed'] == {'epoch': 47, 'hash': RUNTIME_MANIFEST} and app['floor'] == 47, 'compatible confirmed baseline missing')
    require(app['candidate'] is None and app['attempted'] is False and app['verified'] is True, 'pending/unverified application')
    require(inventory['core_exec_start'] == BASE_EXEC, 'unexpected Core command or existing drop-in')
    require(type(inventory['systemd_version']) is int and inventory['systemd_version'] >= 247, 'LoadCredential unsupported')
    require(inventory['forwarder_uid_pids'] == [], 'forwarder UID still has processes')
    require(inventory['active_calls'] == 0 and inventory['pcm_closed'] is True and inventory['ordinary_data_connected'] is False, 'maintenance not idle/data off')
    require(inventory['recovery_used'] is False and inventory['backup_verified'] is True, 'backup or recovery guard')
    require(re.fullmatch(r'[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}', inventory['boot_id']) is not None, 'boot ID')
    for field in ['ledger_sha256', 'local_passwd_sha256', 'local_group_sha256', 'core_unit_sha256', 'backup_sha256']:
        require(re.fullmatch('[a-f0-9]{64}', inventory[field]) is not None, 'invalid digest')
    require(inventory['local_passwd_sha256'] == sha(inventory['local_passwd'].encode()), 'passwd bytes not bound')
    require(inventory['local_group_sha256'] == sha(inventory['local_group'].encode()), 'group bytes not bound')
    require(set(inventory['destinations']) == {x[1] for x in FILES} | {PERSISTENT, RUNTIME}, 'destination inventory')
    require(all(value == 'absent' for value in inventory['destinations'].values()), 'existing path requires explicit recovery, never overwrite')
    return inventory


def create_bundle(inventory, output, backup):
    validate(inventory)
    require(output.is_absolute() and not output.exists(), 'fresh output directory required')
    require(backup.is_file() and not backup.is_symlink() and backup.stat().st_size <= 128 * 1024**2, 'backup file bound')
    backup_bytes = backup.read_bytes()
    require(sha(backup_bytes) == inventory['backup_sha256'], 'backup readback mismatch')
    payloads = {}
    entries = []
    for index, (origin, target, mode) in enumerate(FILES):
        filename = 'payload-' + str(index)
        data = MANAGED_DROPIN if origin is None else (MATERIAL / origin).read_bytes().replace(b'\r\n', b'\n')
        payloads[filename] = data
        entries.append({'name': filename, 'target': target, 'mode': mode, 'sha256': sha(data)})
    for name, key in [('ledger.sha256', 'ledger_sha256'), ('passwd.sha256', 'local_passwd_sha256'), ('group.sha256', 'local_group_sha256'), ('core-unit.sha256', 'core_unit_sha256')]:
        payloads[name] = (inventory[key] + '\n').encode()
    payloads['confirmed.sha256'] = (RUNTIME_MANIFEST + '\n').encode()
    payloads['boot-id'] = (inventory['boot_id'] + '\n').encode()
    payloads['business-before.tar'] = backup_bytes
    payloads['apply.sh'] = (MATERIAL / 'install.sh').read_bytes().replace(b'\r\n', b'\n')
    plan = {'format': 1, 'kind': 'first-forwarding-install', 'runtime_manifest_sha256': RUNTIME_MANIFEST,
        'runtime_source_sha256': RUNTIME_SOURCE, 'creates_forwarder_uid': 983,
        'starts_services': False, 'default_enabled': False, 'restores_business_database': False,
        'files': entries, 'backup_sha256': inventory['backup_sha256'],
        'partial_account_policy': 'leave locked unused account for explicit recovery; never delete or reassign automatically'}
    payloads['plan.json'] = (json.dumps(plan, sort_keys=True, indent=2) + '\n').encode()
    output.mkdir(mode=0o700)
    for name, data in payloads.items():
        (output / name).write_bytes(data)
        (output / name).chmod(0o600)
    # Windows is a supported maintenance host. Never let host text I/O turn
    # the POSIX checksum filename delimiter into a trailing carriage return.
    (output / 'SHA256SUMS').write_bytes(''.join(sha(data) + '  ' + name + '\n' for name, data in sorted(payloads.items())).encode('ascii'))
    (output / 'SHA256SUMS').chmod(0o600)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inventory', required=True, type=Path)
    parser.add_argument('--backup', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    plan = create_bundle(json.loads(args.inventory.read_text()), args.output, args.backup)
    print(json.dumps({'prepared': True, 'files': len(plan['files']), 'default_enabled': False, 'device_changed': False}))


if __name__ == '__main__':
    main()
