#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Extract official GNU cross packages privately; no system package installation."""
import concurrent.futures
import hashlib
import json
import os
import pathlib
import re
import shlex
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

BASE = pathlib.Path(__file__).resolve().parents[1] / '.local/wsl-cross'
DOWNLOADS = BASE / 'downloads'
DOWNLOADS.mkdir(parents=True, exist_ok=True)
plan = subprocess.check_output(['apt-get', '-o', 'Debug::NoLocking=1', '-o', 'Dir::Cache::pkgcache=', '-o', 'Dir::Cache::srcpkgcache=', '--print-uris', '--assume-yes', '--download-only', '--no-install-recommends', 'install', 'gcc-aarch64-linux-gnu', 'libc6-dev-arm64-cross'], text=True)
entries = []
for url, filename, size in re.findall(r"^'([^']+)' (\S+) (\d+) \S+$", plan, re.M):
    parsed = urllib.parse.urlparse(url)
    if parsed.hostname != 'archive.ubuntu.com' or not parsed.path.startswith('/ubuntu/pool/'):
        raise SystemExit('Unexpected package repository')
    metadata = subprocess.check_output(['apt-cache', 'show', filename.split('_')[0]], text=True)
    expected_path = urllib.parse.unquote(parsed.path.removeprefix('/ubuntu/'))
    matches = []
    for block in metadata.split('\n\n'):
        fields = dict(line.split(': ', 1) for line in block.splitlines() if ': ' in line and not line.startswith(' '))
        if fields.get('Filename') == expected_path:
            matches.append(fields)
    if len(matches) != 1 or not re.fullmatch('[0-9a-f]{64}', matches[0].get('SHA256', '')):
        raise SystemExit('Missing unambiguous SHA256 in APT package metadata')
    entries.append({'url': urllib.parse.urlunparse(parsed._replace(scheme='https')), 'filename': filename, 'size': int(size), 'sha256': matches[0]['SHA256'], 'version': matches[0]['Version']})
if not 1 <= len(entries) <= 32:
    raise SystemExit('Expected uninstalled private cross toolchain packages')
(DOWNLOADS/'gnu-packages.json').write_text(json.dumps(entries, indent=2)+'\n')

def download(entry):
    target = DOWNLOADS/entry['filename']
    if target.exists() and target.stat().st_size == entry['size'] and hashlib.sha256(target.read_bytes()).hexdigest() == entry['sha256']:
        return target
    for attempt in range(3):
        try:
            with urllib.request.urlopen(entry['url'], timeout=90) as response:
                data = response.read()
            if len(data) != entry['size'] or hashlib.sha256(data).hexdigest() != entry['sha256']:
                raise RuntimeError('Package size or SHA256 mismatch: '+entry['filename'])
            break
        except Exception:
            if attempt == 2: raise
            time.sleep(1)
    target.write_bytes(data)
    print('Verified '+entry['filename'], flush=True)
    return target

with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
    packages = list(pool.map(download, entries))
build_home = pathlib.Path.home()/'.cache/rustsimhub'
build_home.mkdir(mode=0o700, parents=True, exist_ok=True)
if build_home.is_symlink() or build_home.stat().st_uid != os.getuid() or build_home.stat().st_mode & 0o077:
    raise SystemExit('Build cache must be private and owned by the current user')
root = pathlib.Path(tempfile.mkdtemp(prefix='gnu.', dir=build_home))
for package in packages:
    subprocess.run(['dpkg-deb', '--extract', str(package), str(root)], check=True)
for name, executable in [('arm64-cc', 'aarch64-linux-gnu-gcc'), ('arm64-ar', 'aarch64-linux-gnu-ar')]:
    suffix = ' --sysroot='+shlex.quote(str(root))+' -mcpu=cortex-a53' if name == 'arm64-cc' else ''
    tool = root/'usr/bin'/executable
    if not tool.is_file(): raise SystemExit('Cross tool is missing')
    wrapper = BASE/'bin'/name
    libraries = shlex.quote(str(root/'usr/lib/x86_64-linux-gnu'))
    wrapper.write_text('#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH='+libraries+'${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}\nexec '+shlex.quote(str(tool))+suffix+' "$@"\n')
    wrapper.chmod(0o700)
(BASE/'gnu-root.txt').write_text(str(root)+'\n')
(BASE/'env.sh').write_text('''# Source with the absolute RUSTSIMHUB_CROSS_HOME set.
export CARGO_HOME="$RUSTSIMHUB_CROSS_HOME/cargo"
export RUSTUP_HOME="$RUSTSIMHUB_CROSS_HOME/rustup"
export PATH="$CARGO_HOME/bin:$RUSTSIMHUB_CROSS_HOME/bin:$PATH"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$RUSTSIMHUB_CROSS_HOME/bin/arm64-cc"
export CC_aarch64_unknown_linux_gnu="$RUSTSIMHUB_CROSS_HOME/bin/arm64-cc"
export AR_aarch64_unknown_linux_gnu="$RUSTSIMHUB_CROSS_HOME/bin/arm64-ar"
''')
print('Private GNU cross directory '+str(root), flush=True)
