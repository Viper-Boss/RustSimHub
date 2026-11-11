# SPDX-License-Identifier: AGPL-3.0-only
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import os
import xml.etree.ElementTree as ET
import subprocess
import sys

spec = importlib.util.spec_from_file_location('product_release', Path(__file__).with_name('release.py'))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def roles():
    users = ['root:x:0:0:root:/root:/bin/sh']
    groups = ['root:x:0:', 'audio:x:29:rustsimhub-media',
              'rustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder']
    for role, uid in {**release.BASE_ROLES, 'ble': 982, 'nkn': 981}.items():
        name = 'rustsimhub-' + role
        users.append(f'{name}:x:{uid}:{uid}:fixture:/nonexistent:/usr/sbin/nologin')
        groups.append(f'{name}:x:{uid}:')
    return '\n'.join(users) + '\n', '\n'.join(groups) + '\n'


def infos(email=False):
    runtime = {'format': 1, 'worker_protocol': 5 if email else 4,
        'outbox': {'application_id': 0x52534657, 'read_format': 4 if email else 3,
                   'write_format': 4 if email else 3, 'migrate_from': [3] if email else [1, 2]}}
    core = {'component': 'core', 'target_arch': 'aarch64', 'development_fixtures': False,
            'source_archive_sha256': 'a'*64, 'database_schema': 6, 'forwarding_runtime_contract': runtime}
    worker = {'worker': True, 'protocol': 5 if email else 4, 'development_fixtures': False,
              'source_archive_sha256': 'a'*64, 'forwarding_runtime_contract': copy.deepcopy(runtime)}
    return core, worker


