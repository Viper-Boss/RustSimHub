#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded, read-only heuristic scan of explicit public inputs; never echoes hits."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import selectors
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
MAX_FILE = 512 * 1024
MAX_TOTAL = 8 * 1024 * 1024
MAX_FILES = 1024
MAX_FINDINGS = 256
MAX_LINE = 16384
SECONDS = 5
PRIVATE_PARTS = {'.private', '.local', '.git', '.ssh', 'backups'}
TEXT_SUFFIXES = {'.rs', '.go', '.py', '.js', '.mjs', '.ts', '.json', '.toml', '.lock',
                 '.md', '.txt', '.html', '.css', '.yml', '.yaml', '.sh', '.conf', '.service',
                 '.socket', '.rules', '.webmanifest', '.svg', '.nmconnection'}
PUBLIC_PREFIXES = {'crates', 'configurator', 'tools', 'native', 'firmware', 'docs', '.github'}
PUBLIC_ROOT_FILES = {'Cargo.toml', 'Cargo.lock', 'README.md', 'AGENTS.md', 'LICENSE'}

TOKEN = re.compile(r'\b(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]{20,255}\b')
RULES = (
    ('private_key', re.compile(r'-----BEGIN (?:[A-Z0-9 ]{0,32})PRIVATE KEY-----')),
    ('access_token', TOKEN),
    ('secret_literal', re.compile(r'''(?i)(?<![\w])(?:["']?)(?:password|passphrase|wifi_password|smtp_password|owner_password|access_token|refresh_token|token|seed|private_key)["']?\s*[:=]\s*["']([^"'\r\n]{8,256})["']''')),
    ('wifi_psk', re.compile(r'(?i)^\s*(?:psk|wifi_password|wpa_passphrase)\s*=\s*([^\r\n#;]{8,66})\s*(?:[#;].*)?$')),
    ('personal_identifier', re.compile(r'''(?i)\b(?:phone_number|recipient|imsi|iccid)["']?\s*[:=]\s*["'](\+?[0-9]{6,22})["']''')),
    ('communication_text', re.compile(r'''(?i)\b(?:sms_body|message_body|sms_text)["']?\s*[:=]\s*["']([^"'\r\n]{8,256})["']''')),
    ('bearer_token', re.compile(r'\beyJ[A-Za-z0-9_-]{8,256}\.[A-Za-z0-9_-]{8,256}\.[A-Za-z0-9_-]{8,512}\b')),
)


class ScanError(Exception):
    pass


class Budget:
    def __init__(self, clock=time.monotonic):
        self.clock, self.started, self.total = clock, clock(), 0
        if not math.isfinite(self.started):
            raise ScanError('invalid_clock')

    def check(self):
        elapsed = self.clock() - self.started
        if not math.isfinite(elapsed) or elapsed < 0 or elapsed >= SECONDS:
            raise ScanError('deadline_or_clock')


def relative_name(value):
    if (not isinstance(value, str) or not value or len(value) > 256 or '\\' in value
            or any(ord(c) < 32 or ord(c) == 127 or 0xD800 <= ord(c) <= 0xDFFF for c in value)):
        raise ScanError('invalid_path')
    path = PurePosixPath(value)
    if (not path.parts or path.is_absolute() or path.as_posix() != value or '..' in path.parts
            or any(p.casefold() in PRIVATE_PARTS for p in path.parts)):
        raise ScanError('invalid_path')
    return value


def path_label(value):
    raw = value if isinstance(value, str) else '<invalid>'
    try:
        relative_name(raw)
        if not any(pattern.search(raw) for _, pattern in RULES):
            return raw
    except ScanError:
        pass
    return 'redacted-path-' + hashlib.sha256(raw.encode('utf8', 'replace')).hexdigest()[:16]


