#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run fixed public-API C13/C14 ordinary-UID cases; default is a plan."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys

CASES = {
    'p01_real_directory_eacces_preserves_baseline_and_reopens',
    'p02_stage_persist_collision_keeps_orphan_out_of_durable_ledger',
    'p03_failed_choose_returns_no_path_and_only_durable_attempt_counts',
    'p04_failed_confirm_does_not_durably_confirm_and_reopens_to_fallback',
    'p05_runtime_link_eacces_preserves_selected_but_consumes_attempt',
    'p06_repeated_confirm_refuses_without_changing_durable_state',
    'p07_valid_stale_floor_and_foreign_trust_next_are_never_adopted',
}
C14_CASES = {
    'c14_stage_completed_then_sigkill_reopens_candidate_once',
    'c14_chosen_unconfirmed_then_sigkill_reopens_fallback',
    'c14_signature_and_schema_rejection_preserve_actual_store',
    'c14_real_outbox_unknown_and_projected_fallback_rejection_preserve_receipts',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--suite', choices=('c13', 'c14'), default='c13')
    args = parser.parse_args()
    cases = CASES if args.suite == 'c13' else C14_CASES
    if not args.run:
        print(json.dumps({'execute': False, 'cases': sorted(cases), 'formats': [1, 2],
                          'root': False, 'payload_execution': False, 'device_access': False}))
        return 0
    if sys.platform != 'linux' or os.geteuid() == 0:
        parser.error('ordinary Linux UID required; use the WSL update suite on Windows')
    target = 'failure_matrix' if args.suite == 'c13' else 'crash_matrix'
    command = ['cargo', 'test', '-p', 'rustsimhub-update', '--test', target,
               '--locked', '--offline', '--', '--nocapture']
    if args.suite == 'c14':
        command += ['--skip', 'c14_process_worker', '--skip', 'c14_tmpfs_enospc']
    child = subprocess.Popen(command, cwd=Path(__file__).resolve().parents[1],
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        output, _ = child.communicate(timeout=900)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        output, _ = child.communicate()
        print(output, end='')
        print(json.dumps({'passed': False, 'reason': 'bounded timeout'}))
        return 124
    print(output, end='')
    passed = set(re.findall(r'^test (\w+) \.\.\. ok$', output, re.M))
    ok = child.returncode == 0 and passed == cases and f'{len(cases)} passed; 0 failed; 0 ignored;' in output
    print(json.dumps({'passed': ok, 'case_count': len(cases), 'cases': sorted(passed),
                      'formats': [1, 2], 'backend': 'public-production-Store-API',
                      'payload_execution': False, 'device_access': False,
                      'c14_internal_interruptions_tested': False}))
    return 0 if ok else 1


if __name__ == '__main__':
    raise SystemExit(main())
