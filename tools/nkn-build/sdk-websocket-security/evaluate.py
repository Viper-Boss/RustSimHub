#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compare real WebSocket masks and an upstream backport, without production wiring."""
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
SPEC = importlib.util.spec_from_file_location('sdk_minimal', ROOT.parent / 'sdk-minimal-evaluation/evaluate.py')
minimal = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(minimal)
base = minimal.base
MODULE = 'github.com/gorilla/websocket'
SDK_CASES = 'Test(Explicit|Inherited|Constructor|ClientWallet|CloseActive|Malformed|RPCResponse)'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree(path):
    result = {}
    for file in sorted(path.rglob('*')):
        assert not file.is_symlink()
        if file.is_file():
            assert file.stat().st_size <= 2 * 1024 * 1024
            result[str(file.relative_to(path))] = sha(file)
    assert len(result) <= 200 and sum(p.stat().st_size for p in path.rglob('*') if p.is_file()) <= 4 * 1024 * 1024
    return result


def backport(stage, env, out, original):
    data = (ROOT / 'upstream-mask-backport.patch').read_bytes()
    value = json.loads((ROOT / 'backport-manifest.json').read_text())
    assert value['production_approved'] is False and value['base_version'] == 'v1.5.3'
    assert value['upstream_fix_commit'] == 'd67f41855da42d7bccd9ef050c49f7e54e783b95'
    assert value['patch_sha256'] == hashlib.sha256(data).hexdigest()
    paths = ['conn.go', 'prepared_test.go']
    assert [r['path'] for r in value['files']] == paths
    assert re.findall(r'^\+\+\+ b/(.+)$', data.decode(), re.M) == paths
    assert re.findall(r'^--- a/(.+)$', data.decode(), re.M) == paths
    for row in value['files']:
        assert original[row['path']] == row['before_sha256']
    for dry in [True, False]:
        args = ['patch', '--batch', '--fuzz=0', '-p1', '-i', str(ROOT / 'upstream-mask-backport.patch')]
        if dry:
            args.append('--dry-run')
        base.command(args, stage, env, out, 'backport-check' if dry else 'backport-apply')
    expected = dict(original)
    expected.update({r['path']: r['after_sha256'] for r in value['files']})
    assert tree(stage) == expected
    return value, expected


def scan(stage, env, out, tool, name):
    raw = base.command([str(tool), '-C', str(stage), '-json', 'github.com/nknorg/nkn-sdk-go'],
                       stage, env, out, name + '-source-scan', seconds=120)
    rows = base.decode_stream(raw)
    config = next(r['config'] for r in rows if 'config' in r)
    assert config['scanner_version'] == 'v1.9.0' and config['scan_mode'] == 'source'
    sbom = next(r['SBOM'] for r in rows if 'SBOM' in r)
    replacements = {'./source/third_party/nkn-sdk-go': 'github.com/nknorg/nkn-sdk-go',
                    './source/third_party/ncp-go': 'github.com/nknorg/ncp-go',
                    './source/gorilla-websocket': MODULE}
    modules = {replacements.get(r['path'], r['path']) for r in sbom['modules']}
    assert sbom['roots'] == ['github.com/nknorg/nkn-sdk-go']
    assert {MODULE, 'github.com/nknorg/nkn-sdk-go', 'golang.org/x/crypto'} <= modules, 'incomplete source scan closure'
    findings = [r['finding'] for r in rows if 'finding' in r]
    return {'config': config, 'sbom_module_count': len(sbom['modules']), 'findings': findings,
            'symbol_ids': sorted({f['osv'] for f in findings if f['trace'][0].get('function')})}


