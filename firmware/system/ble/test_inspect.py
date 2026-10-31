# SPDX-License-Identifier: AGPL-3.0-only
"""Inspect the real candidate; system tools use temporary roots/private Unix bus.

No system bus, HCI, NSS edit, service start or production target is used.
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

MATERIAL = Path(__file__).resolve().parent
ROOT = MATERIAL.parents[2]
spec = importlib.util.spec_from_file_location('ble_review_material_plan', MATERIAL/'material_plan.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def inventory():
    pairs = [('core', 986), ('edge', 985), ('media', 984), ('forwarder', 983)]
    passwd = 'root:x:0:0:root:/root:/bin/sh\n' + ''.join(
        f'rustsimhub-{name}:x:{uid}:{uid}::/nonexistent:/usr/sbin/nologin\n' for name, uid in pairs)
    group = 'root:x:0:\n' + ''.join(f'rustsimhub-{name}:x:{uid}:\n' for name, uid in pairs)
    group += 'audio:x:29:rustsimhub-media\nrustsimhub-ipc:x:987:' + ','.join('rustsimhub-'+n for n, _ in pairs) + '\n'
    return {'passwd': passwd, 'group': group}


class Inspection(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.material = Path(self.temp.name)/'material'
        self.material.mkdir()
        for name in module.FILES:
            shutil.copyfile(MATERIAL/name, self.material/name)

    def test_real_material_is_a_bounded_hash_plan_and_never_an_approval(self):
        result = module.inspect(self.material, inventory())
        self.assertEqual(len(result['files']), 4)
        self.assertFalse(result['production_ready'])
        self.assertFalse(result['install_permitted'])
        self.assertFalse(result['default_enabled'])
        self.assertFalse(result['identity']['uid_reserved'])
        self.assertFalse(result['identity']['actual_nss_verified'])
        self.assertTrue(result['identity']['proposed_uid_available_in_supplied_inventory'])
        self.assertIn('dedicated_recovery_directory_review', result['requires'])
        self.assertEqual(result['supplementary_groups'], [])
        self.assertEqual(result['recovery_endpoint']['recovery_directory_mode'], '2750')
        self.assertNotIn('passwd', json.dumps(result))
        before = {p.name: p.read_bytes() for p in self.material.iterdir()}
        module.inspect(self.material)
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.material.iterdir()})

    def test_contract_cannot_turn_proposal_into_production_or_override_paths_roles(self):
        original = json.loads((self.material/module.FILES[0]).read_bytes())
        for key, value in [
            ('format', True), ('stage', 'approved'), ('production_ready', True),
            ('default_enabled', True), ('proposed_uid', 0), ('proposed_gid', 987),
            ('sysusers_template', '../../etc/passwd'), ('role', 'root'), ('approved', True),
        ]:
            data = dict(original); data[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                module.validate_contract(data)
        with self.assertRaises(ValueError):
            module.unique([('format', 1), ('format', 1)])

    def test_service_broadening_and_autostart_reset_or_extra_commands_are_rejected(self):
        original = (self.material/module.FILES[1]).read_text()
        mutations = [
            ('User=rustsimhub-ble', 'User=root'),
            ('ExecStart=/opt/rustsimhub/selected/rustsimhub-ble --service', 'ExecStart=/bin/sh -c anything'),
            ('CapabilityBoundingSet=', 'CapabilityBoundingSet=CAP_NET_ADMIN'),
            ('RestrictAddressFamilies=AF_UNIX', 'RestrictAddressFamilies=AF_UNIX AF_INET'),
            ('MemoryMax=48M', 'MemoryMax=infinity'), ('Restart=no', 'Restart=always'),
            ('PrivateDevices=yes', 'PrivateDevices=no'), ('TimeoutStopSec=25', 'TimeoutStopSec=1'),
            ('RuntimeDirectoryMode=0700', 'RuntimeDirectoryMode=0777'),
            ('ConditionPathExists=/run/rustsimhub-ble/bridge-policy.json', 'ConditionPathExists=|/tmp/any'),
            ('SupplementaryGroups=', 'SupplementaryGroups=rustsimhub-ipc'),
            ('SupplementaryGroups=', 'SupplementaryGroups=bluetooth audio'),
            ('/run/rustsimhub/core-recovery /run/dbus', '/run/rustsimhub/core /run/dbus'),
        ]
        for before, after in mutations:
            with self.subTest(before=before), self.assertRaises((ValueError, module.configparser.Error)):
                module.validate_unit_conditions(original.replace(before, after))
        for tail in ['\n[Install]\nWantedBy=multi-user.target\n', '\nExecStartPre=/bin/sh\n',
                     '\nDeviceAllow=/dev/rfkill rw\n', '\nCapabilityBoundingSet=\n']:
            with self.subTest(tail=tail), self.assertRaises((ValueError, module.configparser.Error)):
                module.validate_unit_conditions(original+tail)

    def test_bus_policy_cannot_add_wildcard_global_role_adapter_set_or_other_service(self):
        original = ET.fromstring((self.material/module.FILES[3]).read_bytes())
        for attributes in [
            {'send_destination': '*'}, {'own': 'org.rustsimhub.Owner'},
            {'send_destination': 'org.bluez', 'send_interface': 'org.freedesktop.DBus.Properties', 'send_member': 'Set'},
            {'send_destination': 'org.freedesktop.NetworkManager'}, {'receive_sender': '*'},
            {'receive_sender': 'org.bluez', 'receive_type': 'method_return'},
        ]:
            tree = copy.deepcopy(original); ET.SubElement(tree[0], 'allow', attributes)
            with self.subTest(attributes=attributes), self.assertRaises(ValueError):
                module.validate_policy(ET.tostring(tree))
        tree = copy.deepcopy(original); ET.SubElement(tree, 'policy', {'context': 'default'})
        with self.assertRaises(ValueError): module.validate_policy(ET.tostring(tree))
        tree = copy.deepcopy(original); tree[0].set('user', 'root')
        with self.assertRaises(ValueError): module.validate_policy(ET.tostring(tree))
        with self.assertRaises(ValueError): module.validate_policy(b'<!ENTITY anything "bad"><busconfig/>')

    def test_old_proposal_general_ipc_paths_and_membership_are_explicitly_refused(self):
        # Minimal failure sample from #53, not a newly invented permissive mode.
        unit = (self.material/module.FILES[1]).read_text()
        for changed in [unit.replace('SupplementaryGroups=', 'SupplementaryGroups=rustsimhub-ipc'),
                        unit.replace('/run/rustsimhub/core-recovery', '/run/rustsimhub/core')]:
            with self.assertRaises(ValueError): module.validate_unit_conditions(changed)
        sysusers = self.material/module.FILES[2]
        sysusers.write_text(sysusers.read_text()+'m rustsimhub-ble rustsimhub-ipc\n')
        with self.assertRaises(ValueError): module.inspect(self.material)

    def test_dedicated_endpoint_contract_cannot_be_widened_or_renamed(self):
        data = json.loads((self.material/module.FILES[0]).read_bytes())
        for key, value in [('supplementary_groups', ['rustsimhub-ipc']),
                           ('recovery_directory', '/run/rustsimhub/core'),
                           ('recovery_directory_owner', 'rustsimhub-ble'),
                           ('recovery_directory_group', 'rustsimhub-ipc'),
                           ('recovery_directory_mode', '2770'),
                           ('recovery_socket_mode', '0666')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                module.validate_contract({**data, key: value})

    def test_notification_fd_policy_refuses_global_value_and_unbounded_fds(self):
        original = ET.fromstring((self.material/module.FILES[3]).read_bytes())
        for attributes in [
            {'send_interface': 'org.freedesktop.DBus.Properties',
             'send_member': 'PropertiesChanged', 'send_type': 'signal'},
            {'send_destination': 'org.bluez', 'send_type': 'method_return',
             'send_requested_reply': 'true'},
            {'receive_sender': 'org.bluez', 'receive_type': 'method_call',
             'receive_member': 'AcquireNotify'},
        ]:
            tree = copy.deepcopy(original); ET.SubElement(tree[0], 'allow', attributes)
            with self.subTest(attributes=attributes), self.assertRaises(ValueError):
                module.validate_policy(ET.tostring(tree))
        for limit in ['2', 'unlimited']:
            tree = copy.deepcopy(original)
            reply = next(n for n in tree[0] if n.attrib.get('send_type') == 'method_return')
            reply.set('max_fds', limit)
            with self.subTest(limit=limit), self.assertRaises(ValueError):
                module.validate_policy(ET.tostring(tree))
        rules = module.allowed_bus_rules()
        acquired = [r for r in rules if r.get('receive_member') == 'AcquireNotify']
        self.assertEqual([r['receive_path'] for r in acquired],
                         [module.CHARACTERS[1], module.CHARACTERS[3]])
        self.assertTrue(all(r['max_fds'] == '0' for r in acquired))

    def test_sysusers_cannot_create_root_or_add_other_roles(self):
        path = self.material/module.FILES[2]
        original = path.read_text()
        for data in [original.replace('982', '0'), original+'m rustsimhub-core bluetooth\n',
                     original.replace('/usr/sbin/nologin', '/bin/sh')]:
            path.write_text(data)
            with self.assertRaises(ValueError): module.inspect(self.material)
        path.write_text(original)

    def test_identity_aliases_and_claimed_approval_are_not_sufficient_evidence(self):
        original = inventory()
        for field, row in [
            ('passwd', 'somebody:x:982:500::/tmp:/bin/sh\n'),
            ('passwd', 'somebody:x:1001:982::/tmp:/bin/sh\n'),
            ('passwd', 'rustsimhub-ble:x:1001:1001::/tmp:/bin/sh\n'),
            ('group', 'somebody:x:982:\n'), ('group', 'rustsimhub-ble:x:1001:\n'),
            ('group', 'somebody:x:1001:rustsimhub-ble\n'),
        ]:
            data = dict(original); data[field] += row
            with self.subTest(row=row), self.assertRaises(ValueError): module.inspect(self.material, data)
        data = dict(original); data['approved'] = True
        with self.assertRaises(ValueError): module.inspect(self.material, data)
        for bad in ['root:x:00:0:root:/root:/bin/sh\n', 'root:x:-1:0:root:/root:/bin/sh\n',
                    'root:x:0:0:root:/root:/bin/sh\nroot:x:0:0:root:/root:/bin/sh\n']:
            with self.assertRaises(ValueError): module.identity_availability(bad, original['group'])

    def test_empty_or_partial_inventory_cannot_claim_the_proposed_uid_is_available(self):
        original = inventory()
        for data in [
            {'passwd': '', 'group': ''},
            {'passwd': 'root:x:0:0:root:/root:/bin/sh\n', 'group': 'root:x:0:\n'},
            {**original, 'passwd': original['passwd'].replace('rustsimhub-core:x:986', 'rustsimhub-core:x:1001')},
            {**original, 'group': original['group'].replace('rustsimhub-ipc:x:987', 'elsewhere:x:987')},
        ]:
            with self.assertRaises(ValueError): module.inspect(self.material, data)

    def test_changed_bundle_is_not_reported_with_a_stale_mixture_of_hashes(self):
        actual = module.validate_policy
        def mutate_after_validation(data):
            actual(data)
            path = self.material/module.FILES[1]
            path.write_text(path.read_text().replace('User=rustsimhub-ble', 'User=root'))
        with patch.object(module, 'validate_policy', mutate_after_validation):
            with self.assertRaisesRegex(ValueError, 'changed during inspection'):
                module.inspect(self.material)

    def test_current_real_ap_nss_guard_accepts_isolated_role_but_rejects_general_ipc_membership(self):
        spec = importlib.util.spec_from_file_location('ble_test_real_ap_install', ROOT/'tools/system/setup_ap_install.py')
        actual = importlib.util.module_from_spec(spec); spec.loader.exec_module(actual)
        data = inventory()
        actual.identities(data['passwd'], data['group'])
        data['passwd'] += 'rustsimhub-ble:x:982:982::/nonexistent:/usr/sbin/nologin\n'
        data['group'] += 'rustsimhub-ble:x:982:\n'
        # Compatibility of this synthetic inventory is not an NSS reservation
        # or installation approval. The actual Root validator is unchanged.
        actual.identities(data['passwd'], data['group'])
        data['group'] = data['group'].replace('rustsimhub-forwarder\n', 'rustsimhub-forwarder,rustsimhub-ble\n')
        with self.assertRaisesRegex(ValueError, 'foreign IPC group member'):
            actual.identities(data['passwd'], data['group'])

    def test_oversize_symlink_hardlink_directory_and_fifo_material_are_refused(self):
        path = self.material/module.FILES[0]
        original = path.read_bytes()
        path.write_bytes(b'x'*(module.MAX_FILE+1))
        with self.assertRaises(ValueError): module.inspect(self.material)
        path.unlink(); path.mkdir()
        with self.assertRaises(ValueError): module.inspect(self.material)
        path.rmdir(); path.write_bytes(original)
        linked = self.material/'extra'
        try:
            os.link(path, linked)
        except OSError:
            if sys.platform.startswith('linux'): raise
        else:
            with self.assertRaises(ValueError): module.inspect(self.material)
            linked.unlink()
        if sys.platform.startswith('linux'):
            path.unlink(); path.symlink_to(MATERIAL/module.FILES[0])
            with self.assertRaises(ValueError): module.inspect(self.material)
            path.unlink(); os.mkfifo(path)
            with self.assertRaises(ValueError): module.inspect(self.material)

    def test_cli_has_no_enable_install_reload_or_arbitrary_output_option(self):
        result = subprocess.run([sys.executable, str(MATERIAL/'material_plan.py'), '--enable'],
                                capture_output=True, timeout=3)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'unrecognized arguments', result.stderr)


@unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('dbus-daemon') and shutil.which('dbus-send'),
                     'private Unix D-Bus fixture requires Linux dbus tools')
class PrivateBus(unittest.TestCase):
    def test_actual_private_bus_enforces_broker_method_and_interface_boundaries(self):
        import select
        import pwd
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            config = ET.Element('busconfig')
            ET.SubElement(config, 'type').text = 'session'
            ET.SubElement(config, 'listen').text = 'unix:path='+str(root/'bus')
            ET.SubElement(config, 'auth').text = 'EXTERNAL'
            default = ET.SubElement(config, 'policy', {'context': 'default'})
            ET.SubElement(default, 'allow', {'user': '*'})
            policy = copy.deepcopy(ET.fromstring((MATERIAL/module.FILES[3]).read_bytes())[0])
            # Fixture only: target the actual current low-privilege UID. No
            # username/account is created; all other rules remain unchanged.
            policy.set('user', pwd.getpwuid(os.geteuid()).pw_name)
            config.append(policy)
            cfg = root/'bus.conf'; cfg.write_bytes(ET.tostring(config))
            with tempfile.TemporaryFile() as errors:
                process = subprocess.Popen(['dbus-daemon', '--config-file='+str(cfg), '--nofork',
                                            '--print-address=1', '--nopidfile'], stdout=subprocess.PIPE, stderr=errors)
                try:
                    ready, _, _ = select.select([process.stdout], [], [], 3)
                    self.assertTrue(ready, 'private bus startup deadline')
                    address = process.stdout.readline().decode('utf8').strip()
                    if not address:
                        errors.seek(0); self.fail('private bus did not start: '+errors.read(4096).decode('utf8', 'replace'))
                    self.assertTrue(address.startswith('unix:path='+str(root/'bus')))
                    def call(member, args=()):
                        return subprocess.run(['dbus-send', '--bus='+address, '--print-reply', '--reply-timeout=1000',
                                               '--dest=org.freedesktop.DBus', '/org/freedesktop/DBus', member, *args],
                                              capture_output=True, timeout=3)
                    allowed = call('org.freedesktop.DBus.GetNameOwner', ['string:org.freedesktop.DBus'])
                    self.assertEqual(allowed.returncode, 0, allowed.stderr.decode('utf8', 'replace'))
                    for member, args in [
                        ('org.freedesktop.DBus.ListNames', []),
                        ('org.freedesktop.DBus.GetNameOwnerWrong', ['string:org.bluez']),
                        ('org.freedesktop.DBus.RequestName', ['string:org.rustsimhub.Unrelated', 'uint32:0']),
                        ('org.freedesktop.DBus.Properties.Set', ['string:any', 'string:any', 'variant:boolean:true']),
                    ]:
                        denied = call(member, args)
                        self.assertNotEqual(denied.returncode, 0)
                        self.assertIn(b'AccessDenied', denied.stderr, denied.stderr.decode('utf8', 'replace'))
                finally:
                    process.terminate()
                    try: process.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.wait(timeout=2)
                    if process.stdout: process.stdout.close()


@unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('systemd-analyze'),
                     'offline unit parsing requires Linux systemd-analyze')
class OfflineSystemd(unittest.TestCase):
    def test_actual_candidate_is_parsed_under_a_temporary_root_without_starting_it(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            units = root/'etc/systemd/system'; units.mkdir(parents=True)
            (root/'etc/os-release').write_text('ID=synthetic\nVERSION_ID=1\n')
            for name in ['sysinit.target', 'basic.target', 'shutdown.target']:
                (units/name).write_text('[Unit]\nDescription=Synthetic offline dependency\nDefaultDependencies=no\n')
            (root/'bin').mkdir(); (root/'bin/true').write_text('#!/bin/sh\nexit 0\n'); (root/'bin/true').chmod(0o755)
            for name in ['rustsimhub-core.service', 'bluetooth.service', 'dbus.service']:
                (units/name).write_text('[Unit]\nDescription=Synthetic offline dependency\nDefaultDependencies=no\n[Service]\nType=oneshot\nExecStart=/bin/true\n')
            executable = root/'opt/rustsimhub/selected/rustsimhub-ble'; executable.parent.mkdir(parents=True)
            executable.write_text('#!/bin/sh\nexit 1\n'); executable.chmod(0o755)  # Never executed.
            (units/'rustsimhub-ble.service').write_bytes((MATERIAL/module.FILES[1]).read_bytes())
            result = subprocess.run(['systemd-analyze', '--root='+str(root), '--man=no', 'verify', 'rustsimhub-ble.service'],
                                    capture_output=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr.decode('utf8', 'replace'))
            self.assertNotIn(b'Unknown', result.stderr)


if __name__ == '__main__':
    unittest.main()
