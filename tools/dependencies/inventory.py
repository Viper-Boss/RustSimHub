#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Offline inventory of public lockfiles; no builds, downloads or license verdict."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MAX_BYTES = 4 * 1024 * 1024
CARGO_MANIFESTS = ('Cargo.toml', 'native/audio-alsa/Cargo.toml')


def read_public(root, name):
    path = root / name
    if not path.is_relative_to(root) or '..' in Path(name).parts:
        raise ValueError('path outside public input')
    for part in [path, *path.parents]:
        if part == root.parent:
            break
        if part.is_symlink():
            raise ValueError('linked input refused')
    if not path.is_file() or path.stat().st_size > MAX_BYTES:
        raise ValueError('missing, nonregular or oversized input')
    data = path.read_bytes()
    if len(data) > MAX_BYTES:
        raise ValueError('input grew')
    return data


def cargo_metadata(root, manifest):
    result = subprocess.run(['cargo', 'metadata', '--manifest-path', str(root / manifest),
                             '--locked', '--offline', '--all-features', '--format-version', '1'],
                            cwd=root, capture_output=True, timeout=30, check=False)
    if result.returncode or len(result.stdout) > MAX_BYTES:
        raise ValueError('offline Cargo metadata unavailable')
    return json.loads(result.stdout)


def cargo_inventory(lock, metadata):
    packages = lock.get('package')
    if lock.get('version') not in (3, 4) or not isinstance(packages, list) or len(packages) > 2000:
        raise ValueError('unsupported or excessive Cargo lock')
    declared = {}
    direct = set()
    if metadata is not None:
        members = set(metadata['workspace_members'])
        for node in (metadata.get('resolve') or {}).get('nodes', []):
            if node['id'] in members:
                direct.update(node['dependencies'])
        declared = {(p['name'], p['version'], p.get('source')): p for p in metadata['packages']}
    rows = []
    seen = set()
    for package in packages:
        name, version, source = package['name'], package['version'], package.get('source')
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,100}', name) or not re.fullmatch(r'[A-Za-z0-9.+-]{1,100}', version):
            raise ValueError('invalid package identity')
        key = (name, version, source)
        if key in seen:
            raise ValueError('duplicate locked package')
        seen.add(key)
        checksum = package.get('checksum')
        if source and source.startswith('registry+') and (not isinstance(checksum, str) or not re.fullmatch('[0-9a-f]{64}', checksum)):
            raise ValueError('registry checksum missing or invalid')
        actual = declared.get(key, {})
        license_id = actual.get('license')
        if not isinstance(license_id, str) or not re.fullmatch(r'[A-Za-z0-9 .()+/\-]{1,256}', license_id):
            license_id = None
        rows.append({'name': name, 'version': version,
                     'source': 'workspace' if source is None else 'registry' if source.startswith('registry+') else 'git_or_other',
                     'checksum': checksum, 'license_declaration': license_id,
                     'license_evidence': 'Cargo package declaration' if license_id else 'unknown',
                     'relationship': 'workspace' if source is None else 'direct' if actual.get('id') in direct else 'transitive' if actual else 'unknown'})
    return rows