def open_root(root):
    root = Path(root).absolute()
    if (root == Path('/') or root.parts[1] in {'dev', 'proc', 'sys', 'etc', 'home', 'root'}
            or any(p.casefold() in PRIVATE_PARTS or p == '..' for p in root.parts)):
        raise ScanError('private_or_system_root')
    if not hasattr(os, 'O_NOFOLLOW') or os.open not in os.supports_dir_fd:
        raise ScanError('unsupported_safe_io')
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
    descriptor = os.open('/', flags)
    try:
        for part in root.parts[1:]:
            child = os.open(part, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except OSError:
        os.close(descriptor)
        raise ScanError('unsafe_or_unreadable_root') from None


def read_at(root_fd, name, budget, limit=MAX_FILE):
    relative_name(name)
    descriptor = os.dup(root_fd)
    try:
        parts = PurePosixPath(name).parts
        for part in parts[:-1]:
            budget.check()
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC, dir_fd=descriptor)
        try:
            before = os.fstat(file_fd)
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
                raise ScanError('nonregular_or_linked_file')
            if before.st_size > limit:
                raise ScanError('file_limit')
            chunks, size = [], 0
            while True:
                budget.check()
                chunk = os.read(file_fd, min(65536, limit + 1 - size, MAX_TOTAL + 1 - budget.total))
                if not chunk:
                    break
                chunks.append(chunk); size += len(chunk); budget.total += len(chunk)
                if size > limit:
                    raise ScanError('file_limit')
                if budget.total > MAX_TOTAL:
                    raise ScanError('total_limit')
            after = os.fstat(file_fd)
            if ((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                    != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns)):
                raise ScanError('file_changed')
            return b''.join(chunks)
        finally:
            os.close(file_fd)
    except OSError:
        raise ScanError('unsafe_or_unreadable_file') from None
    finally:
        os.close(descriptor)


