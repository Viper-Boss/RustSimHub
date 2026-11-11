# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded reads only. Candidate paths are never executed or followed through links."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat


class Refused(ValueError):
    pass


def need(condition, code):
    if not condition:
        raise Refused(code)


def parse(raw, limit=1024 * 1024):
    need(type(raw) is bytes and 0 < len(raw) <= limit, 'json_budget')
    def unique(pairs):
        result = {}
        for key, value in pairs:
            need(key not in result, 'json_duplicate')
            result[key] = value
        return result
    def nonfinite(_):
        raise Refused('json_nonfinite')
    try:
        return json.loads(raw, object_pairs_hook=unique, parse_constant=nonfinite)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise Refused('json_invalid') from error


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)


def same(left, right):
    return canonical(left) == canonical(right)


def hex_id(value, length):
    return type(value) is str and re.fullmatch('[0-9a-f]{' + str(length) + '}', value) is not None


def relative(value):
    need(type(value) is str and 0 < len(value) <= 512, 'path_invalid')
    path = PurePosixPath(value)
    need(not path.is_absolute() and '\\' not in value and ':' not in value
         and all(p not in ('', '.', '..') for p in value.split('/')), 'path_escape')
    need(all(re.fullmatch('[A-Za-z0-9_.@+-]+', p) for p in path.parts), 'path_characters')
    return path.parts


def stamp(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_gid,
            info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


class Tree:
    """Open-at directory walk retains every parent until the read is rechecked."""
    def __init__(self, path):
        self.path = Path(path).absolute()
        need(not any(p in ('.private', '.ssh', '.gnupg', '.aws') for p in self.path.parts),
             'private_directory_refused')
        self.fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
        try:
            for part in self.path.parts[1:]:
                next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                                  dir_fd=self.fd)
                os.close(self.fd)
                self.fd = next_fd
        except BaseException:
            os.close(self.fd)
            raise
        self.identity = stamp(os.fstat(self.fd))

    def __enter__(self):
        return self

    def __exit__(self, *_):
        os.close(self.fd)

    def read(self, name, limit=1024 * 1024, mode=None, allowed_modes=None):
        parts = relative(name)
        parents = [(os.dup(self.fd), self.identity)]
        file_fd = None
        try:
            for part in parts[:-1]:
                fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                             dir_fd=parents[-1][0])
                parents.append((fd, stamp(os.fstat(fd))))
            file_fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                              dir_fd=parents[-1][0])
            before = os.fstat(file_fd)
            need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'file_type_or_links')
            need(0 < before.st_size <= limit, 'file_budget')
            need(not before.st_mode & 0o022, 'writable_public_input')
            if mode is not None:
                need(stat.S_IMODE(before.st_mode) == mode, 'file_mode')
            if allowed_modes is not None:
                need(stat.S_IMODE(before.st_mode) in allowed_modes, 'file_mode')
            output = bytearray()
            while len(output) <= limit:
                part = os.read(file_fd, min(1024 * 1024, limit + 1 - len(output)))
                if not part:
                    break
                output.extend(part)
            need(len(output) == before.st_size and stamp(os.fstat(file_fd)) == stamp(before),
                 'file_changed')
            need(stamp(os.stat(parts[-1], dir_fd=parents[-1][0], follow_symlinks=False)) == stamp(before),
                 'file_replaced')
            for fd, original in parents:
                need(stamp(os.fstat(fd)) == original, 'parent_changed')
            # Reopening the named directory detects an ancestor replacement.
            with Tree(self.path) as current:
                need(current.identity == self.identity, 'tree_replaced')
            return bytes(output)
        finally:
            if file_fd is not None:
                os.close(file_fd)
            for fd, _ in reversed(parents):
                os.close(fd)

    def json(self, name, limit=1024 * 1024):
        return parse(self.read(name, limit), limit)

    def names(self, directory):
        parts = relative(directory)
        fd = os.dup(self.fd)
        try:
            for part in parts:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                                dir_fd=fd)
                os.close(fd)
                fd = child
            names = os.listdir(fd)
            need(len(names) <= 20000, 'entry_budget')
            return sorted(names)
        finally:
            os.close(fd)


def read_file(path, limit=64 * 1024 * 1024):
    path = Path(path).absolute()
    with Tree(path.parent) as tree:
        return tree.read(path.name, limit)
