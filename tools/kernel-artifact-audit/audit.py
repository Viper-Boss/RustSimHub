#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded C18 offline artifact observations. Default is a no-read plan.

Reads only an explicit manifest/directory on Linux; never runs artifact code,
extracts archives, downloads, loads modules, probes devices, or grants release.
"""
import argparse
from collections import deque
import errno
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import struct
import sys
import types

ROOT = Path(__file__).resolve().parents[2]
REFERENCE = '6b5de5f3ca5ff706adacd8f25ed7f638546739a9'
LIMITS = {'kernel_image': 128*1024*1024, 'dtb': 128*1024,
          'module': 2*1024*1024, 'firmware': 2*1024*1024}
TOTAL = 1024*1024*1024
MAX_ENTRIES = 2048


class Invalid(Exception):
    pass


class Missing(Exception):
    pass


def need(condition, code):
    if not condition:
        raise Invalid(code)


def integer(value, low, high):
    return type(value) is int and low <= value <= high


def relative(value):
    need(isinstance(value, str) and 0 < len(value) <= 240, 'path')
    need(all(32 < ord(c) < 127 for c in value), 'path')
    parts = value.split('/')
    need(not any(p in ('', '.', '..') for p in parts)
         and '\\' not in value and ':' not in value, 'path')
    need(not PurePosixPath(value).is_absolute(), 'path')
    return parts


class Directory:
    """Pin each directory component with openat/O_NOFOLLOW; never scan it."""
    def __init__(self, path):
        need(sys.platform == 'linux', 'linux_required')
        raw = os.fspath(path)
        need('..' not in Path(raw).parts, 'directory')
        self.path = Path(os.path.abspath(raw))
        need(self.path != Path('/') and not any(
            self.path == p or p in self.path.parents
            for p in (Path('/dev'), Path('/proc'), Path('/sys'))), 'directory')
        fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
        try:
            for part in self.path.parts[1:]:
                new = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
                              | os.O_CLOEXEC, dir_fd=fd)
                os.close(fd)
                fd = new
            self.fd = fd
        except BaseException:
            os.close(fd)
            raise

    def __enter__(self):
        return self

    def __exit__(self, *args):
        os.close(self.fd)

    def parent(self, name):
        parts = relative(name)
        fd = os.dup(self.fd)
        try:
            for part in parts[:-1]:
                new = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
                              | os.O_CLOEXEC, dir_fd=fd)
                os.close(fd)
                fd = new
            return fd, parts[-1]
        except BaseException:
            os.close(fd)
            raise

    def read(self, name, limit):
        try:
            parent, leaf = self.parent(name)
            try:
                fd = os.open(leaf, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW
                             | os.O_CLOEXEC, dir_fd=parent)
            finally:
                os.close(parent)
        except OSError as error:
            if error.errno == errno.ENOENT:
                raise Missing() from None
            raise Invalid('unsafe_input') from None
        try:
            before = os.fstat(fd)
            need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'regular_single_link')
            need(0 <= before.st_size <= limit, 'file_budget')
            chunks, size = [], 0
            while True:
                part = os.read(fd, min(65536, limit + 1 - size))
                if not part:
                    break
                chunks.append(part)
                size += len(part)
                need(size <= limit, 'file_budget')
            after = os.fstat(fd)
            attrs = ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns', 'st_nlink')
            need(all(getattr(before, k) == getattr(after, k) for k in attrs)
                 and size == before.st_size, 'input_changed')
            return b''.join(chunks)
        finally:
            os.close(fd)

    def create(self, name, data):
        parent, leaf = self.parent(name)
        try:
            fd = os.open(leaf, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
                         | os.O_CLOEXEC, 0o600, dir_fd=parent)
        finally:
            os.close(parent)
        with os.fdopen(fd, 'wb') as handle:
            handle.write(data)


def parse_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, 'duplicate_key')
            result[key] = value
        return result
    try:
        return json.loads(raw, object_pairs_hook=pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(Invalid('number')))
    except (ValueError, UnicodeError, RecursionError):
        raise Invalid('json') from None


def fields(value, keys):
    need(type(value) is dict and set(value) == set(keys), 'fields')


def strings(value, limit=64):
    need(type(value) is list and 0 < len(value) <= limit, 'strings')
    need(all(type(s) is str and 0 < len(s) <= 128 and s.isascii()
             and all(32 <= ord(c) < 127 for c in s) for s in value), 'strings')
    need(len(set(value)) == len(value), 'duplicate_string')
    return value


def module_name(name):
    need(type(name) is str and re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_-]{0,63}', name), 'module_name')
    return name.replace('_', '-')


def fdt_compatible(data):
    """Read only the root compatible of a bounded, structurally checked FDT v17."""
    need(40 <= len(data) <= LIMITS['dtb'], 'fdt_header')
    magic, total, off, strings_off, reserve, version, last, _, strings_size, size = struct.unpack_from('>10I', data)
    need(magic == 0xd00dfeed and total == len(data) and version == 17 and last == 16,
         'fdt_header')
    need(off >= 40 and off % 4 == 0 and size % 4 == 0
         and off+size <= total and strings_off >= 40
         and strings_off+strings_size <= total and reserve >= 40 and reserve % 8 == 0,
         'fdt_bounds')
    end_reserve = reserve
    for _ in range(2048):
        need(end_reserve+16 <= total, 'fdt_reserve')
        address, length = struct.unpack_from('>QQ', data, end_reserve)
        end_reserve += 16
        if address == length == 0:
            break
    else:
        raise Invalid('fdt_reserve')
    regions = sorted([(0, 40), (reserve, end_reserve), (off, off+size),
                      (strings_off, strings_off+strings_size)])
    need(all(a[1] <= b[0] for a, b in zip(regions, regions[1:])), 'fdt_overlap')
    table, block = data[strings_off:strings_off+strings_size], data[off:off+size]
    pos, depth, roots, compatible = 0, 0, 0, None
    properties = []
    for _ in range(8192):
        need(pos+4 <= len(block), 'fdt_tokens')
        token = struct.unpack_from('>I', block, pos)[0]
        pos += 4
        if token == 1:
            end = block.find(b'\0', pos)
            need(end >= pos and end-pos <= 128, 'fdt_node')
            if depth == 0:
                roots += 1
                need(roots == 1 and end == pos, 'fdt_root')
            depth += 1
            need(depth <= 64, 'fdt_depth')
            properties.append(set())
            pos = (end+4) & ~3
        elif token == 2:
            need(depth > 0, 'fdt_depth')
            depth -= 1
            properties.pop()
        elif token == 3:
            need(depth > 0 and pos+8 <= len(block), 'fdt_property')
            length, name_offset = struct.unpack_from('>II', block, pos)
            pos += 8
            need(pos+length <= len(block) and name_offset < len(table), 'fdt_property')
            end = table.find(b'\0', name_offset)
            need(end >= name_offset and end-name_offset <= 128, 'fdt_property')
            name = table[name_offset:end]
            need(name and name not in properties[-1], 'fdt_duplicate_property')
            properties[-1].add(name)
            if depth == 1 and name == b'compatible':
                raw = block[pos:pos+length]
                need(raw.endswith(b'\0'), 'fdt_compatible')
                try:
                    compatible = raw[:-1].decode('ascii').split('\0')
                except UnicodeError:
                    raise Invalid('fdt_compatible') from None
                strings(compatible)
            pos = (pos+length+3) & ~3
        elif token == 4:
            pass
        elif token == 9:
            need(depth == 0 and roots == 1 and pos == len(block) and compatible,
                 'fdt_end')
            return compatible
        else:
            raise Invalid('fdt_token')
    raise Invalid('fdt_tokens')


def pinned_sources():
    paths = ['firmware/board.toml', 'firmware/base-system.lock.json',
             'firmware/kernel/upstream-source.lock.json', 'firmware/kernel/base-build-profile.json',
             'firmware/kernel/audio/source.json', 'firmware/kernel/audio/modules/source.json',
             'tools/audio_modules.py']
    with Directory(ROOT) as directory:
        index = parse_json(directory.read('docs/kernel-artifact-audit/source-index.json', 32768))
        need(index['reference'] == REFERENCE, 'source_reference')
        rows = {r['path']: r for r in index['files']}
        result = {}
        for path in paths:
            raw = directory.read(path, 256*1024)
            row = rows[path]
            need(len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],
                 'source_bytes')
            result[path] = raw
    return result


def prepare(manifest):
    """Pinned mode derives expectations from original locks, never caller overrides."""
    source = pinned_sources()
    helper = types.ModuleType('pinned_audio_modinfo')
    helper.__file__ = str(ROOT/'tools/audio_modules.py')
    exec(compile(source['tools/audio_modules.py'], '<pinned-audio-parser>', 'exec'), helper.__dict__)
    need(type(manifest) is dict and type(manifest.get('schema')) is int
         and manifest['schema'] == 1, 'schema')
    if manifest.get('mode') == 'pinned':
        fields(manifest, ['schema', 'mode', 'source_commit', 'mappings'])
        need(manifest['source_commit'] == REFERENCE, 'source_reference')
        mappings = manifest['mappings']
        need(type(mappings) is list and len(mappings) <= MAX_ENTRIES, 'entries')
        by_id = {}
        for mapping in mappings:
            fields(mapping, ['id', 'path'])
            need(type(mapping['id']) is str and mapping['id'] not in by_id, 'duplicate_id')
            relative(mapping['path'])
            by_id[mapping['id']] = mapping['path']
        import tomllib
        board = tomllib.loads(source['firmware/board.toml'].decode())
        profile = parse_json(source['firmware/kernel/base-build-profile.json'])
        audio = parse_json(source['firmware/kernel/audio/source.json'])
        modules = parse_json(source['firmware/kernel/audio/modules/source.json'])
        need(board['kernel_release'] == profile['kernel_release'] == modules['kernel_release'], 'source_release')
        entries = [dict(id='kernel-image', kind='kernel_image', bytes=profile['image_bytes'],
                        sha256=profile['image_sha256'], metadata={}),
                   dict(id='audio-dtb', kind='dtb', bytes=None,
                        sha256=audio['expected_dtb_sha256'], metadata={})]
        for row in modules['expected_modules']:
            info = row['modinfo']
            entries.append(dict(id='audio-'+row['name'][:-3], kind='module', bytes=row['bytes'],
                                sha256=row['sha256'], metadata=dict(name=info['name'],
                                vermagic=info['vermagic'], depends=info['depends'].split(',') if info['depends'] else [],
                                srcversion=info['srcversion'])))
        need(set(by_id) <= {r['id'] for r in entries}, 'unknown_mapping')
        for entry in entries:
            entry['path'] = by_id.get(entry['id'])
        value = dict(schema=1, mode='pinned', kernel_release=profile['kernel_release'],
                     board_compatible=board['compatible'], entries=entries)
    else:
        fields(manifest, ['schema', 'mode', 'kernel_release', 'board_compatible', 'entries'])
        need(manifest['mode'] == 'synthetic', 'mode')
        value = manifest
    need(type(value['kernel_release']) is str and re.fullmatch(r'[A-Za-z0-9_.+-]{1,128}', value['kernel_release']), 'release')
    strings(value['board_compatible'])
    entries = value['entries']
    need(type(entries) is list and len(entries) <= MAX_ENTRIES, 'entries')
    ids, paths, modules = set(), set(), set()
    for entry in entries:
        fields(entry, ['id', 'kind', 'path', 'bytes', 'sha256', 'metadata'])
        need(type(entry['id']) is str and re.fullmatch(r'[a-z0-9][a-z0-9_.-]{0,63}', entry['id'])
             and entry['id'] not in ids, 'id')
        ids.add(entry['id'])
        need(type(entry['kind']) is str and entry['kind'] in LIMITS, 'kind')
        if entry['path'] is not None:
            relative(entry['path'])
            name = entry['path'].casefold()
            need(name not in paths, 'duplicate_path')
            paths.add(name)
        else:
            need(value['mode'] == 'pinned', 'path')
        need(integer(entry['bytes'], 1, LIMITS[entry['kind']])
             or entry['bytes'] is None and value['mode'] == 'pinned' and entry['kind'] == 'dtb', 'bytes')
        need(type(entry['sha256']) is str and re.fullmatch(r'[0-9a-f]{64}', entry['sha256']), 'hash')
        meta = entry['metadata']
        if entry['kind'] == 'module':
            fields(meta, ['name', 'vermagic', 'depends', 'srcversion'])
            name = module_name(meta['name'])
            need(name not in modules, 'duplicate_module')
            modules.add(name)
            need(type(meta['vermagic']) is str and 0 < len(meta['vermagic']) <= 256
                 and all(32 <= ord(c) < 127 for c in meta['vermagic']), 'vermagic')
            need(type(meta['srcversion']) is str and re.fullmatch('[A-Za-z0-9]{1,64}', meta['srcversion']), 'srcversion')
            need(type(meta['depends']) is list and len(meta['depends']) <= 64, 'depends')
            normalized = [module_name(s) for s in meta['depends']]
            need(len(set(normalized)) == len(normalized), 'duplicate_dependency')
        elif entry['kind'] == 'firmware':
            fields(meta, ['license'])
            need(meta['license'] is None or type(meta['license']) is str
                 and re.fullmatch(r'[A-Za-z0-9_.+-]{1,128}', meta['license']), 'license')
        else:
            fields(meta, [])
    return value, helper


def inspect(manifest, directory):
    value, helper = prepare(manifest)
    rows, observed_modules, total = [], {}, 0
    for entry in value['entries']:
        row = dict(id=entry['id'], kind=entry['kind'], status='not_verified', reasons=[])
        if entry['path'] is None:
            row['reasons'].append('missing_mapping')
            rows.append(row)
            continue
        try:
            remaining = TOTAL-total
            need(remaining >= 0, 'total_budget')
            data = directory.read(entry['path'], min(LIMITS[entry['kind']], remaining))
        except Missing:
            row['reasons'].append('missing_file')
            rows.append(row)
            continue
        total += len(data)
        row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), status='observed')
        if row['sha256'] != entry['sha256'] or entry['bytes'] is not None and len(data) != entry['bytes']:
            row['reasons'].append('digest_or_size')
        try:
            if entry['kind'] == 'kernel_image':
                need(len(data) >= 64 and data[56:60] == b'ARM\x64'
                     and 64 <= struct.unpack_from('<Q', data, 16)[0] <= LIMITS['kernel_image']
                     and struct.unpack_from('<Q', data, 24)[0] & 1 == 0, 'arm64_image')
            elif entry['kind'] == 'dtb':
                compatible = fdt_compatible(data)
                if compatible != value['board_compatible']:
                    row['reasons'].append('board_compatible')
            elif entry['kind'] == 'module':
                need(len(data) >= 64 and data[6] == 1
                     and struct.unpack_from('<Q', data, 40)[0] >= 64, 'elf_header')
                info = helper.modinfo(data)
                meta = entry['metadata']
                expected = dict(name=meta['name'], vermagic=meta['vermagic'],
                                depends=','.join(meta['depends']), srcversion=meta['srcversion'])
                if info != expected:
                    row['reasons'].append('module_metadata')
                if info.get('vermagic', '').split(' ')[0] != value['kernel_release']:
                    row['reasons'].append('release')
                name = module_name(info.get('name'))
                if name in observed_modules:
                    row['reasons'].append('module_alias_conflict')
                deps = info.get('depends', '').split(',') if info.get('depends') else []
                deps = [module_name(d) for d in deps]
                need(len(deps) <= 64 and len(set(deps)) == len(deps), 'module_dependencies')
                if not row['reasons']:
                    observed_modules[name] = deps
            elif entry['metadata']['license'] is None:
                row['reasons'].append('firmware_license_missing')
                row['status'] = 'not_verified'
        except (Invalid, ValueError, UnicodeError, struct.error):
            row['reasons'].append('binary_structure')
        if row['reasons'] and row['reasons'] != ['firmware_license_missing']:
            row['status'] = 'mismatch'
        rows.append(row)
    gaps = []
    kinds = {r['kind'] for r in value['entries']}
    for kind in sorted(set(LIMITS)-kinds):
        gaps.append('missing_kind:'+kind)
    if value['mode'] == 'pinned':
        gaps.extend(['stock_modules_not_verified', 'firmware_manifest_and_license_missing'])
    unresolved = sorted({d for ds in observed_modules.values() for d in ds if d not in observed_modules})
    if unresolved:
        gaps.append('unresolved_module_dependencies')
    # Kahn traversal is bounded and non-recursive even for 2048-node inputs.
    indegree = {n: len([d for d in ds if d in observed_modules]) for n, ds in observed_modules.items()}
    reverse = {n: [] for n in observed_modules}
    for name, deps in observed_modules.items():
        for dep in deps:
            if dep in reverse:
                reverse[dep].append(name)
    queue = deque(n for n, count in indegree.items() if count == 0)
    seen = 0
    while queue:
        seen += 1
        for n in reverse[queue.popleft()]:
            indegree[n] -= 1
            if indegree[n] == 0:
                queue.append(n)
    cycle = seen != len(indegree)
    mismatch = cycle or any(r['status'] == 'mismatch' for r in rows)
    incomplete = gaps or any(r['status'] == 'not_verified' for r in rows)
    code = 4 if mismatch else 3 if incomplete else 0
    return dict(schema=1, execute=True, mode=value['mode'], source_reference=REFERENCE,
                source_trust='recorded_source_locks' if value['mode'] == 'pinned' else 'synthetic_only',
                exit_code=code, byte_checks_complete=code == 0, observations=rows,
                gaps=gaps, unresolved_module_dependencies=unresolved, dependency_cycle=cycle,
                bytes_read=total, firmware_license='not_verified', boot='not_verified',
                recovery='not_verified', release_support='not_verified', authorization_verified=False), code


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args(argv)
    if not args.run:
        print(json.dumps(dict(schema=1, execute=False, modes=['pinned', 'synthetic'],
                              source_reference=REFERENCE, device_access=False)))
        return 0
    try:
        need(sys.platform == 'linux' and os.geteuid() != 0, 'unprivileged_linux_required')
        need(args.manifest is not None and args.artifacts is not None, 'explicit_inputs')
        with Directory(args.manifest.parent) as parent:
            manifest = parse_json(parent.read(args.manifest.name, 1024*1024))
        with Directory(args.artifacts) as directory:
            if args.report is not None:
                report = Path(os.path.abspath(args.report))
                need(report != Path(os.path.abspath(args.manifest))
                     and report != directory.path and directory.path not in report.parents,
                     'report_alias')
                # Reject existing report before reading any artifacts.
                need(not os.path.lexists(report), 'report_exists')
            result, code = inspect(manifest, directory)
        if args.report is not None:
            with Directory(args.report.parent) as parent:
                parent.create(args.report.name, (json.dumps(result, sort_keys=True)+'\n').encode())
    except (Invalid, Missing, OSError, KeyError, TypeError, ValueError, RecursionError) as error:
        # No host paths or parser exception text cross the public boundary.
        reason = str(error) if isinstance(error, Invalid) else 'input_io_or_format'
        if not re.fullmatch(r'[a-z_]{1,40}', reason):
            reason = 'input_io_or_format'
        print(json.dumps(dict(schema=1, execute=True, exit_code=1, error='invalid_input', reason=reason,
                              authorization_verified=False)))
        return 1
    print(json.dumps(result, sort_keys=True))
    return code


if __name__ == '__main__':
    sys.dont_write_bytecode = True
    raise SystemExit(main())
