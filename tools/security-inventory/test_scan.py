# SPDX-License-Identifier: AGPL-3.0-only
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch
import scan


class ScanTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def file(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)
        return path

    def test_all_categories_detected_without_values_or_absolute_paths(self):
        token = 'ghp_' + 'SYNTHETIC_ONLY_' + 'A' * 28
        password = 'SYNTHETIC_PASSWORD_123456'
        body = 'SYNTHETIC_PRIVATE_MESSAGE'
        lines = ['-----BEGIN PRIVATE KEY-----', token, f'password="{password}"',
                 'psk=SYNTHETIC_WIFI_PASSWORD',
                 'recipient="5550001234"', f'sms_body="{body}"',
                 'eyJ' + 'A' * 16 + '.' + 'B' * 16 + '.' + 'C' * 32]
        self.file('release.txt', '\n'.join(lines))
        before = (self.root / 'release.txt').read_bytes()
        report = scan.scan(self.root, ['release.txt'])
        self.assertEqual(report['status'], 'findings')
        self.assertEqual({f['rule'] for f in report['findings']}, {name for name, _ in scan.RULES})
        encoded = json.dumps(report)
        for secret in [token, password, body, '5550001234', '-----BEGIN PRIVATE KEY-----', str(self.root)]:
            self.assertNotIn(secret, encoded)
        self.assertEqual((self.root / 'release.txt').read_bytes(), before)
        self.assertEqual([f['line'] for f in report['findings']], list(range(1, len(lines) + 1)))

    def test_public_keys_digests_references_and_example_fields_are_not_secrets(self):
        self.file('public.json', json.dumps({'public_key': 'a' * 64, 'sha256': 'b' * 64,
                  'password': '<redacted>', 'token': '${TOKEN_FROM_ENV}', 'passphrase': '',
                  'message_schema': 'example metadata, no communication body', 'year': 2026}))
        self.file('public.nmconnection', 'psk="${WIFI_PASSWORD}"\nwifi_password="<redacted>"\n')
        report = scan.scan(self.root, ['public.json', 'public.nmconnection'])
        self.assertEqual(report['status'], 'no_findings_in_scope')
        self.assertEqual(report['findings'], [])

    def test_benign_reference_does_not_hide_a_second_literal_on_same_line(self):
        self.file('mixed.json', '{"password":"<redacted>","token":"SYNTHETIC_REAL_LITERAL"}')
        report = scan.scan(self.root, ['mixed.json'])
        self.assertEqual(report['status'], 'findings')
        self.assertEqual(report['findings'][0]['rule'], 'secret_literal')

    def test_hit_in_filename_is_redacted_too(self):
        name = 'ghp_' + 'SYNTHETIC_FILENAME_' + 'A' * 24 + '.txt'
        self.file(name, 'public')
        report = scan.scan(self.root, [name])
        self.assertTrue(report['scanned'][0]['path'].startswith('redacted-path-'))
        self.assertNotIn(name, json.dumps(report))

    def test_literal_or_phone_used_as_a_filename_does_not_escape_redaction(self):
        for value, text in [('SYNTHETIC_PASSWORD_123456', 'password="SYNTHETIC_PASSWORD_123456"'),
                            ('SYNTHETIC_WIFI_12345', 'psk="SYNTHETIC_WIFI_12345"'),
                            ('5550001234', 'recipient="5550001234"'),
                            ('SYNTHETIC_PRIVATE_MESSAGE', 'sms_body="SYNTHETIC_PRIVATE_MESSAGE"')]:
            name = value + '.txt'
            self.file(name, '-----BEGIN PRIVATE KEY-----\n' + text)
            report = scan.scan(self.root, [name])
            self.assertEqual(report['status'], 'findings')
            self.assertNotIn(value, json.dumps(report), 'Earlier findings on the same path must also be redacted')

    def test_traversal_private_paths_and_duplicate_paths_are_rejected_before_reading(self):
        for names in [['../other.txt'], ['/etc/hosts'], ['.private/key.json'], ['.local/key.json'],
                      ['.git/config'], ['./a.txt'], ['a//b.txt'], ['.'], ['bad\ud800.txt'], ['a.txt', 'a.txt']]:
            report = scan.scan(self.root, names)
            self.assertEqual(report['status'], 'incomplete')
            self.assertEqual(report['scanned'], [])

    def test_system_roots_are_not_scanned(self):
        for root in ['/etc', '/proc', '/sys', '/dev', '/', '/home']:
            report = scan.scan(root, ['public.txt'])
            self.assertEqual(report['status'], 'incomplete')
            self.assertEqual(report['scanned'], [])

    def test_leaf_parent_root_symlinks_and_hardlinks_are_rejected(self):
        original = self.file('original.txt', 'public')
        self.file('sub/item.txt', 'public')
        (self.root / 'leaf.txt').symlink_to(original)
        (self.root / 'linked-parent').symlink_to(self.root / 'sub', target_is_directory=True)
        for name in ['leaf.txt', 'linked-parent/item.txt']:
            self.assertEqual(scan.scan(self.root, [name])['status'], 'incomplete')
        (self.root / 'linked-root').symlink_to(self.root / 'sub', target_is_directory=True)
        self.assertEqual(scan.scan(self.root / 'linked-root', ['item.txt'])['status'], 'incomplete')
        os.link(original, self.root / 'hard.txt')
        for name in ['original.txt', 'hard.txt']:
            self.assertEqual(scan.scan(self.root, [name])['status'], 'incomplete')

    def test_fifo_directory_and_read_failure_do_not_block_or_pass(self):
        os.mkfifo(self.root / 'pipe.txt')
        (self.root / 'directory.txt').mkdir()
        started = time.monotonic()
        for name in ['pipe.txt', 'directory.txt', 'missing.txt']:
            self.assertEqual(scan.scan(self.root, [name])['status'], 'incomplete')
        self.assertLess(time.monotonic() - started, 1)
        self.file('public.txt', 'public')
        with patch.object(os, 'read', side_effect=PermissionError):
            self.assertEqual(scan.scan(self.root, ['public.txt'])['status'], 'incomplete')

    def test_binary_bad_encoding_unknown_type_and_long_line_are_incomplete(self):
        for name, data, code in [('utf8.txt', b'\xff', 'invalid_utf8'),
                                 ('binary.txt', b'public\0opaque', 'binary_input'),
                                 ('unknown.bin', b'opaque', 'unsupported_text_type'),
                                 ('long.txt', b'x' * (scan.MAX_LINE + 1), 'line_limit')]:
            self.file(name, data)
            report = scan.scan(self.root, [name])
            self.assertEqual(report['status'], 'incomplete')
            self.assertEqual(report['issues'][0]['code'], code)

    def test_changed_file_is_not_reported_as_a_complete_snapshot(self):
        target = self.file('changed.txt', 'before')
        calls = 0
        def clock():
            nonlocal calls
            calls += 1
            if calls == 3:
                target.write_text('after')
            return 0
        report = scan.scan(self.root, ['changed.txt'], clock=clock)
        self.assertEqual(report['status'], 'incomplete')
        self.assertEqual(report['issues'][0]['code'], 'file_changed')
        self.assertEqual(report['scanned'], [])

    def test_file_count_per_file_total_and_finding_limits(self):
        self.assertEqual(scan.scan(self.root, [f'{n}.txt' for n in range(scan.MAX_FILES + 1)])['status'], 'incomplete')
        self.file('large.txt', b'x' * (scan.MAX_FILE + 1))
        self.assertEqual(scan.scan(self.root, ['large.txt'])['issues'][0]['code'], 'file_limit')
        self.file('one.txt', '12345'); self.file('two.txt', '67890')
        with patch.object(scan, 'MAX_TOTAL', 8):
            report = scan.scan(self.root, ['one.txt', 'two.txt'])
        self.assertEqual(report['status'], 'incomplete'); self.assertEqual(report['bytes_read'], 9)
        self.assertEqual(report['issues'][0]['code'], 'total_limit')
        self.file('many.txt', '\n'.join(['-----BEGIN PRIVATE KEY-----'] * (scan.MAX_FINDINGS + 1)))
        report = scan.scan(self.root, ['many.txt'])
        self.assertEqual(report['status'], 'incomplete'); self.assertEqual(len(report['findings']), scan.MAX_FINDINGS)

    def test_timeout_rollback_invalid_clock_and_exclusions_never_become_clean(self):
        self.file('public.txt', 'public')
        for times in [[0, 10], [10, 9], [float('nan')]]:
            clock = iter(times)
            self.assertEqual(scan.scan(self.root, ['public.txt'], clock=lambda: next(clock))['status'], 'incomplete')
        self.assertEqual(scan.scan(self.root, ['public.txt'], excluded=1)['status'], 'incomplete')

    def test_actual_git_inventory_is_allowlisted_and_counts_excluded_files(self):
        self.file('crates/public.rs', '// public')
        self.file('not-in-scope.bin', b'opaque')
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        subprocess.run(['git', '-C', str(self.root), 'add', '.'], check=True)
        selected, excluded = scan.public_tracked_paths(self.root)
        self.assertEqual(selected, ['crates/public.rs']); self.assertEqual(excluded, 1)

    def test_manifest_schema_duplicates_and_links_fail_closed(self):
        self.file('manifest.json', '{"schema":1,"files":["public.txt"]}')
        self.assertEqual(scan.manifest_paths(self.root / 'manifest.json')[0], ['public.txt'])
        for data in ['{"schema":1,"schema":1,"files":[]}', '{"schema":true,"files":[]}',
                     '{"schema":1,"files":[],"extra":true}', '{"schema":1,"files":"all"}']:
            self.file('manifest.json', data)
            with self.assertRaises(scan.ScanError): scan.manifest_paths(self.root / 'manifest.json')
        (self.root / 'linked.json').symlink_to(self.root / 'manifest.json')
        with self.assertRaises(scan.ScanError): scan.manifest_paths(self.root / 'linked.json')

    def test_default_cli_does_not_read_inputs_or_run_git(self):
        output = self.root / 'report.json'
        with patch('sys.argv', ['scan.py', '--output', str(output)]), \
                patch.object(scan, 'public_tracked_paths', side_effect=AssertionError('unexpected Git')), \
                patch.object(scan, 'scan', side_effect=AssertionError('unexpected input read')), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(scan.main(), 0)
        self.assertFalse(output.exists())

    def test_cli_exit_states_redaction_and_no_overwrite(self):
        manifest = self.file('manifest.json', '{"schema":1,"files":["input.txt"]}')
        for text, expected in [('public', 0), ('password="SYNTHETIC_PASS"', 1), ('\0', 2)]:
            self.file('input.txt', text)
            output = self.root / f'report-{expected}.json'
            with patch('sys.argv', ['scan.py', '--run', '--artifact', str(self.root), '--manifest', str(manifest), '--output', str(output)]), contextlib.redirect_stdout(io.StringIO()) as captured:
                self.assertEqual(scan.main(), expected)
            self.assertNotIn('SYNTHETIC_PASS', output.read_text() + captured.getvalue())
        before = output.read_bytes()
        with patch('sys.argv', ['scan.py', '--run', '--artifact', str(self.root), '--manifest', str(manifest), '--output', str(output)]), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(scan.main(), 2)
        self.assertEqual(output.read_bytes(), before)

    def test_cli_report_parent_link_and_system_directory_are_refused(self):
        manifest = self.file('manifest.json', '{"schema":1,"files":["input.txt"]}')
        self.file('input.txt', 'public')
        (self.root / 'linked').symlink_to(self.root, target_is_directory=True)
        for output in [self.root / 'linked/report.json', Path('/etc/rsh-synthetic-report-must-not-be-created.json')]:
            with patch('sys.argv', ['scan.py', '--run', '--artifact', str(self.root), '--manifest', str(manifest), '--output', str(output)]), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(scan.main(), 2)
        self.assertFalse((self.root / 'report.json').exists())


if __name__ == '__main__':
    unittest.main()
