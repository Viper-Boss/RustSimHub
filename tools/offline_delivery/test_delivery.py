# SPDX-License-Identifier: AGPL-3.0-only
import copy
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest import mock
from tools.offline_delivery import common, evidence, initramfs, relay, rootfs


class Delivery(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.inputs = self.base / 'inputs'
        self.inputs.mkdir(mode=0o700)
        self.source = common.Directory(self.inputs)
        self.addCleanup(self.source.__exit__)

    def file(self, name, data, mode=0o644):
        p = self.inputs / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
        p.chmod(mode)
        return {'path': name, 'sha256': common.digest(data)}

    def root(self):
        elf = bytearray(64)
        elf[:7] = b'\x7fELF\x02\x01\x01'
        elf[16], elf[18], elf[52] = 2, 183, 64
        entries = []
        for role in rootfs.ROLES:
            entries.append({'role': role, **self.file(role, bytes(elf))})
        return {'schema': 1, 'board': 'ufi003-v02', 'arch': 'aarch64', 'contracts': rootfs.contracts(), 'files': entries}

    def init(self):
        entry = self.file('public-init', b'#!/bin/sh\n# synthetic, never executed\nexit 1\n')
        return {'schema': 1, 'files': [{'path': 'init', 'source': entry['path'], 'mode': '0755', 'sha256': entry['sha256']}]}

    def relay(self):
        return {'schema': 1, 'hostname': 'relay.example.invalid', 'tls_port': 5349, 'relay_min': 50000, 'relay_max': 50015, 'max_allocations': 4, 'lifetime_seconds': 300}

    def test_default_cli_never_reads_nonexistent_inputs(self):
        result = subprocess.run(['python3', '-m', 'tools.offline_delivery', 'rootfs', '--manifest', '/does/not/exist'], capture_output=True, check=True)
        self.assertFalse(json.loads(result.stdout)['reads_performed'])

    def test_json_duplicates_constants_unknown_fields(self):
        for data in (b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":Infinity}', b'[] []'):
            with self.subTest(data=data), self.assertRaises(common.Invalid): common.parse(data)
        with self.assertRaises(common.Invalid): common.exact({'unexpected': 1}, ('schema',))

    def test_relative_paths(self):
        for path in ('/etc/shadow', '../x', 'a/../x', 'a//b', 'x\\y', 'x:y', '.', 'a/./b', 'x\n'):
            with self.subTest(path=path), self.assertRaises(common.Invalid): common.relative(path)

    def test_safe_read_rejects_symlink_hardlink_fifo_and_permissions(self):
        self.file('regular', b'data')
        os.symlink('regular', self.inputs / 'sym')
        os.link(self.inputs / 'regular', self.inputs / 'hard')
        os.mkfifo(self.inputs / 'fifo')
        self.file('writable', b'data', 0o666)
        self.file('setuid', b'data', 0o4644)
        for path in ('regular', 'sym', 'hard', 'fifo', 'writable', 'setuid'):
            with self.subTest(path=path), self.assertRaises((common.Invalid, OSError)): self.source.read(path)

    def test_budget_checked_before_read(self):
        self.file('big', b'12345')
        with self.assertRaises(common.Invalid): self.source.read('big', 4)

    def test_rootfs_two_clean_outputs_reproducible_and_tampering_rejected(self):
        manifest = self.root()
        for output in ('one', 'two'): rootfs.compose(manifest, self.source, self.base / output)
        with common.Directory(self.base / 'one') as one, common.Directory(self.base / 'two') as two:
            self.assertTrue(rootfs.compare(one, two)['reproducible_material_bytes'])
            path = self.base / 'two/rootfs' / rootfs.ROLES['updater'][0]
            path.write_bytes(b'changed')
            with self.assertRaises(common.Invalid): rootfs.compare(one, two)
        self.assertFalse((self.base / 'one/INCOMPLETE').exists())

    def test_rootfs_version_contract_hash_role_arch_permissions_rejected(self):
        original = self.root()
        mutations = [lambda m: m.update(schema=2), lambda m: m.update(arch='x86_64'), lambda m: m.update(contracts={}), lambda m: m['files'][0].update(sha256='0'*64), lambda m: m['files'][0].update(role='owner-key'), lambda m: m['files'].append(m['files'][0]), lambda m: m['files'][0].update(destination='etc/shadow')]
        for mutate in mutations:
            m = copy.deepcopy(original)
            mutate(m)
            with self.assertRaises(common.Invalid): rootfs.validate(m, self.source)

    def test_rootfs_wrong_elf_header_rejected(self):
        m = self.root()
        self.file(m['files'][0]['path'], b'x'*64)
        m['files'][0]['sha256'] = common.digest(b'x'*64)
        with self.assertRaises(common.Invalid): rootfs.validate(m, self.source)

    def test_rootfs_reproduction_rejects_changed_executable_mode(self):
        m = self.root()
        for output in ('one', 'two'): rootfs.compose(m, self.source, self.base / output)
        path = self.base / 'two/rootfs' / rootfs.ROLES['updater'][0]
        path.chmod(0o644)
        with common.Directory(self.base / 'one') as one, common.Directory(self.base / 'two') as two:
            with self.assertRaises(common.Invalid): rootfs.compare(one, two)

    def test_exclusive_outputs_and_interruption_leave_incomplete(self):
        output = self.base / 'interrupted'
        with self.assertRaises(RuntimeError):
            with common.fresh_output(output) as d:
                d.write_new('first', b'bytes')
                raise RuntimeError('interruption')
        self.assertTrue((output / 'INCOMPLETE').exists())
        self.assertFalse((output / 'COMPLETED').exists())
        with self.assertRaises(FileExistsError):
            with common.fresh_output(output): pass

    def test_symlink_output_parent_rejected(self):
        os.symlink(self.inputs, self.base / 'alias')
        with self.assertRaises(OSError):
            with common.fresh_output(self.base / 'alias/out'): pass

    def test_interruption_between_complete_marker_and_journal_removal_is_rejected(self):
        relay.compose(self.relay(), self.base / 'relay')
        with common.Directory(self.base / 'relay') as output:
            output.write_new('INCOMPLETE', b'interrupted before journal removal\n')
            with self.assertRaises(common.Invalid): relay.validate(self.relay(), output)

    def test_initramfs_determinism_roundtrip_and_nested_directory(self):
        m = self.init()
        entry = self.file('config', b'public synthetic config')
        m['files'].append({'path': 'etc/example.conf', 'source': entry['path'], 'sha256': entry['sha256'], 'mode': '0644'})
        a, observations = initramfs.build(m, self.source)
        m['files'].reverse()
        b, _ = initramfs.build(m, self.source)
        self.assertEqual(a, b)
        self.assertEqual(observations, initramfs.inspect(a))
        report = initramfs.compose(m, self.source, self.base / 'ramfs')
        self.assertFalse(report['boot_verified'])
        self.assertFalse(report['kernel_cohort_verified'])

    def test_initramfs_bad_magic_truncation_tail_or_mode(self):
        a, _ = initramfs.build(self.init(), self.source)
        for broken in (b'bad'+a[3:], a[:-1], a+b'\0', a[:14]+b'0000a1ff'+a[22:]):
            with self.assertRaises(common.Invalid): initramfs.inspect(broken)

    def test_initramfs_duplicate_path_or_missing_init(self):
        m = self.init()
        m['files'].append(m['files'][0])
        with self.assertRaises(common.Invalid): initramfs.build(m, self.source)
        m = self.init(); m['files'][0]['path'] = 'not-init'
        with self.assertRaises(common.Invalid): initramfs.build(m, self.source)

    def test_initramfs_file_directory_collision(self):
        m = self.init()
        e = dict(m['files'][0]); e['path'] = 'init/config'; m['files'].append(e)
        with self.assertRaises(common.Invalid): initramfs.build(m, self.source)

    def test_kernel_cohort_missing_mismatched_never_marked_complete(self):
        m = self.init()
        with mock.patch.object(initramfs.audit, 'inspect', return_value=({'byte_checks_complete': False}, 3)):
            with self.assertRaises(common.Invalid): initramfs.compose(m, self.source, self.base / 'out', {}, self.source)
        self.assertFalse((self.base / 'out').exists())

    def test_existing_real_checker_synthetic_cohort_never_claims_pinned_kernel(self):
        import importlib.util
        path = common.ROOT / 'tools/kernel-artifact-audit/test_audit.py'
        spec = importlib.util.spec_from_file_location('kernel_fixtures', path)
        fixtures = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixtures)
        fixture = fixtures.ArtifactChecks(); fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        with common.audit.Directory(fixture.artifacts) as artifacts:
            report = initramfs.compose(self.init(), self.source, self.base / 'out', fixture.value, artifacts)
        self.assertTrue(report['kernel_byte_checks_complete'])
        self.assertEqual(report['kernel_source_trust'], 'synthetic_only')
        self.assertFalse(report['kernel_cohort_verified'])

    def test_cli_run_and_existing_output_rejection(self):
        manifest = self.root()
        path = self.base / 'manifest.json'; path.write_bytes(common.canonical(manifest))
        command = ['python3', '-m', 'tools.offline_delivery', 'rootfs', '--run', '--manifest', str(path), '--inputs', str(self.inputs), '--output', str(self.base / 'cli-out')]
        result = subprocess.run(command, capture_output=True, check=True)
        self.assertFalse(json.loads(result.stdout)['authorization_verified'])
        result = subprocess.run(command, capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, b'')
        self.assertNotIn(str(self.base).encode(), result.stderr)

    def test_relay_generate_check_and_no_secrets(self):
        cfg = self.relay()
        report = relay.compose(cfg, self.base / 'relay')
        self.assertFalse(report['configuration_complete'])
        with common.Directory(self.base / 'relay') as d:
            self.assertEqual(relay.validate(cfg,d), report)
            p = self.base / 'relay/turnserver.conf.template'
            p.write_bytes(p.read_bytes()+b'allow-loopback-peers\n')
            with self.assertRaises(common.Invalid): relay.validate(cfg,d)

    def test_relay_injection_unknown_keys_and_unbounded_budgets(self):
        mutations = [dict(hostname='relay.example\nallow-loopback-peers'), dict(hostname='192.168.1.1'), dict(hostname='-a.example'), dict(hostname='a..example'), dict(hostname='É.example'), dict(max_allocations=9), dict(lifetime_seconds=601), dict(relay_max=50032), dict(relay_max=50000), dict(tls_port=50001), dict(secret='forbidden'), dict(schema=True)]
        for change in mutations:
            cfg = self.relay(); cfg.update(change)
            with self.subTest(change=change), self.assertRaises(common.Invalid): relay.render(cfg)


class Evidence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        def run(*args): return subprocess.check_output(['git','-C',str(self.repo),*args],stderr=subprocess.DEVNULL)
        self.git = run
        run('init','-q')
        (self.repo/'docs').mkdir()
        (self.repo/'docs/public.md').write_bytes(b'public evidence\n')
        os.symlink('/etc/shadow',self.repo/'docs/link')
        run('add','docs')
        run('-c','user.name=fixture','-c','user.email=fixture@example.invalid','commit','-qm','synthetic')
        self.index = {'schema':1,'source_commit':run('rev-parse','HEAD').decode().strip(),'capabilities':[{'id':c,'level':'source','device_status':'unknown','sources':[{'path':'docs/public.md','sha256':common.digest(b'public evidence\n')}]} for c in evidence.CAPABILITIES]}

    def test_exact_git_bytes_verified_without_claiming_device_signoff(self):
        r=evidence.validate(self.index,self.repo)
        self.assertTrue(r['source_bytes_verified']);self.assertFalse(r['release_signoff'])

    def test_wrong_sha_missing_source_symlink_or_private_path_rejected(self):
        for change in ({'sha256':'0'*64},{'path':'docs/missing.md'},{'path':'docs/link'},{'path':'.private/secret'},{'path':'docs/../public.md'},{'path':'docs/*'}):
            m=copy.deepcopy(self.index);m['capabilities'][0]['sources'][0].update(change)
            with self.subTest(change=change),self.assertRaises(common.Invalid):evidence.validate(m,self.repo)

    def test_missing_duplicate_capability_and_false_device_claim_rejected(self):
        for i in range(4):
            m=copy.deepcopy(self.index)
            if i==0:m['capabilities'].pop()
            elif i==1:m['capabilities'][0]=m['capabilities'][1]
            elif i==2:m['capabilities'][0]['device_status']='verified'
            else:m['capabilities'][0]['sources']=[]
            with self.assertRaises(common.Invalid):evidence.validate(m,self.repo)


if __name__ == '__main__': unittest.main()
