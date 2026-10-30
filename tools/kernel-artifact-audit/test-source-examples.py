#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""C17 fixed-source bytes and minimal existing-parser examples, not C18 audit.

Default prints a plan. --run executes five checks in an unprivileged Linux copy.
No artifact discovery, downloads, subprocesses, compilers, module or device access.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import platform
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / 'docs/kernel-artifact-audit'
REFERENCE = '6b5de5f3ca5ff706adacd8f25ed7f638546739a9'


def bounded_json(path, limit):
    with path.open('rb') as handle:
        raw = handle.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('Example metadata exceeds budget')
    return json.loads(raw)


class SourceExamples(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        index = bounded_json(DOCS / 'source-index.json', 32 * 1024)
        if index['reference'] != REFERENCE or len(index['files']) != 31:
            raise ValueError('Wrong fixed source inventory')
        paths = set()
        for row in index['files']:
            relative = PurePosixPath(row['path'])
            if (relative.is_absolute() or '..' in relative.parts
                    or '\\' in row['path'] or ':' in row['path']
                    or row['path'] in paths):
                raise ValueError('Invalid fixed source path')
            paths.add(row['path'])
            target = ROOT.joinpath(*relative.parts)
            if not target.resolve().is_relative_to(ROOT):
                raise ValueError('Fixed source escaped the repository')
            raw = target.read_bytes()
            if len(raw) != row['bytes'] or hashlib.sha256(raw).hexdigest() != row['sha256']:
                raise ValueError('Source bytes differ: ' + row['path'])
            blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
            if blob != row['blob']:
                raise ValueError('Source Git blob differs: ' + row['path'])
        if 'tools/audio_modules.py' not in paths:
            raise ValueError('Actual parser source must be pinned')
        cls.index = index
        fixture = bounded_json(DOCS / 'minimal-failures.json', 16 * 1024)
        if fixture['format'] != 1 or fixture['synthetic'] is not True:
            raise ValueError('Expected synthetic parser examples')
        if set(fixture['cases']) != {'valid', 'wrong_arch', 'section_outside', 'truncated'}:
            raise ValueError('Unexpected example set')
        cls.cases = fixture['cases']
        spec = importlib.util.spec_from_file_location('c17_existing_audio', ROOT / 'tools/audio_modules.py')
        cls.audio = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.audio)

    def test_fixed_31_public_source_files(self):
        self.assertEqual(len(self.index['files']), 31)
        self.assertFalse(self.index['downloaded_artifacts'])
        self.assertFalse(self.index['private_inputs_read'])

    def parse_case(self, name):
        row = self.cases[name]
        raw = bytes.fromhex(row['hex'])
        self.assertLessEqual(len(raw), 1024)
        self.assertEqual(hashlib.sha256(raw).hexdigest(), row['sha256'])
        return self.audio.modinfo(raw)

    def test_valid_synthetic_elf(self):
        self.assertEqual(self.parse_case('valid'), self.cases['valid']['expected'])

    def test_wrong_architecture_rejected(self):
        with self.assertRaisesRegex(ValueError, 'Expected ARM64 relocatable module'):
            self.parse_case('wrong_arch')

    def test_section_table_outside_rejected(self):
        with self.assertRaisesRegex(ValueError, 'Invalid ELF section table'):
            self.parse_case('section_outside')

    def test_truncated_table_rejected(self):
        with self.assertRaisesRegex(ValueError, 'Invalid ELF section table'):
            self.parse_case('truncated')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'execute': False, 'phase': 'C17 source/examples only',
                          'reference': REFERENCE, 'test_count': 5,
                          'artifact_checker_implemented': False,
                          'downloads': False, 'device_access': False}))
        return 0
    if platform.system() != 'Linux' or os.geteuid() == 0:
        parser.error('--run requires an unprivileged Linux source copy')
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(SourceExamples)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    print(json.dumps({'reference': REFERENCE, 'uid': os.geteuid(),
                      'tests_run': result.testsRun, 'failures': len(result.failures),
                      'errors': len(result.errors), 'skipped': len(result.skipped),
                      'passed': result.wasSuccessful() and result.testsRun == 5,
                      'artifact_checker_implemented': False,
                      'real_artifacts_verified': False}))
    return 0 if result.wasSuccessful() and result.testsRun == 5 else 1


if __name__ == '__main__':
    sys.dont_write_bytecode = True
    raise SystemExit(main())
