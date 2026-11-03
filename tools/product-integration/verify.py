#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Read-only fixed-commit assembly check; never an install/health authorization."""
import argparse
import json
from pathlib import Path
import re
import subprocess

LOCK = Path(__file__).with_name('inputs.json')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def unique(items):
    result = {}
    for key, value in items:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def parse(raw):
    require(0 < len(raw) <= 128*1024, 'dependency lock size')
    value = json.loads(raw, object_pairs_hook=unique)
    require(type(value) is dict and set(value) == {'format', 'repository', 'baseline', 'inputs'}, 'lock fields')
    require(type(value['format']) is int and value['format'] == 1, 'lock format')
    require(value['repository'] == 'Viper-Boss/RustSimHub', 'repository identity')
    require(commit(value['baseline']), 'full baseline commit')
    rows = value['inputs']
    require(type(rows) is list and 1 <= len(rows) <= 32, 'input count')
    ids = set()
    prs = set()
    heads = set()
    for row in rows:
        require(type(row) is dict and set(row) == {'id', 'kind', 'pr', 'head', 'requires', 'includes'}, 'input fields')
        name = row['id']
        require(type(name) is str and re.fullmatch('[a-z][a-z0-9-]{0,47}', name) and name not in ids, 'unique input id')
        ids.add(name)
        require(row['kind'] in ['product', 'report'], 'input kind')
        if row['head'] is None:
            require(row['pr'] is None, 'pending input has no invented PR')
        else:
            require(commit(row['head']) and row['head'] not in heads, 'unique full source commit')
            require(type(row['pr']) is int and 0 < row['pr'] < 1_000_000 and row['pr'] not in prs, 'unique PR')
            heads.add(row['head']); prs.add(row['pr'])
        for field in ['requires', 'includes']:
            items = row[field]
            require(type(items) is list and all(type(x) is str for x in items)
                    and len(items) == len(set(items)) and name not in items, 'dependency list')
        require(set(row['includes']) <= set(row['requires']), 'included inputs must be dependencies')
    index = {row['id']: row for row in rows}
    visited, visiting = set(), set()

    def visit(name):
        require(name in index and name not in visiting, 'unknown dependency or cycle')
        if name in visited:
            return
        visiting.add(name)
        for dep in index[name]['requires']:
            visit(dep)
            require(index[dep]['kind'] == 'product', 'report cannot supply a product dependency')
        visiting.remove(name); visited.add(name)
    for name in index:
        visit(name)
    return value


def commit(value):
    return type(value) is str and re.fullmatch('[0-9a-f]{40}', value) is not None


def git(repository, *arguments, allowed=(0,)):
    process = subprocess.run(['git', '--no-replace-objects', '-C', str(repository), *arguments],
                             stdin=subprocess.DEVNULL, capture_output=True, timeout=15)
    require(process.returncode in allowed, 'Git source check failed: '+arguments[0])
    require(len(process.stdout) <= 256*1024 and len(process.stderr) <= 64*1024, 'Git output bound')
    return process.returncode, process.stdout.decode('utf-8').strip()


def ancestor(repository, before, after):
    return git(repository, 'merge-base', '--is-ancestor', before, after, allowed=(0, 1))[0] == 0


def inspect(repository, value, integration):
    require(commit(integration), 'full integration commit')
    require(git(repository, 'rev-parse', '--is-shallow-repository')[1] == 'false', 'complete Git ancestry required')
    for item in [value['baseline'], integration]:
        require(git(repository, 'cat-file', '-t', item)[1] == 'commit', 'Git commit required')
    require(ancestor(repository, value['baseline'], integration), 'integration has another baseline')
    rows = {row['id']: row for row in value['inputs']}
    result, pending, pending_reports = [], [], []
    missing = []
    for row in value['inputs']:
        head = row['head']
        if head is None:
            (pending if row['kind'] == 'product' else pending_reports).append(row['id'])
            result.append({'id':row['id'], 'pr':None, 'head':None, 'state':'awaiting_delivery'})
            continue
        require(git(repository, 'cat-file', '-t', head)[1] == 'commit', 'input is not a Git commit')
        require(ancestor(repository, value['baseline'], head), 'input has another baseline')
        for dep in row['includes']:
            require(rows[dep]['head'] is not None and ancestor(repository, rows[dep]['head'], head),
                    'declared included history is absent')
        present = ancestor(repository, head, integration)
        if not present and row['kind'] == 'product':
            missing.append(row['id'])
        result.append({'id':row['id'], 'pr':row['pr'], 'head':head,
                       'tree':git(repository, 'rev-parse', head+'^{tree}')[1],
                       'state':'included' if present else 'not_in_combination'})
    checkout = git(repository, 'rev-parse', 'HEAD')[1]
    dirty = bool(git(repository, 'status', '--porcelain', '--untracked-files=all')[1])
    return {'format':1, 'repository':value['repository'], 'baseline':value['baseline'],
            'integration':integration, 'checkout_head':checkout, 'checkout_matches_integration':checkout == integration,
            'worktree_dirty':dirty,
            'source_assembly_complete':not pending and not missing and not dirty and checkout == integration,
            'awaiting_delivery':pending, 'awaiting_reports':pending_reports, 'not_in_combination':missing, 'inputs':result,
            'tests_verified':False, 'cloud_checks_verified':False, 'authorization_verified':False,
            'health_authorized':False, 'installable':False, 'device_access':False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository', type=Path, required=True)
    parser.add_argument('--integration', required=True, help='full frozen combination commit, never a branch name')
    parser.add_argument('--lock', type=Path, default=LOCK)
    parser.add_argument('--require-complete', action='store_true', help='exit 3 if source inputs are incomplete or dirty')
    args = parser.parse_args()
    try:
        result = inspect(args.repository, parse(args.lock.read_bytes()), args.integration)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(2, 'Integration source refused: '+str(error)+'\n')
    print(json.dumps(result, sort_keys=True))
    return 3 if args.require_complete and not result['source_assembly_complete'] else 0


if __name__ == '__main__':
    raise SystemExit(main())
