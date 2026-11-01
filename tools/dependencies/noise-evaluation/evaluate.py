#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Default plan only. Explicit --run builds offline and tests an isolated candidate."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
from inventory import collect, digest, query_osv, verify_sources

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]


def command(args, name, out, seconds=180, cwd=ROOT, expected=0):
    completed = subprocess.run(args, cwd=cwd, capture_output=True, timeout=seconds)
    assert len(completed.stdout) + len(completed.stderr) <= 8 * 1024 * 1024
    (out / (name + '.log')).write_bytes(completed.stdout + completed.stderr)
    assert completed.returncode == expected, name + ' failed; see isolated log'
    return completed.stdout


def run(out, online=False):
    source = command(['git', 'rev-parse', 'HEAD'], 'source', out).decode().strip()
    dirty = command(['git', 'status', '--porcelain', '--', str(ROOT)], 'status', out).decode().strip()
    command(['cargo', 'test', '--locked', '--offline'], 'native-tests', out)
    command(['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'], 'clippy', out)
    command(['cargo', 'fmt', '--all', '--', '--check'], 'fmt', out)
    command([sys.executable, '-m', 'unittest', '-v', 'test_inventory.py'], 'source-check-tests', out)
    command(['cargo', 'build', '--release', '--locked', '--offline'], 'native-build', out)
    command(['cargo', 'build', '--lib', '--release', '--target', 'wasm32-unknown-unknown', '--locked', '--offline'], 'wasm-build', out)
    version = command(['wasm-bindgen', '--version'], 'bindgen-version', out).decode().strip()
    assert version == 'wasm-bindgen 0.2.130', 'mismatched binding generator'
    pkg = out / 'pkg'
    command(['wasm-bindgen', '--target', 'web', '--no-typescript', '--out-name', 'noise', '--out-dir', str(pkg),
             str(ROOT / 'target/wasm32-unknown-unknown/release/rustsimhub_noise_evaluation.wasm')], 'bindgen', out)
    command(['cargo', 'check', '--locked', '--offline'], 'minimal-feature-probe', out,
            cwd=ROOT / 'fixtures/minimal-features', expected=101)
    failure = (out / 'minimal-feature-probe.log').read_text()
    assert 'error[E0599]' in failure and 'KeyInit' in failure and 'snow-0.10.0' in failure
    inventory = collect()
    provenance = verify_sources()
    if online:
        try:
            (out / 'osv-response.json').write_bytes(query_osv(inventory))
        except Exception as error:
            inventory['advisory_check'] = {'status': 'unavailable', 'error_class': type(error).__name__}
    (out / 'dependencies.json').write_text(json.dumps(inventory, indent=2) + '\n')
    from browser_check import check
    binary = ROOT / 'target/release/rustsimhub-noise-evaluation'
    browser = check(binary, pkg)
    artifacts = [{'path': p.name, 'bytes': p.stat().st_size, 'sha256': digest(p)} for p in sorted(pkg.iterdir()) if p.is_file()]
    # This checks the existing T5 size ceilings, without adding any file to the PWA.
    assert all(a['bytes'] <= 512 * 1024 for a in artifacts)
    assert sum(a['bytes'] for a in artifacts) <= 2 * 1024 * 1024
    output = {'schema': 1, 'source_commit': source, 'candidate_sources_dirty': bool(dirty),
              'suite_evaluated': 'Noise_XX_25519_AESGCM_SHA256', 'browser': browser,
              'artifacts': artifacts, 'native_sha256': digest(binary),
              'minimal_features_compile': False, 'extra_evaluation_feature': 'use-chacha20poly1305',
              'production_ready': False, 'crypto_wire_frozen': False,
              'pin_grant_core_protocol_implemented': False,
              'dependencies_sha256': digest(out / 'dependencies.json')}
    output['source_provenance'] = provenance
    (out / 'report.json').write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps({'passed_browser_checks': browser['passed'], 'artifacts': artifacts,
                      'production_ready': False, 'advisory_status': inventory['advisory_check']['status']}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--output')
    parser.add_argument('--online-advisories', action='store_true')
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'execute': False, 'network': False, 'production_ready': False}))
        return 0
    if not args.output:
        parser.error('--run requires a new --output under the repository work/ directory')
    out = Path(args.output).absolute()
    work = REPO / 'work'
    if '..' in out.parts or not out.is_relative_to(work):
        parser.error('output must be under work/')
    for p in [out, *out.parents]:
        if p.is_symlink():
            parser.error('linked output refused')
    out.mkdir(mode=0o700)
    try:
        run(out, args.online_advisories)
    except Exception as error:
        print('Evaluation incomplete:', type(error).__name__, str(error), file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
