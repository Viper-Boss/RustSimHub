#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Reuse public crate archives only after checking the committed Cargo.lock checksum."""
import argparse
import hashlib
import os
import pathlib
import tarfile
import tempfile
import tomllib

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--archive', type=pathlib.Path, required=True)
p.add_argument('--destination', type=pathlib.Path, required=True)
p.add_argument('--source', type=pathlib.Path, default=pathlib.Path.home()/'.cargo/registry/cache')
a = p.parse_args()
root = pathlib.Path(__file__).resolve().parents[1]
destination = a.destination.resolve()
if not destination.is_relative_to((root/'.local').resolve()) or not destination.is_dir():
    raise SystemExit('Destination must be an existing project-local cache')
with tarfile.open(a.archive) as archive:
    lock = tomllib.loads(archive.extractfile('Cargo.lock').read().decode())
copied = 0
for package in lock['package']:
    if package.get('source') != 'registry+https://github.com/rust-lang/crates.io-index':
        continue
    name = package['name']+'-'+package['version']+'.crate'
    target = destination/name
    if target.exists(): continue
    for source in a.source.glob('*/'+name):
        if source.stat().st_size > 64*1024*1024: continue
        data = source.read_bytes()
        if hashlib.sha256(data).hexdigest() != package['checksum']: continue
        fd, path = tempfile.mkstemp(prefix='.verified-cache-', dir=destination)
        temporary = pathlib.Path(path)
        try:
            with os.fdopen(fd, 'wb') as f: f.write(data)
            try:
                # Hard link publishes a complete verified file without overwriting a concurrent fetch.
                os.link(temporary, target)
                copied += 1
            except FileExistsError:
                pass
        finally:
            temporary.unlink()
        break
print(f'Reused {copied} public crate archives with exact Cargo.lock SHA256 checks')
