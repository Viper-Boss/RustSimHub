# SPDX-License-Identifier: AGPL-3.0-only
"""Read-only material plan for the D-24 review candidate, never an installer.

No account creation, socket, service call, bus reload, command execution, network,
credential read or target writes. Reported IDs are proposals, not reserved NSS IDs.
"""
import argparse
import configparser
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import xml.etree.ElementTree as ET

MATERIAL = Path(__file__).resolve().parent
MAX_FILE = 32768
EXPECTED = {
    'format': 1, 'stage': 'proposal', 'role': 'rustsimhub-ble',
    'proposed_uid': 982, 'proposed_gid': 982,
    'supplementary_group': 'rustsimhub-ipc', 'core_uid': 986, 'ipc_gid': 987,
    'gatt_contract': 't5-offline-adapters-v1',
    'unit_template': 'rustsimhub-ble.service.in',
    'sysusers_template': 'rustsimhub-ble.sysusers.in',
    'dbus_template': 'rustsimhub-ble.policy.in',
    'default_enabled': False, 'production_ready': False,
}
FILES = ('contract.proposal.json', 'rustsimhub-ble.service.in',
         'rustsimhub-ble.sysusers.in', 'rustsimhub-ble.policy.in')
DENIES = ({'own': '*'}, {'send_destination': '*'}, {'receive_sender': '*'})
BROKER = 'org.freedesktop.DBus'
BLUEZ = 'org.bluez'


def require(value, message):
    if not value:
        raise ValueError(message)


def unique(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def read_regular(path):
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'ordinary unlinked material required')
    require(before.st_size <= MAX_FILE, 'material size bound')
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0)
    flags |= getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
    with os.fdopen(os.open(path, flags), 'rb') as source:
        opened = os.fstat(source.fileno())
        require(stat.S_ISREG(opened.st_mode) and opened.st_nlink == 1 and opened.st_size <= MAX_FILE,
                'opened material metadata')
        require((before.st_dev, before.st_ino) == (opened.st_dev, opened.st_ino), 'material replaced')
        data = source.read(MAX_FILE + 1)
    require(len(data) <= MAX_FILE, 'growing material size bound')
    after = path.lstat()
    require((opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns) ==
            (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), 'material changed')
    return data


def validate_contract(data):
    require(isinstance(data, dict) and set(data) == set(EXPECTED), 'contract field allowlist')
    require(all(type(data[k]) is type(v) and data[k] == v for k, v in EXPECTED.items()),
            'only the fixed unapproved proposal is inspectable')


def validate_unit(text):
    parser = configparser.ConfigParser(interpolation=None, strict=True)
    parser.optionxform = str
    parser.read_file(io.StringIO(text))
    require(set(parser.sections()) == {'Unit', 'Service'} and not parser.defaults(), 'unit sections/defaults')
    unit = dict(parser['Unit'])
    require(unit == {
        'Description': 'RustSimHub optional BLE recovery transport',
        'ConditionPathExists': '/run/rustsimhub-ble/bridge-policy.json',
        'Requires': 'rustsimhub-core.service bluetooth.service',
        'After': 'rustsimhub-core.service bluetooth.service dbus.service',
        'BindsTo': 'rustsimhub-core.service bluetooth.service',
        'PartOf': 'rustsimhub-app.target',
    }, 'unit dependency/condition allowlist')
    # systemd permits repeated conditions; ConfigParser does not. Parse these
    # separately in the caller, retaining every AND-condition without flattening.
    service = dict(parser['Service'])
    expected = {
        'Type': 'exec', 'User': 'rustsimhub-ble', 'Group': 'rustsimhub-ble',
        'SupplementaryGroups': 'rustsimhub-ipc',
        'ExecStart': '/opt/rustsimhub/selected/rustsimhub-ble --service',
        'LoadCredential': 'bridge-policy.json:/run/rustsimhub-ble/bridge-policy.json',
        'BindReadOnlyPaths': '/run/rustsimhub-app/selected:/opt/rustsimhub/selected /run/rustsimhub/core /run/dbus',
        'InaccessiblePaths': '/var/lib/rustsimhub /var/lib/rustsimhub-core /var/lib/rustsimhub-edge /var/lib/rustsimhub-media /var/lib/rustsimhub-forwarder /run/rustsimhub-ble',
        'RuntimeDirectory': 'rustsimhub-ble-worker', 'RuntimeDirectoryMode': '0700',
        'ProtectSystem': 'strict', 'ProtectHome': 'yes', 'PrivateTmp': 'yes',
        'PrivateIPC': 'yes', 'PrivateNetwork': 'yes', 'PrivateDevices': 'yes',
        'ProtectKernelTunables': 'yes', 'ProtectKernelModules': 'yes',
        'ProtectControlGroups': 'yes', 'ProtectProc': 'invisible',
        'NoNewPrivileges': 'yes', 'CapabilityBoundingSet': '', 'AmbientCapabilities': '',
        'RestrictAddressFamilies': 'AF_UNIX', 'RestrictNamespaces': 'yes',
        'RestrictSUIDSGID': 'yes', 'LockPersonality': 'yes', 'DevicePolicy': 'closed',
        'KillMode': 'control-group', 'SendSIGKILL': 'yes', 'Delegate': 'no',
        'Restart': 'no', 'TimeoutStopSec': '25', 'MemoryMax': '48M',
        'TasksMax': '16', 'CPUQuota': '10%', 'UMask': '0077', 'LimitCORE': '0',
    }
    require(service == expected, 'service privilege/command/resource allowlist')


