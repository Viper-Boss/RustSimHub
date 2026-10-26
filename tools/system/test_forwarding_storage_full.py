# SPDX-License-Identifier: AGPL-3.0-only
"""Admission tests for the development-only bounded filesystem harness."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'test-forwarding-storage-full.py'
spec = importlib.util.spec_from_file_location('forwarding_storage_full', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class StorageFullHarnessAdmission(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def manifest(self, rows):
        path = self.root / 'artifacts.jsonl'
        path.write_text(''.join(json.dumps(row) + '\n' for row in rows))
        return path

    def artifact(self, executable):
        return {'reason': 'compiler-artifact',
                'target': {'name': 'rustsimhub_forwarder', 'kind': ['lib']},
                'profile': {'test': True}, 'executable': str(executable)}

    def test_selects_only_one_actual_library_test_not_binary_or_regular_build(self):
        valid = self.artifact('/synthetic/forwarder-tests')
        binary = self.artifact('/synthetic/cli-test')
        binary['target']['kind'] = ['bin']
        regular = self.artifact('/synthetic/not-tests')
        regular['profile']['test'] = False
        path = self.manifest([{'reason': 'build-finished'}, binary, regular, valid])
        self.assertEqual(module.binary_from_manifest(path), Path('/synthetic/forwarder-tests'))

    def test_ambiguous_missing_or_oversized_manifest_is_refused(self):
        for rows in [[], [self.artifact('/a'), self.artifact('/b')]]:
            with self.assertRaises(ValueError):
                module.binary_from_manifest(self.manifest(rows))
        path = self.root / 'oversized'
        path.write_bytes(b' ' * (2 * 1024 * 1024 + 1))
        with self.assertRaises(ValueError):
            module.binary_from_manifest(path)

    def test_shell_and_directory_cannot_be_executed_by_the_root_harness(self):
        script = self.root / 'script'
        script.write_text('#!/bin/sh\nexit 0\n')
        script.chmod(0o755)
        for path in [script, self.root]:
            with self.assertRaises(ValueError):
                module.checked_binary(path)

    def test_writable_or_multiply_linked_executable_is_refused(self):
        binary = self.root / 'elf'
        shutil.copyfile('/bin/true', binary)
        binary.chmod(0o755)
        self.assertEqual(module.checked_binary(binary), binary.resolve())
        binary.chmod(0o775)
        with self.assertRaises(ValueError):
            module.checked_binary(binary)
        binary.chmod(0o755)
        (self.root / 'link').hardlink_to(binary)
        with self.assertRaises(ValueError):
            module.checked_binary(binary)

    def test_oversized_executable_is_refused_without_loading_it(self):
        binary = self.root / 'sparse-elf'
        with binary.open('wb') as file:
            file.write(b'\x7fELF')
            file.truncate(256 * 1024 * 1024 + 1)
        binary.chmod(0o755)
        with self.assertRaises(ValueError):
            module.checked_binary(binary)


if __name__ == '__main__':
    unittest.main()
