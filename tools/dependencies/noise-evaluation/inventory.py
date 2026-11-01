# SPDX-License-Identifier: AGPL-3.0-only
"""Offline locked archive/license evidence; OSV only when explicitly requested."""
import hashlib
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parent


def digest(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError('unsafe or oversized public dependency input')
    return hashlib.sha256(path.read_bytes()).hexdigest()


def collect():
    result = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'],
                            cwd=ROOT, capture_output=True, timeout=60, check=True)
    assert len(result.stdout) <= 4 * 1024 * 1024
    metadata = json.loads(result.stdout)
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    checksums = {(p['name'], p['version']): p.get('checksum') for p in lock['package']}
    home = Path(os.environ['CARGO_HOME'])
    direct = {p['name'] for p in metadata['packages'] if p['id'] in metadata['workspace_members']}
    local = next(p for p in metadata['packages'] if p['name'] in direct)
    declared = {d['name'] for d in local['dependencies']}
    graphs = {}
    for target in ['x86_64-unknown-linux-gnu', 'wasm32-unknown-unknown']:
        filtered = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                                   '--filter-platform', target], cwd=ROOT, capture_output=True, timeout=60, check=True)
        assert len(filtered.stdout) <= 4 * 1024 * 1024
        graphs[target] = {n['id'] for n in json.loads(filtered.stdout)['resolve']['nodes']}
    rows = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['source'] is None:
            continue
        assert package['source'] == 'registry+https://github.com/rust-lang/crates.io-index'
        name, version = package['name'], package['version']
        directory = Path(package['manifest_path']).parent
        archive = home / 'registry/cache' / directory.parent.name / f'{name}-{version}.crate'
        checksum = checksums[name, version]
        assert digest(archive) == checksum, 'archive differs from locked registry checksum'
        license_paths = {p for p in directory.iterdir() if p.is_file() and p.name.upper().startswith(('LICENSE', 'COPYING', 'COPYRIGHT', 'UNLICENSE'))}
        if package.get('license_file'):
            license_paths.add(directory / package['license_file'])
        licenses = []
        for path in sorted(license_paths):
            assert path.resolve().is_relative_to(directory.resolve())
            licenses.append({'path': str(path.relative_to(directory)), 'sha256': digest(path)})
        rows.append({'name': name, 'version': version, 'registry_sha256': checksum,
                     'archive_verified': True, 'license_declaration': package['license'],
                     'license_files': licenses, 'rust_version': package.get('rust_version'),
                     'selected_dependency_graphs': [t for t, ids in graphs.items() if package['id'] in ids],
                     'relationship': 'direct' if name in declared else 'transitive'})
    nodes = metadata['resolve']['nodes']
    selected = {p['id']: p['name'] for p in metadata['packages']}
    snow = next(n for n in nodes if selected[n['id']] == 'snow')
    return {'schema': 1, 'root_lock_sha256': digest(ROOT / 'Cargo.lock'),
            'snow_features': sorted(snow['features']), 'packages': rows,
            'license_review': 'declarations and file hashes only; not legal approval',
            'missing_packaged_license_text': [p['name'] for p in rows if not p['license_files']],
            'advisory_check': {'status': 'not_run'}, 'production_ready': False}


def query_osv(inventory):
    """Public package name/version queries only; no source, identities or secrets."""
    rows = inventory['packages']
    request = {'queries': [{'package': {'ecosystem': 'crates.io', 'name': p['name']},
                            'version': p['version']} for p in rows]}
    data = json.dumps(request).encode()
    with urllib.request.urlopen(urllib.request.Request('https://api.osv.dev/v1/querybatch', data=data,
                                headers={'Content-Type': 'application/json'}), timeout=30) as response:
        raw = response.read(4 * 1024 * 1024 + 1)
    assert len(raw) <= 4 * 1024 * 1024
    results = json.loads(raw)['results']
    assert len(results) == len(rows)
    findings = []
    for package, result in zip(rows, results):
        assert not result.get('next_page_token'), 'incomplete advisory page'
        ids = sorted(v['id'] for v in result.get('vulns', []))
        if ids:
            findings.append({'name': package['name'], 'version': package['version'], 'advisory_ids': ids})
    inventory['advisory_check'] = {'status': 'queried', 'source': 'https://api.osv.dev/v1/querybatch',
                                   'queried_at_utc': datetime.now(timezone.utc).isoformat(),
                                   'raw_response_sha256': hashlib.sha256(raw).hexdigest(),
                                   'findings': findings,
                                   'scope': 'OSV matches at query time; not a complete security audit'}
    return raw


def verify_sources():
    """Verify the committed subset and notices against the checksum-pinned crate."""
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
    snow = next(p for p in lock['package'] if p['name'] == 'snow')
    assert snow['version'] == '0.10.0'
    archives = list((Path(os.environ['CARGO_HOME']) / 'registry/cache').glob('*/snow-0.10.0.crate'))
    assert len(archives) == 1 and digest(archives[0]) == snow['checksum']
    with tarfile.open(archives[0], 'r:gz') as archive:
        def read(name):
            member = archive.getmember('snow-0.10.0/' + name)
            assert member.isfile() and member.size <= 4 * 1024 * 1024
            data = archive.extractfile(member).read()
            assert len(data) == member.size
            return data
        source = read('tests/vectors/snow.txt')
        all_vectors = json.loads(source)['vectors']
        selected = [v for v in all_vectors if v['protocol_name'] == 'Noise_XX_25519_AESGCM_SHA256']
        assert len(all_vectors) == 408 and len(selected) == 1
        assert json.loads((ROOT / 'fixtures/snow-xx-aesgcm-sha256.json').read_text()) == {'vectors': selected}
        notices = {}
        for name in ['LICENSE-MIT', 'LICENSE-APACHE']:
            original = read(name)
            assert original == (ROOT / 'fixtures' / ('snow-' + name)).read_bytes()
            notices[name] = hashlib.sha256(original).hexdigest()
        readme = read('README.md')
        assert b'has not received any formal audit' in readme
        return {'crate': 'snow', 'version': '0.10.0', 'archive_sha256': snow['checksum'],
                'vcs_commit_declared_in_crate': json.loads(read('.cargo_vcs_info.json'))['git']['sha1'],
                'vectors_path': 'tests/vectors/snow.txt', 'upstream_vectors': len(all_vectors),
                'upstream_vectors_file_sha256': hashlib.sha256(source).hexdigest(),
                'selected_vector_sha256': digest(ROOT / 'fixtures/snow-xx-aesgcm-sha256.json'),
                'selected_vectors': len(selected), 'known_answer_messages': 5,
                'notice_sha256': notices, 'formal_audit_claimed': False}
