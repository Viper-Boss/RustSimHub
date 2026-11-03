# SPDX-License-Identifier: AGPL-3.0-only
"""C29: verify exact public Git blob provenance, never certify hardware claims."""
import subprocess
from .common import ROOT, digest, exact, hexadecimal, need, relative

CAPABILITIES = ('forwarding', 'update', 'recovery', 'release', 'ble-control', 'ble-recovery', 'nas-control', 'private-relay')
ALLOWED = ('crates/', 'docs/', 'tools/', 'firmware/')


def git(repo, *args):
    result = subprocess.run(['git', '-C', str(repo), *args], stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=15, check=False)
    need(result.returncode == 0 and len(result.stdout) <= 2 * 1024 * 1024, 'git_source')
    return result.stdout


def validate(index, repo=ROOT):
    exact(index, ('schema', 'source_commit', 'capabilities'))
    need(type(index['schema']) is int and index['schema'] == 1, 'schema')
    revision = hexadecimal(index['source_commit'], 40)
    need(git(repo, 'cat-file', '-t', revision) == b'commit\n', 'commit')
    entries = index['capabilities']
    need(type(entries) is list and len(entries) == len(CAPABILITIES), 'capability_count')
    seen, observations = set(), []
    for entry in entries:
        exact(entry, ('id', 'level', 'sources', 'device_status'))
        cap = entry['id']
        need(isinstance(cap, str) and cap in CAPABILITIES and cap not in seen, 'capability')
        seen.add(cap)
        need(entry['level'] in ('source', 'offline') and entry['device_status'] in ('unknown', 'unsupported'), 'claim_level')
        need(type(entry['sources']) is list and 1 <= len(entry['sources']) <= 16, 'sources')
        paths = set()
        for item in entry['sources']:
            exact(item, ('path', 'sha256'))
            path = item['path']
            relative(path)
            need(path.startswith(ALLOWED) and not any(p.startswith('.') for p in path.split('/')) and path not in paths, 'public_path')
            paths.add(path)
            hexadecimal(item['sha256'])
            # literal pathspec: no wildcard expansion and no symlink/submodule blobs.
            tree = git(repo, 'ls-tree', '-z', revision, '--', ':(literal)' + path)
            need(tree.count(b'\0') == 1 and tree.startswith((b'100644 blob ', b'100755 blob ')), 'regular_blob')
            need(tree.split(b'\t', 1)[1] == path.encode() + b'\0', 'exact_path')
            size = git(repo, 'cat-file', '-s', revision + ':' + path)
            need(size.strip().isdigit() and int(size) <= 2 * 1024 * 1024, 'blob_budget')
            data = git(repo, 'cat-file', 'blob', revision + ':' + path)
            need(digest(data) == item['sha256'], 'source_integrity')
        observations.append({'id': cap, 'level': entry['level'], 'device_status': entry['device_status'], 'verified_source_count': len(paths)})
    return {'schema': 1, 'source_commit': revision, 'capabilities': observations,
            'source_bytes_verified': True, 'release_signoff': False, 'authorization_verified': False}
