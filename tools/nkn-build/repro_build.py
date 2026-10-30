#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit offline, two clean builds per Linux architecture; never signs/installs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def run(arguments, cwd=ROOT, env=None, seconds=120):
    result = subprocess.run(arguments, cwd=cwd, env=env, capture_output=True, timeout=seconds, check=True)
    if len(result.stdout) > 2 * 1024 * 1024 or len(result.stderr) > 2 * 1024 * 1024:
        raise ValueError('tool_output_limit')
    return result.stdout


def output_directory(value):
    out = Path(value).absolute()
    if (not (out.is_relative_to('/workspace/artifacts') or out.is_relative_to('/tmp'))
            or any(p in {'..', '.private', '.local', '.git', '.ssh'} for p in out.parts)):
        raise ValueError('invalid_output')
    for parent in out.parents:
        if parent.is_symlink() or not parent.is_dir():
            raise ValueError('unsafe_output_parent')
    out.mkdir(mode=0o700)  # Deliberately no overwrite or automatic parent creation.
    return out


def build(out):
    if run(['git', 'status', '--porcelain', '--', 'tools/nkn-engine', 'tools/nkn-build']).strip():
        raise ValueError('uncommitted_build_sources')
    commit = run(['git', 'rev-parse', 'HEAD']).decode().strip()
    if not re.fullmatch('[a-f0-9]{40}', commit):
        raise ValueError('invalid_source_commit')
    rows = run(['git', 'ls-tree', '-r', '-l', commit, '--', 'tools/nkn-engine']).decode().splitlines()
    if not rows or len(rows) > 64:
        raise ValueError('source_file_count')
    sources = []
    for row in rows:
        metadata, name = row.split('\t', 1)
        mode, kind, blob, size = metadata.split()
        if (mode not in {'100644', '100755'} or kind != 'blob' or int(size) > 256 * 1024
                or not re.fullmatch(r'tools/nkn-engine/[A-Za-z0-9_./-]+', name) or '..' in name.split('/')):
            raise ValueError('unsupported_public_source')
        data = run(['git', 'show', commit + ':' + name])
        if len(data) != int(size):
            raise ValueError('source_size_changed')
        sources.append((name.removeprefix('tools/nkn-engine/'), data, blob))
    env = dict(os.environ, GOPROXY='off', GOSUMDB='off', GOWORK='off', GOENV='off',
               GOTOOLCHAIN='local', CGO_ENABLED='0', GOOS='linux', GOAMD64='v1', GOARM64='v8.0',
               GOFLAGS='', GOMAXPROCS='4')
    version = run(['go', 'version'], env=env).decode().strip()
    manifest = {'schema': 1, 'source_commit': commit, 'compiler': version, 'nkn_sdk_linked': False,
                'runtime_network_verified': False, 'signed': False, 'installed': False,
                'sources': [{'path': name, 'sha256': hashlib.sha256(data).hexdigest(), 'git_blob': blob}
                            for name, data, blob in sources], 'artifacts': []}
    with tempfile.TemporaryDirectory(prefix='rsh-nkn-build-') as scratch:
        for arch, machine in [('amd64', 62), ('arm64', 183)]:
            binaries = []
            for index in range(2):
                base = Path(scratch) / f'{arch}-{index}'
                module = base / 'source'
                module.mkdir(parents=True)
                for name, data, _ in sources:
                    target = module / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
                selected = dict(env, GOARCH=arch, GOCACHE=str(base / 'empty-cache'))
                target = base / 'nkn-engine'
                print(f'Building linux/{arch}, independent cache {index + 1}/2', flush=True)
                run(['go', 'build', '-p=4', '-trimpath', '-buildvcs=false',
                     '-ldflags=-buildid= -X main.sourceCommit=' + commit,
                     '-o', str(target), './cmd/nkn-engine'], cwd=module, env=selected)
                if target.stat().st_size > 16 * 1024 * 1024:
                    raise ValueError('artifact_size')
                data = target.read_bytes()
                if data[:6] != b'\x7fELF\x02\x01' or struct.unpack('<H', data[18:20])[0] != machine:
                    raise ValueError('wrong_elf_architecture')
                if str(ROOT).encode() in data or str(base).encode() in data:
                    raise ValueError('untrimmed_build_path')
                binaries.append((data, target))
            if binaries[0][0] != binaries[1][0]:
                raise ValueError('non_reproducible_artifact')
            filename = 'nkn-engine-linux-' + arch
            dest = out / filename
            shutil.copyfile(binaries[0][1], dest)
            dest.chmod(0o700)
            info = run(['go', 'version', '-m', str(dest)], env=env).decode()
            manifest['artifacts'].append({'path': filename, 'bytes': len(binaries[0][0]),
                'sha256': hashlib.sha256(binaries[0][0]).hexdigest(), 'elf_machine': machine,
                'two_clean_builds_equal': True, 'build_info': info.replace(str(dest), filename)})
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--output')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'execute': False, 'targets': ['linux/amd64', 'linux/arm64'],
                          'copies_per_target': 2, 'nkn_sdk_linked': False, 'sign_or_install': False}))
        return 0
    if not args.output:
        parser.error('--run requires a new --output directory')
    try:
        result = build(output_directory(args.output))
        print(json.dumps({'source_commit': result['source_commit'], 'artifacts': len(result['artifacts']),
                          'reproducible': True, 'nkn_sdk_linked': False}))
        return 0
    except (OSError, ValueError, subprocess.SubprocessError):
        print('Build incomplete; no runtime, signing or installation verdict.')
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