def public_tracked_paths(root=ROOT):
    # Read only Git's path inventory, not author data or history. Bound pipe reads
    # before decoding; never emit Git stderr or a raw invalid filename.
    process = subprocess.Popen(['git', 'ls-files', '-z'], cwd=root, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    data = bytearray(); deadline = time.monotonic() + 3
    try:
        os.set_blocking(process.stdout.fileno(), False)
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise ScanError('git_inventory_deadline')
                if not selector.select(min(remaining, .1)):
                    continue
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                data.extend(chunk)
                if len(data) > 2 * 1024 * 1024:
                    raise ScanError('git_inventory_limit')
        if process.wait(timeout=max(.01, deadline - time.monotonic())) != 0:
            raise ScanError('git_inventory_unavailable')
        names = data.decode('utf8').split('\0')
        if names and names[-1] == '':
            names.pop()
        if len(names) > 8192:
            raise ScanError('git_inventory_limit')
        selected = [name for name in names if (PurePosixPath(name).parts[0] in PUBLIC_PREFIXES
                    or name in PUBLIC_ROOT_FILES) and (Path(name).suffix in TEXT_SUFFIXES or name == 'LICENSE')]
        return selected, len(names) - len(selected)
    except (OSError, UnicodeError, subprocess.TimeoutExpired):
        raise ScanError('git_inventory_unavailable') from None
    finally:
        if process.poll() is None:
            process.kill()
        process.wait(timeout=1)
        process.stdout.close()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ScanError('duplicate_manifest_field')
        result[key] = value
    return result


def manifest_paths(manifest):
    manifest = Path(manifest).absolute()
    descriptor = open_root(manifest.parent)
    try:
        raw = read_at(descriptor, manifest.name, Budget(), 128 * 1024)
    finally:
        os.close(descriptor)
    try:
        value = json.loads(raw, object_pairs_hook=unique_object)
    except (ValueError, UnicodeError):
        raise ScanError('invalid_manifest') from None
    if (not isinstance(value, dict) or set(value) != {'schema', 'files'}
            or type(value['schema']) is not int or value['schema'] != 1 or not isinstance(value['files'], list)):
        raise ScanError('invalid_manifest')
    return value['files'], hashlib.sha256(raw).hexdigest()


def scan(root, names, *, excluded=0, clock=time.monotonic):
    report = {'schema': 1, 'scope': 'explicit public text inputs only', 'status': 'incomplete',
              'input_count': len(names), 'out_of_scope_count': excluded, 'scanned': [], 'findings': [], 'issues': [],
              'limits': {'files': MAX_FILES, 'file_bytes': MAX_FILE, 'total_bytes': MAX_TOTAL, 'seconds': SECONDS,
                         'findings': MAX_FINDINGS, 'line_characters': MAX_LINE},
              'note': 'Heuristic findings need review. No findings is not proof of secret-free artifacts; unselected files are not scanned.'}
    descriptor = None
    try:
        if not names or len(names) > MAX_FILES:
            raise ScanError('file_count_limit')
        validated = [relative_name(name) for name in names]
        if len(set(validated)) != len(validated):
            raise ScanError('duplicate_path')
        budget = Budget(clock)
        descriptor = open_root(root)
        for index, name in enumerate(validated):
            label = path_label(name)
            first_finding = len(report['findings'])
            try:
                budget.check()
                if Path(name).suffix not in TEXT_SUFFIXES and name != 'LICENSE':
                    raise ScanError('unsupported_text_type')
                raw = read_at(descriptor, name, budget)
                text = raw.decode('utf8')
                if '\0' in text:
                    raise ScanError('binary_input')
                for number, line in enumerate(text.splitlines(), 1):
                    budget.check()
                    if len(line) > MAX_LINE:
                        raise ScanError('line_limit')
                    for rule, pattern in RULES:
                        for match in pattern.finditer(line):
                            if rule in {'secret_literal', 'wifi_psk'}:
                                literal = match[1] if rule == 'secret_literal' else match[1].strip().strip('"\'')
                                if (len(literal) < 8 or literal in {'<redacted>', '<placeholder>'}
                                        or re.fullmatch(r'\$\{[A-Z0-9_]+\}|\$\{\{\s*secrets\.[A-Za-z0-9_]+\s*\}\}', literal)):
                                    continue
                            hit = literal if rule == 'wifi_psk' else match[1] if rule in {'secret_literal', 'personal_identifier', 'communication_text'} else match[0]
                            if hit in name:
                                label = 'redacted-path-' + hashlib.sha256(name.encode()).hexdigest()[:16]
                                for previous in report['findings'][first_finding:]:
                                    previous['path'] = label
                            report['findings'].append({'rule': rule, 'path': label, 'line': number})
                            if len(report['findings']) >= MAX_FINDINGS:
                                raise ScanError('finding_limit')
                            break
                report['scanned'].append({'path': label, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
            except UnicodeError:
                report['issues'].append({'code': 'invalid_utf8', 'path': label})
            except ScanError as error:
                report['issues'].append({'code': str(error), 'path': label})
                if str(error) in {'deadline_or_clock', 'total_limit', 'finding_limit'}:
                    report['not_scanned_count'] = len(validated) - index
                    break
        report['bytes_read'] = budget.total
    except ScanError as error:
        report['issues'].append({'code': str(error)})
    except OSError:
        report['issues'].append({'code': 'unreadable_input'})
    finally:
        if descriptor is not None:
            os.close(descriptor)
    report['status'] = 'incomplete' if report['issues'] or excluded else 'findings' if report['findings'] else 'no_findings_in_scope'
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    group = parser.add_mutually_exclusive_group()
    group.add_argument('--tracked', action='store_true')
    group.add_argument('--artifact', type=Path)
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'reads_content': False, 'runs_commands': False,
                          'uploads': False, 'changes_inputs': False, 'rules': [name for name, _ in RULES]}))
        return 0
    if args.output is None or not (args.tracked or args.artifact) or bool(args.artifact) != bool(args.manifest):
        parser.error('--run requires --output and either --tracked or --artifact with --manifest')
    try:
        if args.tracked:
            names, excluded = public_tracked_paths()
            report = scan(ROOT, names, excluded=excluded)
        else:
            names, digest = manifest_paths(args.manifest)
            report = scan(args.artifact, names)
            report['manifest_sha256'] = digest
    except (ScanError, OSError) as error:
        report = {'schema': 1, 'status': 'incomplete', 'findings': [],
                  'issues': [{'code': str(error) if isinstance(error, ScanError) else 'input_unavailable'}]}
    output_directory = None
    try:
        relative_name(args.output.name)
        output_directory = open_root(args.output.parent)
        output_fd = os.open(args.output.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC,
                            0o600, dir_fd=output_directory)
        with os.fdopen(output_fd, 'w', encoding='utf8') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
            output.write('\n')
    except (OSError, ScanError):
        print('Report output unavailable; no complete scan verdict.')
        return 2
    finally:
        if output_directory is not None:
            os.close(output_directory)
    print(json.dumps({'status': report['status'], 'findings': len(report['findings']), 'issues': len(report['issues'])}))
    return {'no_findings_in_scope': 0, 'findings': 1, 'incomplete': 2}[report['status']]


if __name__ == '__main__':
    raise SystemExit(main())
