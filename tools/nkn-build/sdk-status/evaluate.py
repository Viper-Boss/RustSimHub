#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Verify an existing SDK source stage, then test the actual read-only adapter."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
SPEC = importlib.util.spec_from_file_location('sdk_security', ROOT.parent / 'sdk-websocket-security/evaluate.py')
security = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(security)
base = security.base


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_tree(path, stage):
    assert path.is_relative_to(stage)
    for parent in [path, *path.parents]:
        assert not parent.is_symlink(), 'linked stage refused before traversal'
        if parent == stage:
            break
    return security.tree(path)


def verified_stage(stage, env, out):
    expected = {r['path']: r['sha256'] for r in base.manifest()}
    for directory in [base.ROOT, security.minimal.ROOT]:
        metadata = json.loads((directory / 'patch-manifest.json').read_text())
        expected.update({r['path']: r['after_sha256'] for r in metadata['files']})
    assert source_tree(stage / 'source' / 'third_party', stage) == {k.removeprefix('third_party/'): v for k, v in expected.items()}
    versions = json.loads((security.ROOT / 'versions.json').read_text())
    tag = next(r for r in versions['versions'] if r['version'] == 'v1.5.3')
    cached = json.loads(base.command(['go', 'mod', 'download', '-json', 'github.com/gorilla/websocket@v1.5.3'], ROOT, env, out, 'resolve-tag'))
    archive = Path(cached['Zip'])
    assert archive.is_relative_to(env['GOMODCACHE']) and sha(archive.read_bytes()) == tag['archive_sha256']
    assert cached['Sum'] == tag['module_sum'] and cached['GoModSum'] == tag['go_mod_sum']
    gorilla = {}
    with zipfile.ZipFile(archive) as source:
        assert len(source.infolist()) <= 200
        for member in source.infolist():
            name = member.filename.removeprefix('github.com/gorilla/websocket@v1.5.3/')
            assert name != member.filename and '..' not in Path(name).parts and not name.startswith('/')
            assert member.file_size <= 2 * 1024 * 1024
            if not member.is_dir():
                gorilla[name] = sha(source.read(member))
    patch = json.loads((security.ROOT / 'backport-manifest.json').read_text())
    for row in patch['files']:
        assert gorilla[row['path']] == row['before_sha256']
        gorilla[row['path']] = row['after_sha256']
    assert source_tree(stage / 'source' / 'gorilla-websocket', stage) == gorilla
    for name in ['go.mod', 'go.sum', 'sdk_test.go', 'minimal_test.go', 'mask_test.go']:
        assert not (stage / name).is_symlink() and (stage / name).is_file() and (stage / name).stat().st_size < 128 * 1024
    original = (base.ROOT / 'go.mod').read_text()
    module = original.replace('github.com/gorilla/websocket v1.4.2', 'github.com/gorilla/websocket v1.5.3')
    module += '\n// Verified upstream mask backport; not an unmodified official release.\nreplace github.com/gorilla/websocket => ./source/gorilla-websocket\n'
    sums = (base.ROOT / 'go.sum').read_text().splitlines()
    sums += ['github.com/gorilla/websocket v1.5.3 ' + tag['module_sum'], 'github.com/gorilla/websocket v1.5.3/go.mod ' + tag['go_mod_sum']]
    assert (stage / 'go.mod').read_text() == module
    assert (stage / 'go.sum').read_text() == '\n'.join(sorted(set(sums))) + '\n'
    for directory, name in [(base.ROOT, 'sdk_test.go'), (security.minimal.ROOT, 'minimal_test.go'), (security.ROOT, 'mask_test.go')]:
        assert (stage / name).read_bytes() == (directory / name).read_bytes()