def validate_unit_conditions(text):
    lines = text.splitlines()
    conditions = [line for line in lines if line.startswith('ConditionPathExists=')]
    require(conditions == [
        'ConditionPathExists=/var/lib/rustsimhub/rustsimhub-v1/system/ble/enabled.json',
        'ConditionPathExists=/run/rustsimhub-ble/bridge-policy.json',
    ], 'both fixed AND-conditions required')
    # Keep the last fixed condition for the strict INI parser. Any other
    # duplicate, reset, alternate spelling or hidden condition is still rejected.
    lines.remove(conditions[0])
    validate_unit('\n'.join(lines))


def allowed_bus_rules():
    rules = []
    for member in ['Hello', 'GetNameOwner', 'GetId', 'AddMatch', 'RemoveMatch']:
        rules.append({'send_destination': BROKER, 'send_interface': BROKER, 'send_member': member})
    for interface, members in [
        ('org.bluez.GattManager1', ['RegisterApplication', 'UnregisterApplication']),
        ('org.bluez.LEAdvertisingManager1', ['RegisterAdvertisement', 'UnregisterAdvertisement']),
        ('org.freedesktop.DBus.ObjectManager', ['GetManagedObjects']),
        ('org.freedesktop.DBus.Properties', ['Get', 'GetAll']),
    ]:
        for member in members:
            rules.append({'send_destination': BLUEZ, 'send_interface': interface, 'send_member': member})
    rules.append({'send_destination': BLUEZ, 'send_interface': 'org.freedesktop.DBus.Properties',
                  'send_member': 'PropertiesChanged', 'send_type': 'signal'})
    for kind in ['method_return', 'error']:
        rules.append({'send_destination': BLUEZ, 'send_type': kind, 'send_requested_reply': 'true'})
    for kind in ['method_call', 'method_return', 'error']:
        rule = {'receive_sender': BLUEZ, 'receive_type': kind}
        if kind != 'method_call': rule['receive_requested_reply'] = 'true'
        rules.append(rule)
    for interface in ['org.freedesktop.DBus.Properties', 'org.freedesktop.DBus.ObjectManager']:
        rules.append({'receive_sender': BLUEZ, 'receive_interface': interface, 'receive_type': 'signal'})
    for kind in ['method_return', 'error']:
        rules.append({'receive_sender': BROKER, 'receive_type': kind, 'receive_requested_reply': 'true'})
    for member in ['NameOwnerChanged', 'NameAcquired', 'NameLost']:
        rules.append({'receive_sender': BROKER, 'receive_interface': BROKER,
                      'receive_member': member, 'receive_type': 'signal'})
    return rules


def validate_policy(data):
    require(b'<!ENTITY' not in data.upper(), 'entity definitions refused')
    root = ET.fromstring(data)
    require(root.tag == 'busconfig' and not root.attrib and len(root) == 1, 'bus policy root')
    policy = root[0]
    require(policy.tag == 'policy' and policy.attrib == {'user': 'rustsimhub-ble'}, 'only proposed user policy')
    rules = [(node.tag, node.attrib) for node in policy]
    expected = [('deny', rule) for rule in DENIES] + [('allow', rule) for rule in allowed_bus_rules()]
    require(rules == expected and all(len(node) == 0 for node in policy), 'ordered finite bus rules required')


