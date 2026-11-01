# SPDX-License-Identifier: AGPL-3.0-only
"""Synthetic isolation tests for the actual C18 checker, never device evidence."""
from contextlib import redirect_stdout
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('c18_audit', HERE/'audit.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def dtb(compatible):
    payload = b'\0'.join(s.encode() for s in compatible)+b'\0'
    block = struct.pack('>I', 1)+bytes(4)
    block += struct.pack('>III', 3, len(payload), 0)+payload
    block += bytes((-len(block)) % 4)
    block += struct.pack('>II', 2, 9)
    strings = b'compatible\0'
    offset = 56
    header = struct.pack('>10I', 0xd00dfeed, offset+len(block)+len(strings),
                         offset, offset+len(block), 40, 17, 16, 0, len(strings), len(block))
    return header+bytes(16)+block+strings


class ArtifactChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.artifacts = self.base/'input'
        self.artifacts.mkdir()
        self.manifest = self.base/'manifest.json'
        self.value = dict(schema=1, mode='synthetic', kernel_release='synthetic-release',
                          board_compatible=['synthetic,board'], entries=[])
        image = bytearray(64)
        struct.pack_into('<Q', image, 16, 64)
        image[56:60] = b'ARMd'
        self.add('image', 'kernel_image', bytes(image), {})
        self.add('dtb', 'dtb', dtb(['synthetic,board']), {})
        vectors = json.loads((audit.ROOT/'docs/kernel-artifact-audit/minimal-failures.json').read_text())
        raw = bytearray.fromhex(vectors['cases']['valid']['hex'])
        raw[6] = 1
        info = dict(vectors['cases']['valid']['expected'])
        self.value['kernel_release'] = info['vermagic']
        self.add('module', 'module', bytes(raw), dict(name=info['name'],
                 vermagic=info['vermagic'], depends=[], srcversion=info['srcversion']))
        self.add('firmware', 'firmware', b'synthetic-firmware', {'license':'synthetic-test-only'})

    def add(self, name, kind, data, meta):
        (self.artifacts/name).write_bytes(data)
        self.value['entries'].append(dict(id=name, kind=kind, path=name, bytes=len(data),
             sha256=hashlib.sha256(data).hexdigest(), metadata=meta))

    def run_cli(self, extra=(), raw=None):
        self.manifest.write_text(json.dumps(self.value) if raw is None else raw)
        output = io.StringIO()
        with redirect_stdout(output):
            code = audit.main(['--run', '--manifest', str(self.manifest),
                               '--artifacts', str(self.artifacts), *extra])
        result = json.loads(output.getvalue())
        self.assertNotIn(str(self.base), output.getvalue())
        return code, result

    def mutate_bytes(self, name, data, update_hash=True):
        (self.artifacts/name).write_bytes(data)
        row = next(r for r in self.value['entries'] if r['id'] == name)
        if update_hash:
            row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())

    def test_matching_synthetic_set_is_observed_without_release_authority(self):
        code, result = self.run_cli()
        self.assertEqual(code, 0)
        self.assertTrue(result['byte_checks_complete'])
        self.assertEqual({r['status'] for r in result['observations']}, {'observed'})
        self.assertEqual(result['source_trust'], 'synthetic_only')
        self.assertFalse(result['authorization_verified'])
        self.assertEqual(result['firmware_license'], 'not_verified')

    def test_default_plan_reads_no_inputs_or_outputs(self):
        output = io.StringIO()
        with redirect_stdout(output):
            code = audit.main(['--manifest', '/dev/not-read', '--artifacts', '/dev',
                               '--report', str(self.base/'not-created')])
        self.assertEqual(code, 0)
        self.assertFalse(json.loads(output.getvalue())['execute'])
        self.assertFalse((self.base/'not-created').exists())

    def test_missing_and_empty_required_evidence_stays_incomplete(self):
        (self.artifacts/'module').unlink()
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertIn('missing_file', result['observations'][2]['reasons'])
        self.value['entries'] = []
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertEqual(len(result['gaps']), 4)

    def test_digest_corruption_is_mismatch_and_inputs_are_retained(self):
        self.mutate_bytes('firmware', b'corrupted', update_hash=False)
        self.assertEqual(self.run_cli()[0], 4)
        self.assertEqual((self.artifacts/'firmware').read_bytes(), b'corrupted')

    def test_wrong_board_with_self_consistent_hash_is_rejected(self):
        self.mutate_bytes('dtb', dtb(['synthetic,other']))
        code, result = self.run_cli()
        self.assertEqual(code, 4)
        self.assertIn('board_compatible', result['observations'][1]['reasons'])

    def test_malformed_fdt_and_image_headers_are_mismatches(self):
        for name, data in [('dtb', b'invalid-fdt'), ('image', bytes(64))]:
            with self.subTest(name=name):
                self.mutate_bytes(name, data)
                self.assertEqual(self.run_cli()[0], 4)

    def test_wrong_elf_architecture_and_release_are_rejected(self):
        original = (self.artifacts/'module').read_bytes()
        wrong = bytearray(original)
        struct.pack_into('<H', wrong, 18, 62)
        self.mutate_bytes('module', bytes(wrong))
        self.assertEqual(self.run_cli()[0], 4)
        self.mutate_bytes('module', original)
        self.value['kernel_release'] = 'other-release'
        code, result = self.run_cli()
        self.assertEqual(code, 4)
        self.assertIn('release', result['observations'][2]['reasons'])

    def test_unknown_version_keys_types_and_duplicate_json_are_invalid(self):
        good = copy.deepcopy(self.value)
        for key, value in [('schema', 2), ('schema', True), ('mode', 'unknown'),
                           ('healthy', True), ('entries', {}), ('board_compatible', [])]:
            with self.subTest(key=key, value=value):
                self.value = copy.deepcopy(good)
                self.value[key] = value
                self.assertEqual(self.run_cli()[0], 1)
        self.value = good
        raw = json.dumps(good).replace('"schema": 1', '"schema": 1, "schema": 1')
        self.assertEqual(self.run_cli(raw=raw)[0], 1)
        self.assertEqual(self.run_cli(raw='{"schema":NaN}')[0], 1)

    def test_path_escapes_aliases_and_duplicate_entries_are_invalid(self):
        original = copy.deepcopy(self.value)
        for path in ['../outside', '/absolute', 'a/../x', 'a//b', 'C:x', 'a\\b']:
            self.value = copy.deepcopy(original)
            self.value['entries'][0]['path'] = path
            self.assertEqual(self.run_cli()[0], 1)
        for key in ['id', 'path']:
            self.value = copy.deepcopy(original)
            self.value['entries'][1][key] = self.value['entries'][0][key]
            self.assertEqual(self.run_cli()[0], 1)
        self.value = copy.deepcopy(original)
        self.value['entries'][1]['path'] = 'IMAGE'
        self.assertEqual(self.run_cli()[0], 1)

    def test_symlink_hardlink_fifo_and_parent_links_are_not_read(self):
        target = self.artifacts/'firmware'
        target.unlink()
        outside = self.base/'outside'
        outside.write_bytes(b'preserve')
        target.symlink_to(outside)
        self.assertEqual(self.run_cli()[0], 1)
        target.unlink()
        os.link(outside, target)
        self.assertEqual(self.run_cli()[0], 1)
        target.unlink()
        os.mkfifo(target)
        self.assertEqual(self.run_cli()[0], 1)
        target.unlink()
        (self.artifacts/'alias').symlink_to(self.base, target_is_directory=True)
        self.value['entries'][3]['path'] = 'alias/outside'
        self.assertEqual(self.run_cli()[0], 1)
        self.assertEqual(outside.read_bytes(), b'preserve')

    def test_file_entry_and_manifest_budgets_are_enforced(self):
        with (self.artifacts/'firmware').open('wb') as handle:
            handle.truncate(audit.LIMITS['firmware']+1)
        self.assertEqual(self.run_cli()[0], 1)
        self.assertEqual(self.run_cli(raw=' '*((1024*1024)+1))[0], 1)
        self.value['entries'] = [self.value['entries'][0]]*2049
        self.assertEqual(self.run_cli()[0], 1)

    def test_module_dependency_gap_is_not_silently_accepted(self):
        # The actual existing parser observes depends bytes, not manifest assertions.
        original = (self.artifacts/'module').read_bytes()
        raw = original.replace(b'name=synthetic\0', b'name=synthetid\0')
        self.mutate_bytes('module', raw)
        self.assertEqual(self.run_cli()[0], 4)
        # Exercise the real graph on a correctly re-encoded dependent module.
        info = self.value['entries'][2]['metadata']
        raw = module_elf(info['name'], info['vermagic'], 'missing', info['srcversion'])
        self.mutate_bytes('module', raw)
        info['depends'] = ['missing']
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertEqual(result['unresolved_module_dependencies'], ['missing'])

    def test_module_cycles_and_alias_conflicts_are_rejected(self):
        info = self.value['entries'][2]['metadata']
        self.mutate_bytes('module', module_elf(info['name'], info['vermagic'], info['name'], info['srcversion']))
        info['depends'] = [info['name']]
        code, result = self.run_cli()
        self.assertEqual(code, 4)
        self.assertTrue(result['dependency_cycle'])
        self.value['entries'].append(copy.deepcopy(self.value['entries'][2]))
        self.value['entries'][-1].update(id='other', path='other')
        self.assertEqual(self.run_cli()[0], 1)

    def test_missing_firmware_license_is_explicit(self):
        self.value['entries'][3]['metadata']['license'] = None
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertEqual(result['observations'][3]['status'], 'not_verified')

    def test_complete_dependency_chain_normalizes_module_names(self):
        info = self.value['entries'][2]['metadata']
        self.mutate_bytes('module', module_elf(info['name'], info['vermagic'], 'leaf-node', info['srcversion']))
        info['depends'] = ['leaf-node']
        self.add('leaf', 'module', module_elf('leaf_node', info['vermagic'], '', 'B'*24),
                 dict(name='leaf_node', vermagic=info['vermagic'], depends=[], srcversion='B'*24))
        code, result = self.run_cli()
        self.assertEqual(code, 0)
        self.assertEqual(result['unresolved_module_dependencies'], [])

    def test_fdt_overlaps_versions_and_unbalanced_structure_are_rejected(self):
        original = dtb(['synthetic,board'])
        for offset, value in [(12, 40), (24, 0), (56, 2), (8, 0xffffffff)]:
            raw = bytearray(original)
            struct.pack_into('>I', raw, offset, value)
            self.mutate_bytes('dtb', bytes(raw))
            self.assertEqual(self.run_cli()[0], 4)

    def test_image_memory_extent_is_not_confused_with_file_length(self):
        raw = bytearray((self.artifacts/'image').read_bytes())
        struct.pack_into('<Q', raw, 16, 4096)
        self.mutate_bytes('image', bytes(raw))
        self.assertEqual(self.run_cli()[0], 0)
        struct.pack_into('<Q', raw, 16, audit.LIMITS['kernel_image']+1)
        self.mutate_bytes('image', bytes(raw))
        self.assertEqual(self.run_cli()[0], 4)

    def test_invalid_reason_is_finite_and_directory_alias_is_refused(self):
        self.value['schema'] = 99
        self.assertEqual(self.run_cli()[1]['reason'], 'schema')
        self.value['schema'] = 1
        alias = self.base/'alias'
        alias.symlink_to(self.artifacts, target_is_directory=True)
        self.artifacts = alias
        self.assertEqual(self.run_cli()[0], 1)

    def test_report_is_new_and_cannot_overwrite_inputs(self):
        report = self.base/'report.json'
        code, result = self.run_cli(['--report', str(report)])
        self.assertEqual(code, 0)
        previous = report.read_bytes()
        self.assertEqual(json.loads(previous), result)
        self.assertEqual(self.run_cli(['--report', str(report)])[0], 1)
        self.assertEqual(report.read_bytes(), previous)
        self.assertEqual(self.run_cli(['--report', str(self.artifacts/'new')])[0], 1)
        self.assertFalse((self.artifacts/'new').exists())
        self.assertEqual(self.run_cli(['--report', str(self.manifest)])[0], 1)

    def test_pinned_mode_uses_existing_locks_and_retains_real_artifact_gaps(self):
        self.value = dict(schema=1, mode='pinned', source_commit=audit.REFERENCE, mappings=[])
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertEqual(len(result['observations']), 24)
        self.assertIn('stock_modules_not_verified', result['gaps'])
        self.value['source_commit'] = '0'*40
        self.assertEqual(self.run_cli()[0], 1)
        self.value['source_commit'] = audit.REFERENCE
        self.value['mappings'] = [{'id':'firmware', 'path':'firmware'}]
        self.assertEqual(self.run_cli()[0], 1)

    def test_public_mapping_example_and_cumulative_budget(self):
        original = self.value
        self.value = json.loads((audit.ROOT/'docs/kernel-artifact-audit/pinned-input.json').read_text())
        code, result = self.run_cli()
        self.assertEqual(code, 3)
        self.assertEqual(len(result['observations']), 24)
        self.value = original
        with patch.object(audit, 'TOTAL', 64):
            self.assertEqual(self.run_cli()[0], 1)


def module_elf(name, release, depends, version):
    # Existing project synthetic layout, no executable code or hardware behavior.
    strings = b'\0.shstrtab\0.modinfo\0'
    payload = ('name='+name+'\0vermagic='+release+'\0depends='+depends+'\0srcversion='+version+'\0').encode()
    header = bytearray(64)
    header[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HH', header, 16, 1, 183)
    struct.pack_into('<Q', header, 40, 64+len(strings)+len(payload))
    struct.pack_into('<HHH', header, 58, 64, 3, 1)
    sections = bytes(64)+struct.pack('<IIQQQQIIQQ', 1,3,0,0,64,len(strings),0,0,1,0)
    sections += struct.pack('<IIQQQQIIQQ', 11,1,0,0,64+len(strings),len(payload),0,0,1,0)
    return bytes(header)+strings+payload+sections


if __name__ == '__main__':
    unittest.main(verbosity=2)
