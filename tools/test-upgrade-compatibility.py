#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real finite compatibility CLI, actual program metadata and readonly SQLite.

All Root/data inputs are disposable observations. This checks version logic and
process behavior, not release provenance, live Root files or updater admission.
"""
import argparse
import copy
import json
from pathlib import Path
import sqlite3
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['launcher', 'core', 'worker']:
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--qemu', type=Path)
    parser.add_argument('--sysroot', type=Path)
    parser.add_argument('--legacy-core', type=Path)
    args = parser.parse_args()
    assert bool(args.qemu) == bool(args.sysroot)
    runner = [str(args.qemu.resolve(strict=True)), '-L', str(args.sysroot.resolve(strict=True))] if args.qemu else []

    def command(path, *arguments):
        return [*runner, str(path.resolve(strict=True)), *arguments]

    def info(path):
        result = subprocess.run(command(path, '--build-info'), capture_output=True, check=True, timeout=5)
        assert not result.stderr and len(result.stdout) <= 16384
        value = json.loads(result.stdout)
        assert value['development_fixtures'] is False
        assert len(value['source_archive_sha256']) == 64
        return result.stdout.decode(), value

    core, ci = info(args.core)
    worker, wi = info(args.worker)
    _, li = info(args.launcher)
    assert li['upgrade_check_protocol'] == 1
    assert ci['source_archive_sha256'] == wi['source_archive_sha256']
    assert ci['forwarding_runtime_contract'] == wi['forwarding_runtime_contract']
    programs = {'core': core, 'worker': worker}
    base = {'format': 1, 'target_arch': ci['target_arch'], 'root_mode': 'setup_ap',
            'root_contract': json.dumps(ci['setup_ap_installation_contract']),
            'data': {'business_schema': 6, 'outbox': None},
            'selected': copy.deepcopy(programs), 'candidate': copy.deepcopy(programs),
            'fallback': copy.deepcopy(programs)}
    results = []
    with tempfile.TemporaryDirectory(prefix='rsh-upgrade-cli-') as temporary:
        root = Path(temporary)
        empty = root/'cwd'
        empty.mkdir()

        def check(name, request, code, decision=None):
            raw = json.dumps(request).encode() if isinstance(request, dict) else request
            proc = subprocess.run(command(args.launcher, '--check-upgrade'), input=raw,
                                  capture_output=True, cwd=empty, timeout=5)
            assert list(empty.iterdir()) == [], 'comparison CLI wrote files'
            assert len(proc.stdout) < 16384 and len(proc.stderr) < 1024
            assert proc.returncode == code, (name, proc.returncode, proc.stderr.decode())
            if decision:
                assert not proc.stderr
                value = json.loads(proc.stdout)
                assert value['decision'] == decision and value['authorization_verified'] is False
                assert value['target_arch'] == request['target_arch']
            else:
                assert not proc.stdout and proc.stderr == b'core launcher: compatibility input refused\n'
                value = None
            results.append({'case': name, 'exit_code': code, 'passed': True})
            return value

        for version in [1, 2, 3, 4]:
            directory = root/str(version)
            directory.mkdir(mode=0o700)
            (directory/'.writer.lock').touch(mode=0o600)
            path = directory/'outbox.sqlite3'
            with sqlite3.connect(path) as db:
                db.executescript('CREATE TABLE synthetic(id INTEGER); PRAGMA application_id='+str(0x52534657)+';')
                db.execute('PRAGMA user_version='+str(version))
            path.chmod(0o600)
            observed = subprocess.run(command(args.worker, '--inspect-outbox', str(directory)),
                                      capture_output=True, check=True, timeout=5)
            probe = json.loads(observed.stdout)
            assert probe['outbox']['application_id'] == 0x52534657
            request = copy.deepcopy(base)
            request['data']['outbox'] = probe['outbox']
            result = check('observed-format-'+str(version), request,
                          0 if version == 3 else 4 if version == 4 else 3,
                          'compatible' if version == 3 else 'rejected' if version == 4 else 'maintenance_required')
            if version in [1, 2]:
                assert result['projected_data']['outbox']['format'] == 3
                assert result['fallback_after_candidate']['decision'] == 'compatible'

        check('explicit-absent', copy.deepcopy(base), 0, 'compatible')
        for role in ['selected', 'candidate', 'fallback']:
            request = copy.deepcopy(base)
            changed = json.loads(request[role]['worker'])
            changed['source_archive_sha256'] = 'b'*64
            request[role]['worker'] = json.dumps(changed)
            check('worker-source-'+role, request, 4, 'rejected')
        request = copy.deepcopy(base)
        request['root_contract'] = '{}'
        check('wrong-root-contract', request, 3, 'maintenance_required')
        request = copy.deepcopy(base)
        request['target_arch'] = 'x86_64' if ci['target_arch'] == 'aarch64' else 'aarch64'
        check('wrong-architecture', request, 4, 'rejected')
        request = copy.deepcopy(base)
        if args.legacy_core:
            historic, _ = info(args.legacy_core)
        else:
            historic = json.dumps({'source_archive_sha256': 'f0226d2b32ec69a7ab1a062516a75dfbe7229b403d8f72210c7c8e2a099896b6',
                                   'database_schema': 6, 'development_fixtures': False})
        request['fallback'] = {'core': historic, 'worker': None}
        result = check('legacy-without-runtime', request, 3, 'maintenance_required')
        assert 'missing_runtime_contract' in result['fallback_after_candidate']['reasons']
        raw = json.dumps(base).encode()
        check('duplicate-envelope', raw.replace(b'{', b'{"format":1,', 1), 1)
        request = copy.deepcopy(base)
        del request['data']['outbox']
        check('missing-data-observation', request, 1)
        check('oversized-input', b' '*(128*1024+1), 1)
        check('empty-input', b'', 1)
    print(json.dumps({'passed': True, 'case_count': len(results), 'cases': results,
                      'actual_core_worker_metadata': True, 'actual_outbox_probe': True,
                      'actual_legacy_program': bool(args.legacy_core), 'qemu': bool(runner),
                      'device_access': False, 'live_admission_verified': False}))


if __name__ == '__main__':
    main()
