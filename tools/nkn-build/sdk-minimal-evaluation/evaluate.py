#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Evaluate a checksum-chained WebSocket-only SDK derivative, only on loopback."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent / 'sdk-evaluation'
spec = importlib.util.spec_from_file_location('sdk_compatibility', BASE / 'evaluate.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metadata():
    value = json.loads((ROOT / 'patch-manifest.json').read_text())
    data = (ROOT / 'websocket-only.patch').read_bytes()
    assert value['source_commit'] == base.COMMIT and value['production_approved'] is False
    assert value['base_patch_sha256'] == digest(BASE / 'sdk-compatibility.patch')
    assert value['patch_sha256'] == hashlib.sha256(data).hexdigest()
    paths = ['third_party/nkn-sdk-go/client.go', 'third_party/nkn-sdk-go/config.go']
    assert [r['path'] for r in value['files']] == paths
    assert re.findall(r'^\+\+\+ b/(.+)$', data.decode(), re.M) == paths
    assert re.findall(r'^--- a/(.+)$', data.decode(), re.M) == paths
    return value


def apply(stage, env, out):
    value = metadata()
    for row in value['files']:
        assert digest(stage / 'source' / row['path']) == row['before_sha256']
    for dry in [True, False]:
        args = ['patch', '--batch', '--fuzz=0', '-p1', '-i', str(ROOT / 'websocket-only.patch')]
        if dry:
            args.append('--dry-run')
        base.command(args, stage / 'source', env, out, 'minimal-patch-check' if dry else 'minimal-patch-apply')
    return value


def inventory(stage, env, out, name):
    packages = base.decode_stream(base.command(['go', 'list', '-mod=readonly', '-deps', '-test', '-json', '.'],
                                               stage, env, out, name + '-packages'))
    modules = {}
    paths = set()
    for package in packages:
        paths.add(package['ImportPath'])
        module = package.get('Module')
        if module and not module.get('Main'):
            modules[module['Path']] = module
    rows = []
    for path, module in sorted(modules.items()):
        replaced = module.get('Replace')
        directory = Path((replaced or module)['Dir'])
        assert directory.is_relative_to(stage) or directory.is_relative_to(env['GOMODCACHE'])
        notices = []
        for file in sorted(directory.iterdir()):
            if file.is_file() and file.name.upper().startswith(('LICENSE', 'COPYING', 'COPYRIGHT', 'UNLICENSE')):
                assert not file.is_symlink() and file.stat().st_size <= 128 * 1024
                notices.append({'path': file.name, 'sha256': digest(file)})
        rows.append({'path': path, 'version': module['Version'], 'sum': module.get('Sum'),
                     'go_mod_sum': module.get('GoModSum'), 'selected_by_probe': True,
                     'source': 'checksum-pinned-local-fork' if replaced else 'Go-module', 'license_files': notices})
    return {'packages': sorted(paths), 'modules': rows}


def run(out, cache, online):
    own = ['evaluate.py', 'test_candidate.py', 'minimal_test.go', 'patch-manifest.json', 'websocket-only.patch']
    previous = ['evaluate.py', 'sdk_test.go', 'test_sources.py', 'go.mod', 'go.sum',
                'source-manifest.json', 'sdk-compatibility.patch', 'patch-manifest.json']
    inputs = {str(p.relative_to(ROOT.parent)): digest(p)
              for p in [*(ROOT / n for n in own), *(BASE / n for n in previous)]}
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', str(ROOT), str(BASE)], cwd=ROOT, text=True).strip()
    sources = base.verified_cache(cache)
    env = dict(os.environ, GOWORK='off', GOENV='off', GOTOOLCHAIN='local', GOMAXPROCS='4',
               GOPROXY='off', GOSUMDB='off', CGO_ENABLED='1', RSH_NKN_PATCHED='1')
    for key in ['GOMODCACHE', 'GOCACHE', 'GOPATH']:
        assert key in env and Path(env[key]).is_absolute() and Path(env[key]).is_relative_to('/workspace')
    stage = out / 'compatibility-stage'
    stage.mkdir(mode=0o700)
    for name, data in sources.items():
        path = stage / 'source' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    for name in ['go.mod', 'go.sum', 'sdk_test.go']:
        shutil.copyfile(BASE / name, stage / name)
    version = base.command(['go', 'version'], stage, env, out, 'go-version').decode().strip()
    assert version.startswith('go version go1.27.3 ')
    base.command(['go', 'mod', 'verify'], stage, env, out, 'module-verify')
    base.command([sys.executable, '-m', 'unittest', '-v', 'test_candidate.py'], ROOT, env, out, 'candidate-tests')
    old = base.apply_candidate(stage, env, out)
    base.verify_patched(stage, sources, old)
    base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v', '.'], stage, env, out, 'compatibility-tests')
    before = inventory(stage, dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH='amd64'), out, 'compatibility')
    minimal = out / 'minimal-stage'
    shutil.copytree(stage, minimal)
    change = apply(minimal, env, out)
    shutil.copyfile(ROOT / 'minimal_test.go', minimal / 'minimal_test.go')
    expected = {r['path']: r['after_sha256'] for r in [*old['files'], *change['files']]}
    for name, data in sources.items():
        assert digest(minimal / 'source' / name) == expected.get(name, hashlib.sha256(data).hexdigest())
    stage = minimal
    formatted = base.command(['gofmt', '-l', 'source/third_party/nkn-sdk-go/client.go',
                              'source/third_party/nkn-sdk-go/config.go', 'minimal_test.go'], stage, env, out, 'gofmt')
    assert not formatted.strip(), 'candidate Go source is not formatted'
    base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v', '.'], stage, env, out, 'minimal-tests')
    base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v',
                  'github.com/nknorg/nkn-sdk-go', 'github.com/nknorg/ncp-go'], stage, env, out, 'upstream-tests')
    base.command(['go', 'vet', '-mod=readonly', '.', 'github.com/nknorg/nkn-sdk-go', 'github.com/nknorg/ncp-go'], stage, env, out, 'vet')
    after = {}
    artifacts = []
    for arch, machine in [('amd64', 62), ('arm64', 183)]:
        selected = dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH=arch)
        graph = inventory(stage, selected, out, 'minimal-' + arch)
        assert not any(p.startswith(('github.com/pion/', 'github.com/nknorg/nkn/v2/api/webrtc')) for p in graph['packages'])
        after[arch] = graph
        binary = out / ('sdk-minimal-linux-' + arch + '.test')
        base.command(['go', 'test', '-c', '-mod=readonly', '-trimpath', '-buildvcs=false', '-o', str(binary), '.'], stage, selected, out, 'build-' + arch)
        with binary.open('rb') as stream:
            header = stream.read(20)
        assert header[:6] == b'\x7fELF\x02\x01' and struct.unpack('<H', header[18:20])[0] == machine
        assert binary.stat().st_size <= 64 * 1024 * 1024
        if arch == 'amd64':
            base.command([str(binary), '-test.v', '-test.timeout=15s'], stage, env, out, 'native-artifact-tests', seconds=20)
        artifacts.append({'path': binary.name, 'bytes': binary.stat().st_size, 'sha256': digest(binary),
                          'architecture': arch, 'kind': 'test probe; NOT an engine release', 'executed': arch == 'amd64'})
    for name, data in sources.items():
        assert digest(stage / 'source' / name) == expected.get(name, hashlib.sha256(data).hexdigest())
    assert head == subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    assert dirty == subprocess.check_output(['git', 'status', '--porcelain', '--', str(ROOT), str(BASE)], cwd=ROOT, text=True).strip()
    assert inputs == {str(p.relative_to(ROOT.parent)): digest(p)
                      for p in [*(ROOT / n for n in own), *(BASE / n for n in previous)]}
    advisories = base.advisories(after['amd64']['modules'], out) if online else {'status': 'not_run'}
    report = {'schema': 1, 'source_commit': head, 'candidate_sources_dirty': bool(dirty), 'input_sha256': inputs,
              'upstream_commit': base.COMMIT, 'upstream_files_verified': len(sources), 'go': version,
              'base_patch': old, 'minimal_patch': change, 'before': before, 'after': after,
              'artifacts': artifacts, 'advisories': advisories,
              'compatibility_cases': 5, 'minimal_cases': 7, 'upstream_cases': 4,
              'production_approved': False, 'engine_stdio_sdk_linked': False, 'arm_executed': False,
              'network_scope': 'synthetic owned loopback only; other dial refused before DNS/socket',
              'inventory_scope': 'compiled test closure, not full historical module graph; manifests unchanged'}
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'before_modules': len(before['modules']), 'after_modules': len(after['amd64']['modules']),
                      'minimal_cases': 7, 'production_approved': False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--source-cache')
    parser.add_argument('--output')
    parser.add_argument('--online-advisories', action='store_true')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'network': False, 'execute': False, 'production_approved': False}))
        return 0
    try:
        if not args.source_cache or not args.output:
            raise ValueError('explicit verified source cache and NEW output under work/ required')
        cache, out = base.safe_path(args.source_cache), base.safe_path(args.output)
        out.mkdir(mode=0o700)
        run(out, cache, args.online_advisories)
    except Exception as error:
        print('Minimal SDK evaluation incomplete:', type(error).__name__, str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
