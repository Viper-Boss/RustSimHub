# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded explicit inputs and exclusive, journalled staging outputs."""
import contextlib
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('kernel_audit', ROOT / 'tools/kernel-artifact-audit/audit.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)
Invalid, need, relative = audit.Invalid, audit.need, audit.relative
MAX_FILE = 128 * 1024 * 1024
MAX_TOTAL = 512 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True) + '\n').encode()


def exact(value, keys):
    need(type(value) is dict and set(value) == set(keys), 'fields')


def number(value, low, high):
    need(type(value) is int and low <= value <= high, 'number')
    return value


def hexadecimal(value, length=64):
    need(isinstance(value, str) and re.fullmatch('[0-9a-f]{%d}' % length, value), 'digest')
    return value


def parse(data):
    need(len(data) <= 256 * 1024, 'json_budget')
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, 'duplicate_field')
            result[key] = value
        return result
    def constant(_):
        raise Invalid('json_constant')
    try:
        return json.loads(data, object_pairs_hook=pairs, parse_constant=constant)
    except (ValueError, UnicodeError, RecursionError):
        raise Invalid('json') from None


class Directory(audit.Directory):
    def read(self, name, limit=MAX_FILE, expected_mode=None):
        parent, leaf = self.parent(name)
        try:
            fd = os.open(leaf, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
        finally:
            os.close(parent)
        try:
            before = os.fstat(fd)
            need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'regular_single_link')
            need(before.st_mode & 0o7022 == 0, 'unsafe_mode')
            need(expected_mode is None or stat.S_IMODE(before.st_mode) == expected_mode, 'mode_mismatch')
            need(before.st_size <= limit, 'file_budget')
            chunks, size = [], 0
            while True:
                part = os.read(fd, min(65536, limit + 1 - size))
                if not part:
                    break
                size += len(part)
                need(size <= limit, 'file_budget')
                chunks.append(part)
            after = os.fstat(fd)
            attrs = ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns', 'st_nlink', 'st_mode')
            need(size == before.st_size and all(getattr(before, k) == getattr(after, k) for k in attrs), 'input_changed')
            return b''.join(chunks)
        finally:
            os.close(fd)

    def write_new(self, name, data, mode=0o600):
        parts = relative(name)
        fd = os.dup(self.fd)
        try:
            for part in parts[:-1]:
                try:
                    os.mkdir(part, 0o700, dir_fd=fd)
                except FileExistsError:
                    pass
                new = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=fd)
                os.fsync(fd)
                os.close(fd)
                fd = new
            out = os.open(parts[-1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, mode, dir_fd=fd)
            try:
                view = memoryview(data)
                while view:
                    written = os.write(out, view)
                    need(written > 0, 'short_write')
                    view = view[written:]
                os.fchmod(out, mode)
                os.fsync(out)
            finally:
                os.close(out)
            os.fsync(fd)
        finally:
            os.close(fd)


@contextlib.contextmanager
def fresh_output(path):
    """Exclusive directory; interrupted outputs retain INCOMPLETE, never reused."""
    raw = Path(path)
    relative(raw.name)
    with Directory(raw.parent) as parent:
        os.mkdir(raw.name, 0o700, dir_fd=parent.fd)
        fd = os.open(raw.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent.fd)
        output = Directory.__new__(Directory)
        output.fd, output.path = fd, raw
        try:
            output.write_new('INCOMPLETE', b'offline staging; not an installed image\n')
            os.fsync(parent.fd)
            yield output
            output.write_new('COMPLETED', b'content checked; authorization_verified=false\n')
            os.unlink('INCOMPLETE', dir_fd=fd)
            os.fsync(fd)
            os.fsync(parent.fd)
        finally:
            os.close(fd)


def write_bundle(path, files, report):
    with fresh_output(path) as output:
        for name, data, mode in files:
            output.write_new(name, data, mode)
            need(output.read(name, len(data), mode) == data, 'output_readback')
        output.write_new('report.json', canonical(report))
    return report


def require_completed(directory):
    try:
        os.stat('INCOMPLETE', dir_fd=directory.fd, follow_symlinks=False)
    except FileNotFoundError:
        pass
    else:
        raise Invalid('incomplete')
    need(directory.read('COMPLETED', 128, 0o600) == b'content checked; authorization_verified=false\n', 'incomplete')
