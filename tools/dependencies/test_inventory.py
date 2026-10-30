# SPDX-License-Identifier: AGPL-3.0-only
import copy
import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import inventory


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.source = 'registry+https://github.com/rust-lang/crates.io-index'
        self.lock = {'version': 4, 'package': [{'name': 'product', 'version': '1.0.0'},
                    {'name': 'direct', 'version': '1.0.0', 'source': self.source, 'checksum': 'a' * 64},
                    {'name': 'indirect', 'version': '2.0.0', 'source': self.source, 'checksum': 'b' * 64}]}
        self.metadata = {'workspace_members': ['product-id'],
                         'packages': [{'name': p['name'], 'version': p['version'],
                                       'source': p.get('source'), 'id': p['name'] + '-id',
                                       'license': 'MIT OR Apache-2.0'} for p in self.lock['package']],
                         'resolve': {'nodes': [{'id': 'product-id', 'dependencies': ['direct-id']}]}}

    def test_direct_transitive_and_package_license_declarations(self):
        rows = inventory.cargo_inventory(self.lock, self.metadata)
        self.assertEqual([r['relationship'] for r in rows], ['workspace', 'direct', 'transitive'])
        self.assertEqual(rows[1]['checksum'], 'a' * 64)
        self.assertEqual(rows[1]['license_declaration'], 'MIT OR Apache-2.0')
        self.assertNotIn(self.source, str(rows), 'No raw credential-bearing source URL is exported')

    def test_unknown_metadata_does_not_become_license_approval(self):
        rows = inventory.cargo_inventory(self.lock, None)
        self.assertTrue(all(r['license_declaration'] is None for r in rows))
        metadata = copy.deepcopy(self.metadata)
        metadata['packages'][1]['license'] = None
        self.assertIsNone(inventory.cargo_inventory(self.lock, metadata)[1]['license_declaration'])

    def test_bad_checksums_and_duplicate_locked_packages_are_rejected(self):
        for checksum in [None, 'not-a-sha', 'a' * 63]:
            lock = copy.deepcopy(self.lock); lock['package'][1]['checksum'] = checksum
            with self.assertRaises(ValueError): inventory.cargo_inventory(lock, self.metadata)
        lock = copy.deepcopy(self.lock); lock['package'].append(lock['package'][1])
        with self.assertRaises(ValueError): inventory.cargo_inventory(lock, self.metadata)

    def test_go_replacement_is_not_authenticated_by_an_official_checksum(self):
        mod = 'module example.test/product\nrequire (\nexample.test/sdk v1.2.3\nexample.test/indirect v2.0.0 // indirect\n)\nreplace example.test/sdk => ./third_party/sdk\n'
        sums = 'example.test/sdk v1.2.3 h1:' + 'A' * 43 + '=\nexample.test/sdk v1.2.3/go.mod h1:' + 'B' * 43 + '=\n'
        rows = inventory.go_inventory(mod, sums)
        sdk, indirect = rows['requirements']
        self.assertTrue(sdk['replacement']); self.assertIsNone(sdk['license_declaration'])
        self.assertTrue(indirect['indirect']); self.assertIsNone(indirect['checksum'])
        self.assertFalse(rows['replacements'][0]['verified'])
        with self.assertRaises(ValueError): inventory.go_inventory(mod, sums + 'example.test/sdk v1.2.3 h1:' + 'C' * 43 + '=\n')
        with self.assertRaises(ValueError): inventory.go_inventory(mod, 'bad checksum')

    def test_public_file_limits_links_and_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); target = root / 'Cargo.lock'; target.write_text('public')
            self.assertEqual(inventory.read_public(root, 'Cargo.lock'), b'public')
            (root / 'linked').symlink_to(target)
            for name in ['linked', '../Cargo.lock', '.']:
                with self.assertRaises(ValueError): inventory.read_public(root, name)
            target.write_bytes(b'x' * (inventory.MAX_BYTES + 1))
            with self.assertRaises(ValueError): inventory.read_public(root, 'Cargo.lock')

    def test_default_cli_does_not_even_invoke_inventory(self):
        with patch('sys.argv', ['inventory.py']), patch.object(inventory, 'inventory', side_effect=AssertionError('unexpected execution')), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(inventory.main(), 0)


if __name__ == '__main__':
    unittest.main()
