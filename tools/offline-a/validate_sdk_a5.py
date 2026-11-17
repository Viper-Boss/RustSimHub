#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Rebuild the existing four-patch SDK verification chain without downloads."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import zipfile
from validate_a5 import Run, sha


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def suite(r, a):
    go = str(a.go.resolve())
    r.env.update(GOPATH=str(a.go_path.resolve()), GOMODCACHE=str(a.go_mod_cache.resolve()),
                 GOCACHE=str(a.go_cache.resolve()), CGO_ENABLED='1',
                 PATH=str(a.go.resolve().parent) + ':' + r.env['PATH'])
    assert r.run('go-version', [go, 'version']).strip() == 'go version go1.27.3 linux/amd64'
    root = r.source / 'tools/nkn-build'
    status = load('a5_fixed_status', root / 'sdk-status/evaluate.py')
    security = status.security; base = status.base
    stage = r.source / 'work/verified-base'; stage.mkdir(parents=True)
    original = base.verified_cache(a.sdk_cache.resolve())
    assert len(original) == 31
    for name, data in original.items():
        dest = stage / 'source' / name; dest.parent.mkdir(parents=True, exist_ok=True); dest.write_bytes(data)
    for name in ['go.mod', 'go.sum', 'sdk_test.go']:
        shutil.copyfile(base.ROOT / name, stage / name)
    compatibility = base.apply_candidate(stage, r.env, r.out)
    base.verify_patched(stage, original, compatibility)
    minimal = security.minimal.apply(stage, r.env, r.out)
    expected = {n: hashlib.sha256(data).hexdigest() for n, data in original.items()}
    for manifest in [compatibility, minimal]:
        for row in manifest['files']:
            expected[row['path']] = row['after_sha256']
    assert security.tree(stage / 'source/third_party') == {k.removeprefix('third_party/'): v for k, v in expected.items()}
    version = next(v for v in json.loads((security.ROOT / 'versions.json').read_text())['versions'] if v['version'] == 'v1.5.3')
    assert sha(a.websocket_zip) == version['archive_sha256']
    gorilla = stage / 'source/gorilla-websocket'; gorilla.mkdir()
    with zipfile.ZipFile(a.websocket_zip) as z:
        assert len(z.infolist()) <= 200
        for item in z.infolist():
            name = item.filename.removeprefix('github.com/gorilla/websocket@v1.5.3/')
            assert name != item.filename and not name.startswith('/') and '..' not in Path(name).parts and item.file_size < 2*1024*1024
            dest = gorilla / name
            if item.is_dir():
                dest.mkdir(parents=True, exist_ok=True)
            else:
                dest.parent.mkdir(parents=True, exist_ok=True); dest.write_bytes(z.read(item))
    mask, _ = security.backport(gorilla, r.env, r.out, security.tree(gorilla))
    module = (base.ROOT / 'go.mod').read_text().replace('github.com/gorilla/websocket v1.4.2', 'github.com/gorilla/websocket v1.5.3')
    module += '\n// Verified upstream mask backport; not an unmodified official release.\nreplace github.com/gorilla/websocket => ./source/gorilla-websocket\n'
    (stage / 'go.mod').write_text(module)
    sums = (base.ROOT / 'go.sum').read_text().splitlines()
    sums += ['github.com/gorilla/websocket v1.5.3 ' + version['module_sum'],
             'github.com/gorilla/websocket v1.5.3/go.mod ' + version['go_mod_sum']]
    (stage / 'go.sum').write_text('\n'.join(sorted(set(sums))) + '\n')
    for directory, name in [(security.minimal.ROOT, 'minimal_test.go'), (security.ROOT, 'mask_test.go')]:
        shutil.copyfile(directory / name, stage / name)
    # This independently checks the fixed fork, both SDK patches and WebSocket
    # archive/module sums and backport tree. Missing cache is a failure, not skip.
    status.verified_stage(stage, r.env, r.out)
    prepared = r.source / 'work/managed'
    r.run('managed-prepare', ['python3', str(root / 'sdk-managed/prepare.py'),
          '--verified-sdk-stage', str(stage), '--output', str(prepared)])
    managed = prepared / 'stage'
    inputs = json.loads((prepared / 'inputs.json').read_text())
    assert inputs['head'] == r.report['source']['head']
    for path in (r.source / 'tools/nkn-engine').rglob('*'):
        if path.is_file() and path.suffix in ['.go', '.mod']:
            assert (managed / 'source/engine' / path.relative_to(r.source / 'tools/nkn-engine')).read_bytes() == path.read_bytes()
    assert (managed / 'source/nkn-build/usage-vectors/vectors.json').read_bytes() == (root / 'usage-vectors/vectors.json').read_bytes()
    r.report['fixed_chain'] = {'source_files': 31, 'compatibility_patch': compatibility['patch_sha256'],
                              'minimal_patch': minimal['patch_sha256'], 'mask_patch': mask['patch_sha256'],
                              'managed_patch': sha(root / 'sdk-managed/managed.patch'), 'vectors_exact': True}
    locks = {n: sha(managed / n) for n in ['go.mod', 'go.sum']}
    r.run('modules-verify', [go, 'mod', 'verify'], cwd=managed)
    text = r.run('managed-regressions', [go, 'test', '-race', '-mod=readonly', '-count=1', '-timeout=45s',
                                       '-v', '-run', '^TestManaged', 'github.com/nknorg/nkn-sdk-go'], cwd=managed)
    assert '--- PASS: TestManaged' in text and '\nPASS\n' in text
    engine = 'github.com/Viper-Boss/RustSimHub/tools/nkn-engine'
    for label, tags in [('default', 'nkn_sdk_candidate'), ('transaction', 'nkn_sdk_candidate,nkn_usage_transaction_candidate')]:
        raw = r.run(label + '-race', [go, 'test', '-race', '-mod=readonly', '-tags='+tags,
                    '-count=1', '-timeout=90s', '-json', engine+'/...'], cwd=managed)
        events = [json.loads(line) for line in raw.splitlines() if line.startswith('{')]
        passed = [e for e in events if e.get('Action') == 'pass' and 'Test' in e]
        assert passed and not any(e.get('Action') == 'fail' for e in events)
        r.report.setdefault('go_test_counts', {})[label] = len(passed)
        r.run(label + '-vet', [go, 'vet', '-mod=readonly', '-tags='+tags, engine+'/...'], cwd=managed)
    for label, package, tags in [('sdk', engine+'/cmd/nkn-sdk-engine', 'nkn_sdk_candidate,nkn_usage_transaction_candidate'), ('root', engine+'/cmd/nkn-root', '')]:
        dest = r.native / (label + '-arm64')
        argv = [go, 'build', '-mod=readonly', '-trimpath']
        if tags:
            argv += ['-tags='+tags]
        r.run(label+'-arm64', [*argv, '-o', str(dest), package], cwd=managed,
              extra={'GOOS': 'linux', 'GOARCH': 'arm64', 'CGO_ENABLED': '0'})
        data = dest.read_bytes(); assert data[:4] == b'\x7fELF' and int.from_bytes(data[18:20], 'little') == 183
        r.report.setdefault('arm_compile_only', []).append({'name': label, 'sha256': sha(dest), 'executed': False})
    assert all(sha(managed / name) == value for name, value in locks.items())
    r.report.update(locks_unchanged=True, passed=True)
    r.save()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['repo', 'output', 'work-root', 'cargo-target', 'go', 'sdk-cache', 'websocket-zip', 'go-path', 'go-mod-cache', 'go-cache']:
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--node', default='node')
    a = p.parse_args(); a.profile = 'a2'
    run = None
    try:
        run = Run(a)
        suite(run, a)
    except Exception as error:
        if run is not None:
            run.report['error'] = type(error).__name__ + ': ' + str(error)
            run.save()
        raise


if __name__ == '__main__':
    main()