class ProductRelease(unittest.TestCase):
    def test_profiles_are_finite_and_health_interface_not_falsely_advertised(self):
        self.assertEqual(len(release.PROFILES), 6)
        for value in ['', 'shell', 'd-recovery-v1', 'product-health-v1', True, {}]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                release.profile(value)

    def test_default_and_all_opt_in_prefixes(self):
        for name, (product, _, recovery) in release.PROFILES.items():
            value = release.prefixes(name)
            self.assertEqual('--enable-update-health-candidate' in value, name=='product-recovery-health-v1')
            self.assertEqual(len(value), int(product) + int(recovery) + int(name=='product-recovery-health-v1'))
            self.assertEqual('--enable-secure-recovery-candidate' in value, recovery)
        self.assertEqual(release.prefixes('legacy-v1'), [])

    def test_real_old_ap_role_guard_accepts_unchanged_base_and_audio(self):
        passwd, group = roles()
        result = release.role_plan(passwd, group, 'legacy-v1')
        self.assertEqual(set(result), set(release.BASE_ROLES))

    def test_optional_roles_use_actual_snapshot_not_reserved_values(self):
        passwd, group = roles()
        passwd = passwd.replace(':982:982:', ':2982:2982:').replace(':981:981:', ':2981:2981:')
        group = group.replace(':982:', ':2982:').replace(':981:', ':2981:')
        result = release.role_plan(passwd, group, 'product-v1')
        self.assertEqual(result['ble'], {'uid': 2982, 'gid': 2982})
        self.assertEqual(result['nkn']['uid'], 2981)

    def test_optional_roles_cannot_join_old_ap_ipc_or_audio(self):
        passwd, group = roles()
        for changed in [group.replace('rustsimhub-forwarder\n', 'rustsimhub-forwarder,rustsimhub-nkn\n'),
                        group.replace('audio:x:29:rustsimhub-media', 'audio:x:29:rustsimhub-media,rustsimhub-ble')]:
            with self.assertRaises(ValueError):
                release.role_plan(passwd, changed, 'product-v1')

    def test_missing_duplicate_alias_and_foreign_members_refused(self):
        passwd, group = roles()
        cases = [(passwd.replace('rustsimhub-ble:', 'missing-ble:'), group),
                 (passwd + 'alias:x:981:500:fixture:/:/bin/false\n', group),
                 (passwd, group + 'alias:x:981:\n'),
                 (passwd, group.replace('rustsimhub-nkn:x:981:', 'rustsimhub-nkn:x:981:root')),
                 (passwd + 'foreign:x:4000:981:fixture:/:/bin/false\n', group)]
        for user, grp in cases:
            with self.subTest(user=user[-40:], group=grp[-40:]), self.assertRaises(ValueError):
                release.role_plan(user, grp, 'product-v1')

    def test_legacy_has_no_optional_role_requirement(self):
        passwd, group = roles()
        passwd = '\n'.join(line for line in passwd.splitlines() if 'rustsimhub-nkn' not in line and 'rustsimhub-ble' not in line)
        group = '\n'.join(line for line in group.splitlines() if 'rustsimhub-nkn' not in line and 'rustsimhub-ble' not in line)
        release.role_plan(passwd, group, 'legacy-v1')
        with self.assertRaises(ValueError):
            release.role_plan(passwd, group, 'product-v1')

    def test_actual_core_worker_contract_for_every_profile(self):
        for name, (_, email, _) in release.PROFILES.items():
            core, worker = infos(email)
            self.assertEqual(release.contract(core, worker, name, 'a'*64), core['forwarding_runtime_contract'])

    def test_mixed_source_schema_worker_protocol_and_fixture_refused(self):
        for location, field, bad in [('core', 'source_archive_sha256', 'b'*64), ('core', 'database_schema', 9),
                                     ('core', 'database_schema', True), ('core', 'development_fixtures', True),
                                     ('worker', 'protocol', 5), ('worker', 'source_archive_sha256', 'c'*64),
                                     ('worker', 'development_fixtures', True)]:
            core, worker = infos()
            (core if location == 'core' else worker)[field] = bad
            with self.subTest(field=field, value=bad), self.assertRaises(ValueError):
                release.contract(core, worker, 'product-v1', 'a'*64)

    def test_runtime_boolean_and_email_downgrade_not_numeric_equivalence(self):
        core, worker = infos()
        core['forwarding_runtime_contract']['format'] = True
        with self.assertRaises(ValueError):
            release.contract(core, worker, 'product-v1', 'a'*64)
        for email, name in [(True, 'product-v1'), (False, 'product-email-v1')]:
            with self.assertRaises(ValueError):
                release.contract(*infos(email), name, 'a'*64)

    def test_duplicate_and_nonfinite_json_refused(self):
        for raw in [b'{"x":1,"x":2}', b'{"x":NaN}', b'', b' '*131073]:
            with self.assertRaises(ValueError):
                release.strict_json(raw)

    def test_materials_bind_exact_source_bytes_and_no_implicit_enablement(self):
        passwd, group = roles()
        for name in release.PROFILES:
            with tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                account = release.role_plan(passwd, group, name)
                config, plan = release.render(root, name, 'a'*64, {'rustsimhub-core': 'b'*64}, account)
                self.assertEqual(config['core_sha256'], 'b'*64)
                self.assertEqual(config['source_archive_sha256'], 'a'*64)
                self.assertFalse(plan['installation_permitted'])
                self.assertFalse(plan['starts_services'])
                self.assertFalse(plan['forwarding_enabled'])
                self.assertFalse(any(plan['nkn_references'].values()))
                self.assertFalse(plan['ble_enabled'])
                for file in root.iterdir():
                    self.assertNotIn(b'@', file.read_bytes())
                    if file.suffix == '.service':
                        self.assertNotIn('[Install]', file.read_text())
                        self.assertIn('Restart=no', file.read_text())
                self.assertEqual((root/'rustsimhub-ble-recovery.service').exists(), release.profile(name)[2])
                if name == 'legacy-v1':
                    self.assertIn('MemoryMax=96M', (root/'50-product-profile.conf').read_text())
                    self.assertEqual(sorted(p.name for p in root.iterdir()), ['50-product-profile.conf', 'product-profile.json'])

    def test_exclusive_output_never_overwrites_existing_material(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'material'
            release.write_new(path, b'original')
            with self.assertRaises(FileExistsError):
                release.write_new(path, b'replaced')
            self.assertEqual(path.read_bytes(), b'original')

    @unittest.skipUnless(os.name == 'posix', 'Linux source admission')
    def test_verified_empty_go_sum_is_source_not_an_executable(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'go.sum'
            path.touch()
            self.assertEqual(release.managed_stage.source_file(path), b'')
            with self.assertRaises(ValueError):
                release.arm.regular(path)
            link = Path(temp)/'link'
            link.symlink_to(path)
            with self.assertRaises(OSError):
                release.managed_stage.source_file(link)

    def test_product_bus_policy_reuses_fixed_rules_and_only_adds_actual_backend_read(self):
        material = release.ROOT/'firmware/system'
        original = ET.fromstring((material/'ble/rustsimhub-ble.policy.in').read_bytes())
        actual = ET.fromstring((material/'product-profiles/ble-product.policy').read_bytes())
        before = [(v.tag, v.attrib) for v in original[0]]
        after = [(v.tag, v.attrib) for v in actual[0]]
        addition = ('allow', {'send_destination': 'org.bluez', 'send_interface': 'org.freedesktop.DBus.ObjectManager',
            'send_member': 'GetManagedObjects', 'send_type': 'method_call', 'max_fds': '0', 'send_path': '/'})
        self.assertEqual(actual[0].attrib, {'user': 'rustsimhub-ble'})
        self.assertEqual(after, [*before[:4], addition, *before[4:]])

    def test_optimized_python_cannot_disable_inherited_sdk_verification(self):
        script = "import release\ntry: release.build(None)\nexcept ValueError as e:\n if 'optimization' not in str(e): raise RuntimeError('wrong rejection')\nelse: raise RuntimeError('accepted optimized verification')"
        result = subprocess.run([sys.executable, '-O', '-c', script], cwd=Path(__file__).parent,
                                capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def test_architecture_can_omit_old_module_but_cannot_add_unreviewed_module(self):
        own = 'this-project'
        baseline = {'crypto', 'sys'}
        self.assertEqual(release.managed_stage.module_closure({own, 'crypto'}, baseline, own), ['sys'])
        self.assertEqual(release.managed_stage.module_closure({own, 'crypto', 'sys'}, baseline, own), [])
        for actual in [{own, 'crypto', 'unreviewed'}, {'crypto'}]:
            with self.assertRaises(ValueError):
                release.managed_stage.module_closure(actual, baseline, own)

    @unittest.skipUnless(hasattr(os, 'waitid'), 'Linux owned build process groups')
    def test_build_failure_timeout_and_output_cap_are_not_success(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            release.run_build(['/bin/sh', '-c', 'printf built'], root, os.environ.copy(), root/'ok.log')
            with self.assertRaises(ValueError):
                release.run_build(['/bin/sh', '-c', 'exit 7'], root, os.environ.copy(), root/'fail.log')
            with self.assertRaises(ValueError):
                release.run_build(['/bin/sh', '-c', 'sleep 20 & echo child=$!; wait'], root,
                                  os.environ.copy(), root/'timeout.log', timeout=.15)
            raw = (root/'timeout.log').read_text()
            child = int(raw.split('child=')[-1].strip())
            stat = Path('/proc')/str(child)/'stat'
            self.assertTrue(not stat.exists() or stat.read_text().split(') ', 1)[1].startswith('Z'))
            with self.assertRaises(ValueError):
                release.run_build(['/bin/sh', '-c', 'printf %0200d 0'], root, os.environ.copy(),
                                  root/'large.log', max_bytes=128)


if __name__ == '__main__':
    unittest.main()