def identity_availability(passwd, group):
    """Inventory is supplied evidence, not authenticated NSS or approval."""
    def rows(text, fields):
        require(type(text) is str and len(text.encode('utf8')) <= 262144, 'identity input bound')
        data = [line.split(':') for line in text.splitlines() if line]
        require(all(len(row) == fields for row in data), 'identity fields')
        require(len({r[0] for r in data}) == len(data) and all(r[0] for r in data), 'duplicate identity name')
        for row in data:
            for index in ([2, 3] if fields == 7 else [2]):
                number = row[index]
                require(number.isascii() and number.isdigit() and str(int(number)) == number
                        and int(number) < 2**32, 'canonical identity number')
        return data
    users, groups = rows(passwd, 7), rows(group, 4)
    require([(u[0], u[2], u[3]) for u in users if u[0] == 'root'] == [('root', '0', '0')],
            'complete supplied inventory requires root identity')
    for name, uid in [('core', 986), ('edge', 985), ('media', 984), ('forwarder', 983)]:
        name = 'rustsimhub-'+name
        require([(u[0], u[2], u[3]) for u in users if u[0] == name or u[2] == str(uid)] ==
                [(name, str(uid), str(uid))], 'existing fixed role missing or aliased')
        require([(g[0], g[2]) for g in groups if g[0] == name or g[2] == str(uid)] ==
                [(name, str(uid))], 'existing fixed group missing or aliased')
    require([(g[0], g[2]) for g in groups if g[0] == 'rustsimhub-ipc' or g[2] == '987'] ==
            [('rustsimhub-ipc', '987')], 'existing IPC group missing or aliased')
    require(not any(u[0] == 'rustsimhub-ble' or u[2] == '982' or u[3] == '982' for u in users),
            'proposed name/UID/primary GID occupied')
    require(not any(g[0] == 'rustsimhub-ble' or g[2] == '982'
                    or 'rustsimhub-ble' in g[3].split(',') for g in groups),
            'proposed group/GID/member occupied')
    return {'proposed_uid_available_in_supplied_inventory': True,
            'actual_nss_verified': False, 'uid_reserved': False}


def inspect(directory=MATERIAL, inventory=None):
    directory = Path(directory)
    require(stat.S_ISDIR(directory.lstat().st_mode), 'ordinary material directory required')
    files = {name: read_regular(directory/name) for name in FILES}
    validate_contract(json.loads(files[FILES[0]], object_pairs_hook=unique))
    validate_unit_conditions(files[FILES[1]].decode('utf8'))
    sysusers = [line for line in files[FILES[2]].decode('utf8').splitlines() if line and not line.startswith('#')]
    require(sysusers == [
        'u rustsimhub-ble 982 "RustSimHub optional BLE transport" /nonexistent /usr/sbin/nologin',
        'm rustsimhub-ble rustsimhub-ipc',
    ], 'fixed proposed sysusers only')
    validate_policy(files[FILES[3]])
    require(all(read_regular(directory/name) == data for name, data in files.items()),
            'material bundle changed during inspection')
    result = {
        'format': 1, 'kind': 'ble-service-review-plan', 'production_ready': False,
        'install_permitted': False, 'default_enabled': False, 'proposed_uid': 982,
        'files': [{'name': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
                  for name, data in files.items()],
        'requires': ['main_role_uid_review', 'current_ap_nss_guard_update_review',
                     'real_core_recovery_route', 'approved_secure_session',
                     'real_ble_service_binary', 'signed_root_material_admission',
                     'actual_nss_and_bluez_service_validation'],
    }
    if inventory is not None:
        require(type(inventory) is dict and set(inventory) == {'passwd', 'group'}, 'inventory allowlist')
        result['identity'] = identity_availability(inventory['passwd'], inventory['group'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()  # No install/enable/reload/output-path or override options.
    try:
        print(json.dumps(inspect(), ensure_ascii=False, indent=2))
    except (ValueError, OSError, UnicodeError, configparser.Error, ET.ParseError):
        # Do not echo source contents, credentials or arbitrary inventory fields.
        raise SystemExit('BLE review material rejected')


if __name__ == '__main__':
    main()