def go_inventory(mod_text, sum_text):
    """Read locked facts without running upstream Go code or resolving replacements."""
    requires, replacements, sums = [], [], {}
    block = None
    for raw in mod_text.splitlines():
        text = raw.split('//', 1)[0].strip()
        if not text:
            continue
        if text == ')':
            block = None
            continue
        if text in ('require (', 'replace ('):
            block = text.split()[0]
            continue
        if text.startswith('require '):
            kind, text = 'require', text[8:].strip()
        elif text.startswith('replace '):
            kind, text = 'replace', text[8:].strip()
        else:
            kind = block
        if kind == 'require':
            fields = text.split()
            if (len(fields) != 2 or len(fields[0]) > 256 or len(fields[1]) > 128
                    or len(requires) >= 2000):
                raise ValueError('unsupported Go require syntax')
            requires.append({'module': fields[0], 'version': fields[1], 'indirect': '// indirect' in raw})
        elif kind == 'replace':
            if text.count('=>') != 1 or len(text) > 1024 or len(replacements) >= 100:
                raise ValueError('unsupported Go replace syntax')
            old, new = (part.strip() for part in text.split('=>'))
            replacements.append({'module': old.split()[0], 'target': new, 'verified': False})
    for line in sum_text.splitlines():
        fields = line.split()
        if (len(fields) != 3 or len(fields[0]) > 256 or len(fields[1]) > 128
                or len(sums) >= 10000 or not re.fullmatch(r'h1:[A-Za-z0-9+/]{43}=', fields[2])):
            raise ValueError('invalid Go checksum line')
        key = tuple(fields[:2])
        if key in sums and sums[key] != fields[2]:
            raise ValueError('conflicting Go checksum')
        sums[key] = fields[2]
    seen = set()
    for row in requires:
        key = row['module'], row['version']
        if key in seen:
            raise ValueError('duplicate Go requirement')
        seen.add(key)
        row.update(checksum=sums.get(key), go_mod_checksum=sums.get((key[0], key[1] + '/go.mod')),
                   license_declaration=None, replacement=any(r['module'] == key[0] for r in replacements))
    return {'requirements': requires, 'replacements': replacements,
            'note': 'go.sum does not authenticate local replacements; licenses and advisories remain unreviewed'}


def inventory(root=ROOT, metadata_runner=cargo_metadata, upstream=None):
    reports, inputs, gaps = [], [], []
    for manifest in CARGO_MANIFESTS:
        lock_name = str(Path(manifest).with_name('Cargo.lock'))
        raw = read_public(root, lock_name)
        inputs.append({'path': lock_name, 'sha256': hashlib.sha256(raw).hexdigest()})
        try:
            metadata = metadata_runner(root, manifest)
        except (ValueError, OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
            metadata = None
            gaps.append(manifest + ': offline metadata unavailable; licenses/relationships unknown')
        packages = cargo_inventory(tomllib.loads(raw.decode('utf8')), metadata)
        reports.append({'manifest': manifest, 'packages': packages})
        if any(row['license_declaration'] is None for row in packages):
            gaps.append(manifest + ': unknown license declarations')
    go = None
    if upstream is not None:
        raw_mod, raw_sum = read_public(upstream, 'go.mod'), read_public(upstream, 'go.sum')
        go = go_inventory(raw_mod.decode('utf8'), raw_sum.decode('utf8'))
        inputs.extend({'path': 'explicit-upstream/' + name, 'sha256': hashlib.sha256(raw).hexdigest()}
                      for name, raw in [('go.mod', raw_mod), ('go.sum', raw_sum)])
        gaps.append('upstream Go licenses, local replacements and advisories require separate source review')
    else:
        gaps.append('no explicit upstream Go snapshot supplied')
    return {'schema': 1, 'scope': 'public lockfile declarations; not a security or license approval',
            'inputs': inputs, 'cargo': reports, 'go': go,
            'system_dependencies': [{'name': name, 'version': None, 'license_declaration': None,
                                     'evidence': 'native/audio-alsa uses the system library; target package evidence required'}
                                    for name in ['ALSA', 'Opus']],
            'completeness': 'incomplete', 'gaps': gaps + ['system library versions/licenses and vulnerability advisories not verified'],
            'network_used': False, 'builds_run': False, 'approved': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--upstream', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if not args.run:
        print(json.dumps({'mode': 'plan', 'manifests': CARGO_MANIFESTS, 'commands_run': False,
                          'network_used': False, 'approval': 'not_assessed'}))
        return 0
    if args.output is None:
        parser.error('--output is required')
    report = inventory(upstream=args.upstream)
    encoded = json.dumps(report, ensure_ascii=False, indent=2)
    if len(encoded.encode()) > MAX_BYTES:
        raise ValueError('report limit')
    with args.output.open('x', encoding='utf8') as handle:
        handle.write(encoded + '\n')
    print('Inventory written; unresolved licenses/system evidence/advisories remain. Not an approval.')
    return 2


if __name__ == '__main__':
    raise SystemExit(main())
