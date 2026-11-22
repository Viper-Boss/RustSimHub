#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build the opt-in NKN reporting layer from a verified RustSimHub archive."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from arm_build_manifest import regular, sha, verify_source

BINARY = 'rustsimhub-usage-checkin'
NOTICES = 'THIRD-PARTY-NOTICES.txt'
MODULE = 'tools/nkn-usage-checkin'
GO_VERSION = 'go1.25.7'
LICENSE_NAMES = ('LICENSE', 'LICENSE.txt', 'LICENSE.md', 'COPYING', 'NOTICE', 'NOTICE.txt', 'PATENTS')


def notices(go, env, source, module, binary):
    parts = ['RustSimHub (AGPL-3.0-only)\n' + (source/'LICENSE').read_text()]
    goroot = Path(subprocess.check_output([go, 'env', 'GOROOT'], cwd=module, env=env, text=True).strip())
    parts.append('Go runtime '+GO_VERSION+'\n'+(goroot/'LICENSE').read_text())
    metadata = subprocess.check_output([go, 'version', '-m', binary], cwd=module, env=env, text=True)
    for line in metadata.splitlines():
        fields = line.strip().split('\t')
        if fields[0] != 'dep':
            continue
        path, version = fields[1:3]
        record = json.loads(subprocess.check_output(
            [go, 'list', '-mod=readonly', '-m', '-json', path], cwd=module, env=env, text=True))
        if record['Version'] != version or 'Replace' in record:
            raise ValueError('unverified Go dependency '+path)
        licenses = [Path(record['Dir'])/name for name in LICENSE_NAMES]
        licenses = [file for file in licenses if file.is_file()]
        if not licenses:
            raise ValueError('missing Go dependency license for '+path)
        for license in licenses:
            parts.append(path+' '+version+' '+license.name+'\n'+regular(license, 128*1024).decode('utf-8'))
    return ('\n\n'.join(parts)+'\n').encode()


def build(go, archive, revision, source_hash, output):
    repo = Path(__file__).resolve().parents[2]
    env = dict(os.environ, GOENV='off', GOWORK='off', GOTOOLCHAIN='local', GOPROXY='off',
               GOSUMDB='off', GOFLAGS='', CGO_ENABLED='0', GOOS='linux', GOARCH='arm64',
               GOARM64='v8.0', GOEXPERIMENT='')
    version = subprocess.check_output([go, 'env', 'GOVERSION'], env=env, text=True).strip()
    if version != GO_VERSION:
        raise ValueError('expected pinned statistics toolchain '+GO_VERSION)
    with tempfile.TemporaryDirectory() as private:
        source = Path(private)/'source'
        verify_source(archive, repo, revision, source_hash, source)
        module = source/MODULE
        binary = Path(private)/BINARY
        subprocess.run([go, 'build', '-mod=readonly', '-trimpath', '-buildvcs=false',
                        '-ldflags=-s -w', '-o', binary, '.'], cwd=module, env=env, check=True)
        executable = regular(binary)
        notices_body = notices(go, env, source, module, binary)
        manifest = {
            'format': 1, 'revision': revision, 'source_archive_sha256': source_hash,
            'target': 'linux/arm64', 'go_version': version,
            'artifact_sha256': {BINARY: sha(executable), NOTICES: sha(notices_body)},
        }
        output.mkdir(mode=0o700)
        (output/BINARY).write_bytes(executable)
        (output/BINARY).chmod(0o555)
        (output/NOTICES).write_bytes(notices_body)
        (output/'build.json').write_text(json.dumps(manifest, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--go', default='go')
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build(args.go, args.archive, args.revision, args.source, args.output)