def run(out, cache, scanner):
    inputs = {str(p.relative_to(ROOT.parent)): sha(p) for directory in [ROOT, minimal.ROOT, base.ROOT]
              for p in directory.iterdir() if p.suffix in ['.py', '.go', '.mod', '.sum', '.json', '.patch']}
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', str(ROOT), str(minimal.ROOT), str(base.ROOT)], cwd=ROOT, text=True).strip()
    sources = base.verified_cache(cache)
    env = dict(os.environ, GOWORK='off', GOENV='off', GOTOOLCHAIN='local', GOMAXPROCS='4',
               GOPROXY='off', GOSUMDB='off', CGO_ENABLED='1', RSH_NKN_PATCHED='1')
    for key in ['GOMODCACHE', 'GOCACHE', 'GOPATH']:
        assert key in env and Path(env[key]).is_absolute() and Path(env[key]).is_relative_to('/workspace')
    version = base.command(['go', 'version'], ROOT, env, out, 'go-version').decode().strip()
    assert version.startswith('go version go1.27.3 ')
    stage = out / 'baseline-stage'
    stage.mkdir(mode=0o700)
    for name, data in sources.items():
        path = stage / 'source' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    for name in ['go.mod', 'go.sum', 'sdk_test.go']:
        shutil.copyfile(base.ROOT / name, stage / name)
    old = base.apply_candidate(stage, env, out)
    base.verify_patched(stage, sources, old)
    cut = minimal.apply(stage, env, out)
    sdk_expected = {r['path']: r['after_sha256'] for r in [*old['files'], *cut['files']]}
    shutil.copyfile(minimal.ROOT / 'minimal_test.go', stage / 'minimal_test.go')
    shutil.copyfile(ROOT / 'mask_test.go', stage / 'mask_test.go')
    metadata = json.loads((ROOT / 'versions.json').read_text())
    assert metadata['module'] == MODULE and metadata['production_approved'] is False
    assert metadata['actual_candidate_commit'] == 'd67f41855da42d7bccd9ef050c49f7e54e783b95'
    versions = metadata['versions']
    assert [v['version'] for v in versions] == ['v1.4.2', 'v1.5.3', 'v1.5.4-0.20240701034025-d67f41855da4']
    # Offline, exact-version lookup. Every archive/notice and module sum is
    # checked before use; no download or mutation of the module cache occurs.
    resolved = {}
    for row in versions:
        value = json.loads(base.command(['go', 'mod', 'download', '-json', MODULE + '@' + row['version']],
                                        ROOT, env, out, 'resolve-' + row['version']))
        assert value['Sum'] == row['module_sum'] and value['GoModSum'] == row['go_mod_sum']
        directory = Path(value['Dir'])
        assert directory.is_relative_to(env['GOMODCACHE'])
        for file, key in [('conn.go', 'conn_go_sha256'), ('go.mod', 'go_mod_sha256'), ('LICENSE', 'license_sha256')]:
            assert sha(directory / file) == row[key]
        assert sha(Path(value['Zip'])) == row['archive_sha256']
        resolved[row['version']] = directory
    if scanner:
        assert scanner.is_file() and not scanner.is_symlink()
        tool_info = base.command(['go', 'version', '-m', str(scanner)], stage, env, out, 'scanner-build-info').decode()
        assert '\tmod\tgolang.org/x/vuln\tv1.9.0\t' in tool_info
    else:
        tool_info = None
    stages = {'baseline': stage}
    nominal = out / 'nominal-stage'
    shutil.copytree(stage, nominal)
    go_mod = (nominal / 'go.mod').read_text()
    assert go_mod.count(MODULE + ' v1.4.2') == 1
    (nominal / 'go.mod').write_text(go_mod.replace(MODULE + ' v1.4.2', MODULE + ' v1.5.3'))
    sums = (nominal / 'go.sum').read_text().splitlines()
    sums += [MODULE + ' v1.5.3 ' + versions[1]['module_sum'], MODULE + ' v1.5.3/go.mod ' + versions[1]['go_mod_sum']]
    (nominal / 'go.sum').write_text('\n'.join(sorted(set(sums))) + '\n')
    stages['nominal'] = nominal
    candidate = out / 'backport-stage'
    shutil.copytree(nominal, candidate)
    derivative = candidate / 'source' / 'gorilla-websocket'
    original = tree(resolved['v1.5.3'])
    shutil.copytree(resolved['v1.5.3'], derivative)
    # Go's cache is read-only. Change permissions only in this newly owned
    # derivative, never on the registry directory or its parent.
    derivative.chmod(0o700)
    for file in derivative.rglob('*'):
        file.chmod(0o700 if file.is_dir() else 0o600)
    patch, expected = backport(derivative, env, out, original)
    # Verify that the production mask function is precisely the upstream fix,
    # retaining the tag's unrelated code (including its self-contained proxy).
    def mask_function(file):
        text = file.read_text()
        return text[text.index('// maskRand is an io.Reader'):text.index('\nfunc hideTempErr')]
    assert mask_function(derivative / 'conn.go') == mask_function(resolved[versions[2]['version']] / 'conn.go')
    with (candidate / 'go.mod').open('a') as file:
        file.write('\n// Verified upstream mask backport; not an unmodified official release.\nreplace ' + MODULE + ' => ./source/gorilla-websocket\n')
    stages['backport'] = candidate
    scans = {}
    for name, selected in stages.items():
        selected_env = dict(env, RSH_WS_EXPECT_PREDICTABLE='0' if name == 'backport' else '1')
        base.command(['go', 'mod', 'verify'], selected, env, out, name + '-module-verify')
        # Only the isolated mask test enables legacy Seed semantics; all SDK
        # regressions/builds preserve the session's ordinary runtime settings.
        controlled = dict(selected_env, GODEBUG=env.get('GODEBUG', '') + ',randseednop=0')
        mask_log = base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v', '-run', '^TestActualClientFrameMaskSource$', '.'],
                                selected, controlled, out, name + '-mask-test')
        assert len(re.findall(rb'^--- PASS: TestActualClientFrameMaskSource ', mask_log, re.M)) == 1
        sdk_log = base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v', '-run', SDK_CASES, '.'],
                               selected, selected_env, out, name + '-sdk-tests')
        assert len(re.findall(rb'^--- PASS:', sdk_log, re.M)) == 7
        if scanner:
            scans[name] = scan(selected, selected_env, out, scanner, name)
        for path, data in sources.items():
            assert sha(selected / 'source' / path) == sdk_expected.get(path, hashlib.sha256(data).hexdigest())
    if scanner:
        assert 'GO-2026-6278' in scans['baseline']['symbol_ids']
        # A database range hides the tag's still-predictable mask behavior.
        # The real-frame test above, not this absence, checks the backport.
        assert 'GO-2026-6278' not in scans['nominal']['symbol_ids']
    stage = candidate
    base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=20s', '-v',
                  'github.com/nknorg/nkn-sdk-go', 'github.com/nknorg/ncp-go'], stage, env, out, 'upstream-sdk-tests')
    base.command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=20s', '-v', '-run', '^TestPreparedMessage$', MODULE],
                 stage, env, out, 'upstream-prepared-test')
    base.command(['go', 'vet', '-mod=readonly', '.', 'github.com/nknorg/nkn-sdk-go', MODULE], stage, env, out, 'vet')
    formatted = base.command(['gofmt', '-l', 'mask_test.go', 'source/gorilla-websocket/conn.go'],
                             stage, env, out, 'gofmt')
    assert not formatted.strip()
    graphs, artifacts = {}, []
    for arch, machine in [('amd64', 62), ('arm64', 183)]:
        selected_env = dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH=arch)
        before = minimal.inventory(stages['baseline'], selected_env, out, arch + '-baseline')
        graph = minimal.inventory(stage, selected_env, out, arch)
        previous = {m['path']: m for m in before['modules']}
        current = {m['path']: m for m in graph['modules']}
        assert previous.keys() == current.keys(), 'candidate changed the selected module set'
        for name in previous:
            if name != MODULE:
                assert previous[name] == current[name], 'unrelated dependency changed'
        assert not any(p.startswith(('github.com/pion/', 'golang.org/x/net/', 'github.com/nknorg/nkn/v2/api/webrtc')) for p in graph['packages'])
        graphs[arch] = graph
        binary = out / ('sdk-security-linux-' + arch + '.test')
        base.command(['go', 'test', '-c', '-mod=readonly', '-trimpath', '-buildvcs=false', '-o', str(binary), '.'], stage, selected_env, out, 'build-' + arch)
        with binary.open('rb') as file:
            header = file.read(20)
        assert header[:6] == b'\x7fELF\x02\x01' and struct.unpack('<H', header[18:20])[0] == machine
        assert binary.stat().st_size <= 64 * 1024 * 1024
        if arch == 'amd64':
            base.command([str(binary), '-test.v', '-test.timeout=15s', '-test.run=' + SDK_CASES], stage, env, out, 'native-sdk-tests', seconds=20)
            controlled = dict(env, GODEBUG=env.get('GODEBUG', '') + ',randseednop=0', RSH_WS_EXPECT_PREDICTABLE='0')
            base.command([str(binary), '-test.v', '-test.timeout=15s', '-test.run=^TestActualClientFrameMaskSource$'], stage, controlled, out, 'native-mask-test', seconds=20)
        artifacts.append({'architecture': arch, 'bytes': binary.stat().st_size, 'sha256': sha(binary), 'executed': arch == 'amd64', 'kind': 'test probe; NOT engine release'})
    assert tree(derivative) == expected
    assert tree(resolved['v1.5.3']) == original, 'registry cache changed'
    assert head == subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    assert dirty == subprocess.check_output(['git', 'status', '--porcelain', '--', str(ROOT), str(minimal.ROOT), str(base.ROOT)], cwd=ROOT, text=True).strip()
    assert inputs == {str(p.relative_to(ROOT.parent)): sha(p) for directory in [ROOT, minimal.ROOT, base.ROOT]
                      for p in directory.iterdir() if p.suffix in ['.py', '.go', '.mod', '.sum', '.json', '.patch']}
    report = {'schema': 1, 'source_commit': head, 'candidate_sources_dirty': bool(dirty), 'input_sha256': inputs,
              'go': version, 'versions': metadata, 'backport': patch, 'original_gorilla_files': original,
              'derivative_gorilla_files': expected, 'graphs': graphs, 'artifacts': artifacts, 'scans': scans,
              'scanner_build_info': tool_info, 'scanner_sha256': sha(scanner) if scanner else None,
              'mask_cases': 1, 'mask_versions_compared': 3, 'sdk_cases': 7, 'original_sdk_cases': 4, 'original_prepared_case': 1,
              'mask_observation': {'v1.4.2': '16 masks match controlled math/rand', 'v1.5.3': '16 masks match controlled math/rand',
                                   'v1.5.3+backport': 'does not match controlled math/rand'},
              'production_approved': False, 'engine_stdio_sdk_linked': False, 'arm_executed': False,
              'network_scope': 'owned net.Pipe and loopback only; scans only public package/version data when explicitly requested'}
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'mask_versions_compared': 3, 'sdk_cases': 7, 'backport_files': 2, 'production_approved': False}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--source-cache')
    parser.add_argument('--output')
    parser.add_argument('--online-scans', action='store_true')
    parser.add_argument('--govulncheck')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'network': False, 'execute': False, 'production_approved': False}))
        return 0
    try:
        if not args.source_cache or not args.output or bool(args.govulncheck) != args.online_scans:
            raise ValueError('explicit work/ cache and NEW output required; scanner requires --online-scans')
        out = base.safe_path(args.output)
        out.mkdir(mode=0o700)
        run(out, base.safe_path(args.source_cache), Path(args.govulncheck).absolute() if args.govulncheck else None)
    except Exception as error:
        print('WebSocket candidate evaluation incomplete:', type(error).__name__, str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