def run(stage, out):
    env = dict(os.environ, GOWORK='off', GOENV='off', GOTOOLCHAIN='local', GOMAXPROCS='4',
               GOPROXY='off', GOSUMDB='off', CGO_ENABLED='1', RSH_NKN_PATCHED='1')
    for key in ['GOMODCACHE', 'GOCACHE', 'GOPATH']:
        assert key in env and Path(env[key]).is_absolute() and Path(env[key]).is_relative_to('/workspace')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', 'tools/nkn-engine', str(ROOT)], cwd=REPO, text=True).strip()
    inputs = {str(p.relative_to(REPO)): sha(p.read_bytes()) for directory in [REPO / 'tools/nkn-engine', ROOT]
              for p in directory.rglob('*') if p.is_file() and p.suffix in ['.go', '.py', '.mjs', '.mod']}
    verified_stage(stage, env, out)
    # Copy only independently verified source/templates; never copy a cached
    # executable, credentials, arbitrary extra source root or generated log.
    selected = out / 'stage'
    selected.mkdir(mode=0o700)
    for name in ['go.mod', 'go.sum', 'sdk_test.go', 'minimal_test.go', 'mask_test.go']:
        shutil.copyfile(stage / name, selected / name)
    shutil.copytree(stage / 'source' / 'third_party', selected / 'source' / 'third_party')
    shutil.copytree(stage / 'source' / 'gorilla-websocket', selected / 'source' / 'gorilla-websocket')
    engine = selected / 'source' / 'engine'
    for path in (REPO / 'tools/nkn-engine').rglob('*'):
        if not path.is_file() or path.suffix not in ['.go', '.mod']:
            continue
        assert not path.is_symlink() and path.stat().st_size <= 128 * 1024
        target = engine / path.relative_to(REPO / 'tools/nkn-engine')
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(path.read_bytes())
    with (selected / 'go.mod').open('a') as file:
        file.write('\nrequire github.com/Viper-Boss/RustSimHub/tools/nkn-engine v0.0.0\nreplace github.com/Viper-Boss/RustSimHub/tools/nkn-engine => ./source/engine\n')
    shutil.copyfile(ROOT / 'source_test.go', selected / 'source_test.go')
    version = base.command(['go', 'version'], selected, env, out, 'go-version').decode().strip()
    assert version.startswith('go version go1.27.3 ')
    base.command(['go', 'mod', 'verify'], selected, env, out, 'module-verify')
    runtime_env = dict(env, RSH_SDK_STATUS_OUTPUT=str(out / 'samples.json'))
    raw = base.command(['go', 'test', '-tags=nkn_sdk_candidate', '-race', '-mod=readonly', '-count=1', '-timeout=20s', '-v', '-run=^TestSDKSource', '.'],
                       selected, runtime_env, out, 'sdk-source-tests')
    assert raw.count(b'--- PASS: TestSDKSource') == 4
    module = 'github.com/Viper-Boss/RustSimHub/tools/nkn-engine'
    base.command(['go', 'test', '-tags=nkn_sdk_candidate', '-race', '-mod=readonly', '-count=1', '-timeout=20s', '-v', module + '/bridge', module + '/protocol'],
                 selected, env, out, 'bridge-protocol-tests')
    base.command(['go', 'vet', '-tags=nkn_sdk_candidate', '-mod=readonly', '.', module + '/sdkstatus', module + '/bridge'], selected, env, out, 'vet')
    base.command(['node', str(ROOT / 'check_projection.mjs'), str(out / 'samples.json')], REPO, env, out, 'production-js-projection')
    artifacts = []
    for arch, machine in [('amd64', 62), ('arm64', 183)]:
        binary = out / ('sdk-status-linux-' + arch + '.test')
        build_env = dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH=arch)
        base.command(['go', 'test', '-c', '-tags=nkn_sdk_candidate', '-mod=readonly', '-trimpath', '-buildvcs=false', '-o', str(binary), '.'], selected, build_env, out, 'build-' + arch)
        with binary.open('rb') as file:
            header = file.read(20)
        assert header[:6] == bytes([127, 69, 76, 70, 2, 1]) and struct.unpack('<H', header[18:20])[0] == machine
        assert binary.stat().st_size < 64 * 1024 * 1024
        if arch == 'amd64':
            base.command([str(binary), '-test.v', '-test.timeout=20s', '-test.run=^TestSDKSource'], selected, runtime_env, out, 'native-source-tests', seconds=25)
        artifacts.append({'architecture': arch, 'bytes': binary.stat().st_size, 'sha256': sha(binary.read_bytes()), 'executed': arch == 'amd64', 'kind': 'test probe, not engine release'})
    assert inputs == {str(p.relative_to(REPO)): sha(p.read_bytes()) for directory in [REPO / 'tools/nkn-engine', ROOT]
                      for p in directory.rglob('*') if p.is_file() and p.suffix in ['.go', '.py', '.mjs', '.mod']}
    assert head == subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
    assert dirty == subprocess.check_output(['git', 'status', '--porcelain', '--', 'tools/nkn-engine', str(ROOT)], cwd=REPO, text=True).strip()
    report = {'schema': 1, 'source_commit': head, 'candidate_sources_dirty': bool(dirty), 'input_sha256': inputs,
              'go': version, 'sdk_source_tests': 4, 'new_bridge_tests': 3, 'bridge_protocol_tests': 10,
              'production_js_samples': 2, 'artifacts': artifacts, 'production_enabled': False,
              'sdk_startup_implemented': False, 'core_authorized_provider_installed': False,
              'scope': 'real SDK read-only adapter/stdio projection; no owner/network/process readiness inferred'}
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'sdk_source_tests': 4, 'bridge_protocol_tests': 10, 'production_js_samples': 2, 'production_enabled': False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--verified-sdk-stage')
    parser.add_argument('--output')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'network': False, 'execute': False, 'production_enabled': False}))
        return 0
    try:
        if not args.verified_sdk_stage or not args.output:
            raise ValueError('verified SDK stage and NEW output under work/ required')
        out = base.safe_path(args.output)
        out.mkdir(mode=0o700)
        run(base.safe_path(args.verified_sdk_stage), out)
    except Exception as error:
        print('SDK status validation incomplete:', type(error).__name__, str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
