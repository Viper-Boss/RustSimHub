# SPDX-License-Identifier: AGPL-3.0-only
"""C27: reproducible external newc initramfs, no unpacking or execution."""
import stat
import re
from .common import MAX_FILE, MAX_TOTAL, canonical, digest, exact, need, number, relative, write_bundle, audit

MAX_ENTRIES = 2048


def pad(data):
    return data + b'\0' * (-len(data) % 4)


def record(name, data, mode, ino):
    encoded = name.encode('ascii') + b'\0'
    fields = (ino, mode, 0, 0, 1, 0, len(data), 0, 0, 0, 0, len(encoded), 0)
    return pad(b'070701' + b''.join(('%08x' % n).encode() for n in fields) + encoded) + pad(data)


def build(manifest, source):
    exact(manifest, ('schema', 'files'))
    need(type(manifest['schema']) is int and manifest['schema'] == 1, 'schema')
    need(type(manifest['files']) is list and 0 < len(manifest['files']) <= MAX_ENTRIES, 'entry_budget')
    nodes, total = {}, 0
    for entry in manifest['files']:
        exact(entry, ('path', 'source', 'sha256', 'mode'))
        parts = relative(entry['path'])
        relative(entry['source'])
        need(entry['path'] != 'TRAILER!!!' and entry['path'] not in nodes, 'duplicate_path')
        need(entry['mode'] in ('0644', '0755'), 'mode')
        data = source.read(entry['source'], MAX_FILE)
        total += len(data)
        need(total <= MAX_TOTAL and digest(data) == entry['sha256'], 'integrity')
        nodes[entry['path']] = (data, stat.S_IFREG | int(entry['mode'], 8))
        for i in range(1, len(parts)):
            parent = '/'.join(parts[:i])
            need(parent not in nodes or stat.S_ISDIR(nodes[parent][1]), 'parent_file')
            nodes[parent] = (b'', stat.S_IFDIR | 0o755)
    need('init' in nodes and nodes['init'][1] == stat.S_IFREG | 0o755, 'executable_init_required')
    need(len(nodes) <= MAX_ENTRIES, 'entry_budget')
    archive = b''.join(record(name, data, mode, i + 1) for i, (name, (data, mode)) in enumerate(sorted(nodes.items())))
    archive += record('TRAILER!!!', b'', 0, len(nodes) + 1)
    need(len(archive) <= MAX_TOTAL, 'archive_budget')
    observations = inspect(archive)
    return archive, observations


def inspect(data):
    need(len(data) <= MAX_TOTAL, 'archive_budget')
    offset, seen, observations, trailer = 0, {}, [], False
    while offset < len(data):
        need(len(seen) <= MAX_ENTRIES and len(data) - offset >= 110, 'header')
        start = offset
        header = data[offset:offset + 110]
        need(header[:6] == b'070701', 'newc_magic')
        need(re.fullmatch(b'[0-9a-fA-F]{104}', header[6:]), 'header_hex')
        try:
            fields = [int(header[i:i + 8], 16) for i in range(6, 110, 8)]
        except ValueError:
            raise audit.Invalid('header_hex') from None
        ino, mode, uid, gid, nlink, mtime, size, devmaj, devmin, rmaj, rmin, length, check = fields
        need(0 < length <= 241 and size <= MAX_FILE, 'entry_budget')
        offset += 110
        raw = data[offset:offset + length]
        need(len(raw) == length and raw[-1:] == b'\0' and b'\0' not in raw[:-1], 'name')
        try:
            name = raw[:-1].decode('ascii')
        except UnicodeError:
            raise audit.Invalid('name') from None
        offset += length
        aligned = (offset + 3) & ~3
        need(data[offset:aligned] == b'\0' * (aligned - offset), 'padding')
        offset = aligned
        content = data[offset:offset + size]
        need(len(content) == size, 'truncated')
        offset += size
        aligned = (offset + 3) & ~3
        need(data[offset:aligned] == b'\0' * (aligned - offset), 'padding')
        offset = aligned
        need((uid, gid, nlink, mtime, devmaj, devmin, rmaj, rmin, check) == (0, 0, 1, 0, 0, 0, 0, 0, 0), 'metadata')
        if name == 'TRAILER!!!':
            need(mode == 0 and size == 0 and offset == len(data), 'trailer')
            trailer = True
            break
        parts = relative(name)
        need(name not in seen and ino not in seen.values(), 'duplicate_entry')
        need(mode in (stat.S_IFDIR | 0o755, stat.S_IFREG | 0o644, stat.S_IFREG | 0o755), 'file_type_mode')
        need(not stat.S_ISDIR(mode) or size == 0, 'directory_size')
        for i in range(1, len(parts)):
            parent = '/'.join(parts[:i])
            need(any(e['path'] == parent and e['mode'] == format(stat.S_IFDIR | 0o755, 'o') for e in observations), 'parent_directory')
        seen[name] = ino
        observations.append({'path': name, 'mode': format(mode, 'o'), 'bytes': size, 'sha256': digest(content)})
        need(offset > start, 'progress')
    need(trailer and any(e['path'] == 'init' and e['mode'] == format(stat.S_IFREG | 0o755, 'o') for e in observations), 'init_trailer')
    return observations


def compose(manifest, source, output, kernel_manifest=None, kernel_source=None):
    data, entries = build(manifest, source)
    report = {'schema': 1, 'kind': 'external-initramfs-candidate', 'sha256': digest(data),
              'entries': entries, 'authorization_verified': False, 'boot_verified': False,
              'kernel_cohort_verified': False}
    if kernel_manifest is not None:
        # Existing UPD-07 checker owns its locked kernel/DTB/module semantics.
        cohort, code = audit.inspect(kernel_manifest, kernel_source)
        need(code == 0 and cohort['byte_checks_complete'], 'kernel_cohort_incomplete')
        report['kernel_cohort_verified'] = cohort.get('mode') == 'pinned' and cohort.get('source_trust') == 'recorded_source_locks'
        report['kernel_byte_checks_complete'] = True
        report['kernel_source_trust'] = cohort['source_trust']
        report['kernel_audit_sha256'] = digest(canonical(cohort))
    return write_bundle(output, [('initramfs.cpio', data, 0o600)], report)
