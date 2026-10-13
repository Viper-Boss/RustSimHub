#!/usr/bin/env python3
"""Download public native build dependencies with a fixed allowlist and SHA-256."""
import hashlib
import json
import pathlib
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
output = ROOT / '.local' / 'audio-image-context'
output.mkdir(parents=True, exist_ok=True)
lock = json.loads((ROOT / 'tools/audio-builder-deps.json').read_text(encoding='utf-8'))
checksums = []
for entry in lock['packages']:
    url = urllib.parse.urlparse(entry['url'])
    if url.scheme != 'https' or url.netloc != 'deb.debian.org' or not url.path.startswith('/debian/pool/'):
        raise SystemExit('Unsupported package source')
    destination = output / pathlib.PurePosixPath(url.path).name
    data = destination.read_bytes() if destination.exists() else urllib.request.urlopen(entry['url'], timeout=45).read()
    if len(data) > 16 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != entry['sha256']:
        raise SystemExit('Native dependency hash mismatch')
    destination.write_bytes(data)
    checksums.append(entry['sha256'] + '  ' + destination.name)
    print(f"Verified {entry['name']} {entry['version']}")
(output / 'SHA256SUMS').write_text('\n'.join(checksums) + '\n', encoding='utf-8', newline='\n')
