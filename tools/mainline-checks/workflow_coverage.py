#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Manual exact-source coverage contract, NOT a general YAML parser/executor.

Any workflow byte or expanded stage identity change requires explicit review.
Unknown YAML syntax therefore cannot silently acquire a "covered" result.
The inventory never supplies commands to execute: checklist.CHECKS alone does.
"""
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import sys
from checklist import CHECKS, fixed_environment

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = '.github/workflows/rust.yml'


def identity(check):
    value = asdict(check)
    value['fixed_environment'] = fixed_environment(check)
    return json.loads(json.dumps(value))


def audit(root=ROOT, checks=CHECKS, contract=None):
    if contract is None:
        path = Path(__file__).with_name('workflow-inventory.json')
        if path.stat().st_size > 128 * 1024:
            raise ValueError('workflow coverage inventory exceeds budget')
        contract = json.loads(path.read_text())
    if contract.get('schema') != 1 or contract.get('workflow') != WORKFLOW:
        raise ValueError('unsupported workflow coverage inventory')
    raw = (Path(root) / WORKFLOW).read_bytes()
    if len(raw) > 128 * 1024 or hashlib.sha256(raw).hexdigest() != contract['workflow_sha256']:
        raise ValueError('workflow changed: manual coverage synchronization required; no YAML is evaluated')
    actual = [identity(c) for c in checks]
    if actual != contract['check_identities']:
        raise ValueError('expanded check identity changed: review added/deleted/modified argv, environment or metadata')
    names = {c.name for c in checks}
    if len(names) != len(checks):
        raise ValueError('duplicate stage identity')
    covered, observed = set(), set()
    nonexecuted, excluded = [], []
    for row in contract['workflow_steps']:
        key = (row['job'], row['step'])
        if key in observed or row['disposition'] not in ('checks', 'capability_probe', 'external_setup', 'mixed_setup', 'excluded_scope'):
            raise ValueError('duplicate or unsupported manual workflow step')
        observed.add(key)
        if row['disposition'] == 'excluded_scope':
            if row['checks'] or not row['reason']:
                raise ValueError('excluded workflow validation needs a reason and cannot supply executable checks')
            excluded.append(dict(row, status='not_run'))
            continue
        if not set(row['checks']) <= names or (row['disposition'] == 'checks' and not row['checks']):
            raise ValueError('workflow step missing its fixed check identities')
        covered.update(row['checks'])
        if row['disposition'] != 'checks':
            if not row['reason']:
                raise ValueError('nonexecuted workflow setup/probe needs an explicit reason')
            nonexecuted.append(row)
    extra = set(contract['runner_only'])
    if covered & extra or covered | extra != names:
        raise ValueError('workflow coverage or runner-only inventory is incomplete')
    return {'method': 'manually reviewed exact-byte workflow and expanded fixed argv; no general YAML evaluation',
            'workflow': WORKFLOW, 'workflow_sha256': contract['workflow_sha256'],
            'workflow_steps': len(observed), 'check_count': len(checks),
            'workflow_check_count': len(covered), 'runner_only': sorted(extra),
            'nonexecuted_setup_or_probes': nonexecuted,
            'excluded_workflow_validations': excluded,
            'not_general_ci_or_release_approval': True}


if __name__ == '__main__':
    try:
        if len(sys.argv) != 1:
            raise ValueError('no external workflow or command arguments accepted')
        print(json.dumps(audit(), ensure_ascii=False, indent=2))
    except (ValueError, OSError, KeyError, TypeError) as exc:
        print('workflow coverage: ' + str(exc), file=sys.stderr)
        raise SystemExit(2)
