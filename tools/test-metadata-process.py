#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the ignored real Root/UID986 metadata boundary in an isolated network.

The test program is selected from this checkout's Cargo artifacts. Three tiny
synthetic ELF fixtures implement only the fixed build-info argument and bounded
fault cases. No installed trust, device, production data or service is used.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TEST = 'admission::programs::tests::actual_root_capture_drops_to_core_uid_clears_groups_and_enforces_deadline'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cargo-artifacts', type=Path, required=True)
    parser.add_argument('--missing-kill', action='store_true')
    args = parser.parse_args()
    assert os.geteuid() == 0 and socket.if_nameindex() == [(1, 'lo')]
    artifacts = []
    for line in args.cargo_artifacts.read_text().splitlines():
        row = json.loads(line)
        if (row.get('reason') == 'compiler-artifact' and row.get('profile', {}).get('test')
                and row.get('target', {}).get('name') == 'rustsimhub_update'
                and row.get('target', {}).get('kind') == ['lib'] and row.get('executable')):
            artifacts.append(Path(row['executable']).resolve(strict=True))
    assert len(artifacts) == 1, 'need the exact current updater library test artifact'
    with tempfile.TemporaryDirectory(prefix='rsh-signed-metadata-') as directory:
        directory = Path(directory)
        directory.chmod(0o755)
        fixtures = []
        for name, define in [('normal', []), ('wait', ['-DTEST_WAIT=1']), ('flood', ['-DTEST_FLOOD=1'])]:
            output = directory/name
            subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', *define,
                            str(ROOT/'tools/fixtures/metadata-process.c'), '-o', str(output)],
                           check=True, timeout=20)
            output.chmod(0o555)
            fixtures.append(str(output))
        env = {'PATH':'/usr/bin:/bin', 'LC_ALL':'C',
               'RUSTSIMHUB_METADATA_FIXTURE':fixtures[0],
               'RUSTSIMHUB_METADATA_WAIT_FIXTURE':fixtures[1],
               'RUSTSIMHUB_METADATA_FLOOD_FIXTURE':fixtures[2],
               'RUSTSIMHUB_SYNTHETIC_PARENT_SECRET':'synthetic-only-not-for-child'}
        selected_test = ('admission::programs::tests::actual_root_capture_requires_the_ability_to_terminate_its_dropped_uid_child'
                         if args.missing_kill else TEST)
        result = subprocess.run([str(artifacts[0]), '--exact', selected_test, '--ignored',
                                 '--nocapture', '--test-threads=1'],
                                env=env, extra_groups=[987,988], capture_output=True,
                                timeout=12, check=True)
        assert b'1 passed; 0 failed' in result.stdout
        assert b'synthetic-only-not-for-child' not in result.stdout+result.stderr
        print(json.dumps({'passed':True, 'actual_root_test':True, 'actual_uid':None if args.missing_kill else 986,
                          'supplementary_groups_cleared':None if args.missing_kill else True,
                          'environment_cleared':None if args.missing_kill else True,
                          'sealed_execution':not args.missing_kill,'stdout_budget':16384,'deadline_seconds':3,
                          'missing_kill_guard_verified':args.missing_kill,
                          'production_arm_programs_tested':False,'device_access':False}))


if __name__ == '__main__':
    main()
