#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Finite offline product/WASM matrix. Reports never authorize installation.

Builds reuse the original WASM producer, browser, session peer and production
chain. This tool has no device, signing, epoch, service or remote-shell action.
"""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
PROFILES = Path(__file__).with_name('profiles.json')
MODES = tuple(row['mode'] for row in json.loads(PROFILES.read_text())['profiles'])


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def call(argv, **kwargs):
    return subprocess.run([str(a) for a in argv], check=True,
                          timeout=kwargs.pop('timeout', 180), **kwargs)


def identity(revision):
    if not re.fullmatch('[0-9a-f]{40}', revision):
        raise ValueError('full frozen source commit required')
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if actual != revision or subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT):
        raise ValueError('clean checkout of the exact frozen source required')
    archive = subprocess.check_output(['git', 'archive', '--format=tar', revision], cwd=ROOT)
    return {'revision': revision, 'source_archive_sha256': hashlib.sha256(archive).hexdigest()}


def fresh(path):
    path = path.absolute()
    if not path.is_relative_to(ROOT / 'work') or path.exists() or any(p.is_symlink() for p in [path, *path.parents]):
        raise ValueError('new nonsymlink workspace work output required')
    return path


def ordinary(path):
    path = path.absolute()
    if not path.is_file() or any(p.is_symlink() for p in [path, *path.parents]):
        raise ValueError('regular nonsymlink artifact required')
    return path


def save(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + '\n')


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def modes(programs, provenance):
    core, worker, launcher = [ordinary(programs / name) for name in
                              ('rustsimhub-core', 'rustsimhub-forwarder', 'rustsimhub-core-launcher')]
    infos = {}
    for name, program in [('core', core), ('worker', worker), ('launcher', launcher)]:
        info = json.loads(call([program, '--build-info'], capture_output=True, timeout=5).stdout)
        if info.get('source_archive_sha256') != provenance['source_archive_sha256'] or info.get('development_fixtures') is not False:
            raise ValueError('mixed source or fixture program refused')
        infos[name] = info
    if infos['core'].get('forwarding_runtime_contract') != infos['worker'].get('forwarding_runtime_contract') or infos['worker'].get('worker') is not True:
        raise ValueError('actual Core/worker protocol generations differ')
    base = json.loads(call([core, '--product-runtime-compatibility', 'legacy-v1'], capture_output=True, timeout=5).stdout)
    features = base['features']
    cases = []
    for row in json.loads(PROFILES.read_text())['profiles']:
        supported = all(features.get(f) is True for f in row['requires']) and not any(features.get(f) for f in row.get('excludes', []))
        result = subprocess.run([str(core), '--product-runtime-compatibility', row['mode']], capture_output=True, timeout=5)
        if supported:
            if result.returncode != 0:
                raise ValueError('supported actual producer mode failed: ' + row['mode'])
            info = json.loads(result.stdout)
            if (info['mode'] != row['mode'] or info['source_archive_sha256'] != provenance['source_archive_sha256'] or
                    info['business']['write'] != row['business_schema'] or info['business']['automatic_migration'] or
                    info['opens_database'] or info['starts_services'] or info['authorization_verified']):
                raise ValueError('actual mode contract mismatch')
        else:
            if result.returncode != 4 or b'unsupported product runtime mode' not in result.stderr:
                raise ValueError('unsupported mode did not fail through original producer')
            info = None
        cases.append({'mode': row['mode'], 'supported': supported, 'exit': result.returncode, 'metadata': info})
    return {'actual_program_metadata': infos, 'cases': cases, 'program_sha256':
            {p.name: sha(p) for p in [core, worker, launcher]}, 'report_not_authority': True}


def wasm(args, provenance):
    output = fresh(args.output)
    bindgen = ordinary(args.wasm_bindgen)
    if call([bindgen, '--version'], capture_output=True, text=True).stdout.strip() != 'wasm-bindgen 0.2.129':
        raise ValueError('locked wasm-bindgen 0.2.129 required')
    if shutil.disk_usage(ROOT).free < 400 * 1024**2:
        raise ValueError('400MiB free workspace reserve required before build')
    output.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target'), CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
               RUSTSIMHUB_BUILD_SOURCE_SHA256=provenance['source_archive_sha256'],
               CARGO_PROFILE_RELEASE_OPT_LEVEL='z', CARGO_PROFILE_RELEASE_PANIC='abort')
    # The existing producer also assembles the actual product site and CSP.
    call([sys.executable, ROOT / 'tools/build-recovery-configurator.py', '--output', output,
          '--wasm-bindgen', bindgen], env=env, timeout=300)
    if shutil.disk_usage(ROOT).free < 400 * 1024**2:
        raise ValueError('native peer not started: workspace reserve exhausted')
    native_env = dict(env, CARGO_PROFILE_RELEASE_PANIC='unwind')
    with (output / 'native-peer-build.log').open('wb') as log:
        call(['cargo', 'build', '--release', '--locked', '--offline', '-p', 'rustsimhub-recovery-session',
              '--features', 'dev-fixtures', '--example', 'session-interop'], env=native_env,
             cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    peer = ROOT / 'target/release/examples/session-interop'
    call([sys.executable, ROOT / 'tools/run-web-recovery-session.py', '--binary', peer,
          '--wasm', output / 'wasm', '--output', output / 'browser-interop'], timeout=180)
    call([sys.executable, ROOT / 'tools/run-product-configurator.py', '--site', output / 'site',
          '--native-peer', peer, '--output', output / 'browser-product'], timeout=180)
    report = {**provenance, 'kind': 'actual-wasm-native-browser-matrix', 'built': True,
              'browser_executed': True, 'wasm_bindgen_version': '0.2.129',
              'native_peer_sha256': sha(peer), 'wasm_sha256': sha(output / 'wasm/rustsimhub_recovery_session_bg.wasm'),
              'wasm_js_sha256': sha(output / 'wasm/rustsimhub_recovery_session.js'),
              'interop': json.loads((output / 'browser-interop/browser.json').read_text()),
              'product': json.loads((output / 'browser-product/report.json').read_text()),
              'physical_radio': False, 'core_business_authorization_tested': False,
              'published': False, 'report_not_authority': True}
    if report['interop']['passed'] < 18 or report['product']['native_crypto']['passed'] < 18:
        raise ValueError('actual cross-language checks missing')
    save(output / 'matrix-result.json', report)
    return report


def chain(args, provenance):
    output = fresh(args.output)
    output.mkdir(parents=True, mode=0o700)
    env = dict(os.environ)
    for key in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH'):
        env.pop(key, None)
    # Original runner executes the actual installer/guards/old migrations/Core.
    command = [sys.executable, ROOT / 'tools/product-integration/production_chain.py', '--mode', 'native',
               '--scenario', 'email-recovery', '--programs', args.programs, '--test-driver', args.test_driver,
               '--legacy-driver', args.legacy_driver, '--matrix-worker4', args.worker4,
               '--matrix-no-email-tests', args.no_email_tests, '--revision', args.revision,
               '--runtime-image', args.runtime_image]
    actual_modes = modes(args.programs, provenance)
    completed = call(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=360)
    entry = json.loads(completed.stdout.splitlines()[-1])
    original = json.loads((ROOT / entry['evidence'] / 'result.json').read_text())
    if not entry['passed'] or not original['passed']:
        raise ValueError('original isolated chain did not pass')
    report = {**provenance, 'kind': 'original-email4-then-recovery9', 'result': original,
              'mode_matrix': actual_modes, 'chain_log_sha256': sha(ROOT / entry['evidence'] / 'chain.log'),
              'evidence': entry['evidence'], 'automatic_health': 'unsupported, real refusal asserted',
              'interruption': 'original Snapshot both writers held after committed email4; SIGKILL, inspect and explicit fresh recovery',
              'hardware_tested': False, 'report_not_authority': True}
    save(output / 'matrix-result.json', report)
    return report


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--revision', required=True)
    sub = p.add_subparsers(dest='command', required=True)
    a = sub.add_parser('modes'); a.add_argument('--programs', type=Path, required=True); a.add_argument('--output', type=Path, required=True)
    a = sub.add_parser('wasm'); a.add_argument('--wasm-bindgen', type=Path, required=True); a.add_argument('--output', type=Path, required=True)
    a = sub.add_parser('email-recovery')
    for name in ('programs', 'test-driver', 'legacy-driver', 'worker4', 'no-email-tests', 'output'):
        a.add_argument('--' + name, type=Path, required=True)
    a.add_argument('--runtime-image', required=True)
    args = p.parse_args()
    try:
        provenance = identity(args.revision)
        if args.command == 'wasm': result = wasm(args, provenance)
        elif args.command == 'email-recovery': result = chain(args, provenance)
        else:
            result = {**provenance, **modes(args.programs, provenance)}
            out = fresh(args.output); out.mkdir(parents=True, mode=0o700); save(out / 'matrix-result.json', result)
    except (ValueError, OSError, KeyError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        p.exit(1, 'Matrix refused: ' + str(error) + '\n')
    print(json.dumps({'passed': True, 'command': args.command, 'revision': result['revision'],
                      'report_not_authority': True}, sort_keys=True))


if __name__ == '__main__':
    main()
