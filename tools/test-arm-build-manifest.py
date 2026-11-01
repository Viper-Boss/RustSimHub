#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Committed archive and export failure tests; synthetic ELF is never executed."""
import copy
import errno
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch
import arm_build_manifest as module

SOURCE = 'a' * 64
REVISION = 'b' * 40


def headers():
    data = bytearray(64)
    data[:7] = b'\x7fELF\x02\x01\x01'
    data[16], data[18], data[52] = 2, 183, 64
    return bytes(data)


def infos():
    values = {name: {'component': name, 'target_arch': 'aarch64', 'development_fixtures': False,
                     'source_archive_sha256': SOURCE, 'setup_ap_installation_contract': {'format': 1}}
              for name in module.COMPONENTS if name != 'update'}
    values['core']['database_schema'] = 6
    values['edge'].update(local_installation_preparation_supported=True, media_installation_preparation_supported=True)
    # The actual updater CLI has no component/arch/fixture fields. Architecture
    # still comes from its checked ELF; do not require or fabricate those fields.
    values['update'] = {'source_archive_sha256': SOURCE, 'database_schema': 6,
                        'forwarding_application_format_supported': True, 'boot_runtime_selection_supported': True}
    return values


@unittest.skipUnless(sys.platform == 'linux', 'POSIX workstation tooling')
class Export(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='rsh-build-manifest-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.programs = self.root / 'programs'
        self.programs.mkdir()
        for name in module.PROGRAMS:
            (self.programs / name).write_bytes(headers() + name.encode())
        (self.programs / 'build.json').write_text('{"private_path":"must-not-copy"}')
        self.toolchain = self.root / 'toolchain.json'
        self.toolchain.write_text('{"pinned":"synthetic unit fixture"}')
        self.output = self.root / 'out'
        self.infos = infos()

    def collect(self, *, cargo_output=False):
        def read(path, runner):
            self.assertEqual(runner, ('synthetic-unit-reader',))
            return copy.deepcopy(self.infos[path.name.removeprefix('rustsimhub-')])
        return module.collect(self.programs, self.output, REVISION, SOURCE, self.toolchain,
                              ('synthetic-unit-reader',), info_reader=read, version_reader=lambda _: ['2.41'],
                              cargo_output=cargo_output)

    def test_explicit_cargo_mode_snapshots_hardlinks_as_independent_files(self):
        original = self.programs / module.PROGRAMS[0]
        alias = self.programs / 'cargo-deps-copy'
        os.link(original, alias)
        with self.assertRaises(ValueError): self.collect()
        self.assertFalse(self.output.exists())
        value = self.collect(cargo_output=True)
        for name in module.PROGRAMS:
            self.assertEqual((self.output / name).stat().st_nlink, 1)
        alias.write_bytes(b'changed after export')
        exported = self.output / original.name
        self.assertEqual(hashlib.sha256(exported.read_bytes()).hexdigest(), value['artifact_sha256'][original.name])
        self.assertNotIn(alias.name, value['artifact_sha256'])

    def test_cargo_mode_still_rejects_symlink_fifo_and_oversize(self):
        path = self.programs / module.PROGRAMS[0]
        outside = self.root / 'outside'
        outside.write_bytes(path.read_bytes())
        for kind in ('symlink', 'fifo', 'oversize'):
            with self.subTest(kind=kind):
                path.unlink()
                if kind == 'symlink': path.symlink_to(outside)
                elif kind == 'fifo': os.mkfifo(path)
                else:
                    with path.open('wb') as stream: stream.truncate(module.LIMIT + 1)
                with self.assertRaises((ValueError, OSError)): self.collect(cargo_output=True)
                self.assertFalse(self.output.exists())
                path.unlink()
                path.write_bytes(outside.read_bytes())

    def test_all_twelve_programs_flat_legacy_fields_and_no_path_leak(self):
        self.infos['core']['private_path'] = '/private/must-not-copy'
        value = self.collect()
        self.assertEqual(set(value['artifact_sha256']), set(module.PROGRAMS))
        self.assertEqual(value['database_schema'], 6)
        self.assertTrue(value['forwarding_application_format_supported'])
        self.assertFalse(value['hardware_tested'])
        self.assertNotIn('must-not-copy', json.dumps(value))
        self.assertEqual(json.loads((self.output / 'build.json').read_text()), value)
        self.assertEqual(len((self.output / 'SHA256SUMS').read_text().splitlines()), 12)
        for name in module.PROGRAMS:
            self.assertEqual(hashlib.sha256((self.output / name).read_bytes()).hexdigest(), value['artifact_sha256'][name])
            self.assertEqual((self.output / name).stat().st_mode & 0o777, 0o555)

    def test_missing_new_roles_fail_before_output(self):
        for name in ('rustsimhub-setup-ap', 'rustsimhub-core-launcher', 'rustsimhub-mdns'):
            with self.subTest(name=name):
                path = self.programs / name
                original = path.read_bytes()
                path.unlink()
                with self.assertRaises(FileNotFoundError): self.collect()
                self.assertFalse(self.output.exists())
                path.write_bytes(original)

    def test_links_fifo_directory_and_wrong_machine_are_refused(self):
        path = self.programs / 'rustsimhub-core'
        original = path.read_bytes()
        outside = self.root / 'outside'
        outside.write_bytes(original)
        for kind in ('symlink', 'hardlink', 'fifo', 'directory', 'x86'):
            with self.subTest(kind=kind):
                path.unlink()
                if kind == 'symlink': path.symlink_to(outside)
                elif kind == 'hardlink': os.link(outside, path)
                elif kind == 'fifo': os.mkfifo(path)
                elif kind == 'directory': path.mkdir()
                else:
                    data = bytearray(original);data[18] = 62;path.write_bytes(data)
                with self.assertRaises((ValueError, OSError)): self.collect()
                self.assertFalse(self.output.exists())
                if path.is_dir(): path.rmdir()
                else: path.unlink()
                path.write_bytes(original)

    def test_bad_source_schema_capability_fixture_or_joint_contract_is_not_exported(self):
        for component, field, value in (
            ('edge', 'source_archive_sha256', 'c' * 64),
            ('core', 'database_schema', True), ('update', 'database_schema', 5),
            ('edge', 'local_installation_preparation_supported', 1),
            ('update', 'forwarding_application_format_supported', False),
            ('core', 'development_fixtures', True), ('core', 'component', 'edge'),
            ('edge', 'target_arch', 'x86_64'),
            ('setup-ap', 'setup_ap_installation_contract', {'format': True}),
        ):
            with self.subTest(component=component, field=field):
                self.infos = infos();self.infos[component][field] = value
                with self.assertRaises(ValueError): self.collect()
                self.assertFalse(self.output.exists())

    def test_existing_output_is_preserved(self):
        self.output.mkdir();sentinel = self.output / 'sentinel';sentinel.write_bytes(b'preserve')
        with self.assertRaises(ValueError): self.collect()
        self.assertEqual(sentinel.read_bytes(), b'preserve')

    def test_export_disk_failure_retains_marked_partial_not_a_ready_manifest(self):
        with patch.object(os, 'fsync', side_effect=OSError(errno.ENOSPC, 'synthetic full disk')):
            with self.assertRaises(OSError): self.collect()
        self.assertTrue((self.output / 'BUILD-MANIFEST.FAILED').is_file())
        self.assertFalse((self.output / 'build.json').exists())


@unittest.skipUnless(sys.platform == 'linux', 'real isolated Git archive fixtures')
class Source(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='rsh-source-manifest-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.repo = self.root / 'repo';self.repo.mkdir()
        def git(*args):
            env = dict(os.environ, GIT_AUTHOR_NAME='Synthetic', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                       GIT_COMMITTER_NAME='Synthetic', GIT_COMMITTER_EMAIL='fixture@example.invalid',
                       GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1')
            return subprocess.check_output(['git', *args], cwd=self.repo, env=env, stderr=subprocess.DEVNULL)
        self.git = git
        git('init', '-q', '-b', 'main')
        (self.repo / 'sub').mkdir();(self.repo / 'first').write_bytes(b'first')
        (self.repo / 'sub/有 空格').write_bytes(b'second')
        git('add', '.');git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'synthetic fixture')
        self.revision = git('rev-parse', 'HEAD').decode().strip()
        self.raw = git('archive', '--format=tar', 'HEAD')
        self.archive = self.root / 'source.tar'

    def verify(self, data, output=None):
        self.archive.write_bytes(data)
        return module.verify_source(self.archive, self.repo, self.revision, module.sha(data), output)

    def changed(self, action):
        output = io.BytesIO()
        with tarfile.open(fileobj=io.BytesIO(self.raw)) as source, tarfile.open(fileobj=output, mode='w') as target:
            for member in source:
                if action == 'missing' and member.name == 'first': continue
                data = source.extractfile(member).read() if member.isfile() else None
                if action == 'changed' and member.name == 'first': data = b'other';member.size = len(data)
                target.addfile(member, io.BytesIO(data) if data is not None else None)
                if action == 'duplicate' and member.name == 'first': target.addfile(member, io.BytesIO(data))
            if action in ('extra', 'escape', 'absolute', 'link'):
                name = {'extra': 'extra', 'escape': '../escape', 'absolute': '/absolute', 'link': 'link'}[action]
                member = tarfile.TarInfo(name)
                if action == 'link': member.type = tarfile.SYMTYPE;member.linkname = '/outside'
                else: member.size = 1
                target.addfile(member, None if action == 'link' else io.BytesIO(b'x'))
        return output.getvalue()

    def test_normal_git_archive_directories_unicode_and_extraction(self):
        destination = self.root / 'source'
        self.assertEqual(self.verify(self.raw, destination), 2)
        self.assertEqual((destination / 'sub/有 空格').read_bytes(), b'second')

    def test_partial_extra_duplicate_links_or_changed_archive_never_extract(self):
        for action in ('missing', 'extra', 'duplicate', 'link', 'changed', 'escape', 'absolute'):
            with self.subTest(action=action):
                destination = self.root / action
                with self.assertRaises(ValueError): self.verify(self.changed(action), destination)
                self.assertFalse(destination.exists())

    def test_existing_source_directory_is_not_overwritten(self):
        destination = self.root / 'source';destination.mkdir()
        sentinel = destination / 'sentinel';sentinel.write_bytes(b'preserve')
        with self.assertRaises(ValueError): self.verify(self.raw, destination)
        self.assertEqual(sentinel.read_bytes(), b'preserve')


@unittest.skipUnless(sys.platform == 'linux', 'POSIX bounded subprocess observer')
class Tools(unittest.TestCase):
    def test_success_and_stderr_exit_output_and_closed_pipe_deadlines(self):
        self.assertEqual(module.bounded_run([sys.executable, '-c', 'print("ok")']), b'ok\n')
        for code in ('import sys;sys.stderr.write("error")', 'raise SystemExit(2)',
                     'import os;os.write(1,b"x"*10000)',
                     'import os,time;os.close(1);os.close(2);time.sleep(10)'):
            with self.subTest(code=code):
                start = time.monotonic()
                with self.assertRaises(ValueError):
                    module.bounded_run([sys.executable, '-c', code], timeout=.5, limit=100)
                self.assertLess(time.monotonic() - start, 3)

    def test_metadata_duplicates_nonfinite_and_glibc_numeric_comparison(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'{} trailing'):
            with patch.object(module, 'bounded_run', return_value=raw):
                with self.assertRaises(ValueError): module.metadata(Path('/synthetic'), ())
        with patch.object(module, 'bounded_run', return_value=b'GLIBC_2.9 GLIBC_2.41'):
            self.assertEqual(module.versions(Path('/synthetic')), ['2.9', '2.41'])
        with patch.object(module, 'bounded_run', return_value=b'GLIBC_2.100'):
            with self.assertRaises(ValueError): module.versions(Path('/synthetic'))


if __name__ == '__main__':
    unittest.main()
