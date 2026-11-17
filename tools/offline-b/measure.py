#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Bounded Windows host measurements of fixed synthetic fixtures; no device benchmark."""
import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import statistics
import sys
import time

import run
from checks import CHECKS

PROFILES = {'statistics': 'web-usage-stats', 'network': 'web-network-policy',
            'forwarding': 'web-forwarding', 'sms': 'web-daily-sms'}
METHOD = 'windows-owned-job-cpu-and-sampled-working-set-v1'


def measure(profile, output, iterations=8, seconds=10):
    if profile not in PROFILES or not 1 <= iterations <= 16 or not 1 <= seconds <= 20:
        raise ValueError('finite workload/iteration/deadline required')
    before = run.source(run.ROOT)
    report = {'format': 1, 'kind': 'host-synthetic-resource-evidence', 'method': METHOD,
              'profile': profile, 'source_before': before, 'results': [],
              'environment': {'os': platform.system(), 'architecture': platform.machine(),
                              'python': platform.python_version()},
              'scope': 'job members including Python bootstrap and functional fixture; excludes parent sampler',
              'units': {'cpu': 'job-member CPU seconds / measured wall seconds * 100 (one core)',
                        'rss': 'maximum observed sum of job-member working sets; bytes, sampled, not OS peak',
                        'latency': 'fixture plus bootstrap/cleanup wall time; milliseconds, not RPC latency'},
              'limits': {'iterations': iterations, 'seconds': seconds, 'processes': 32,
                         'job_committed_bytes': 1024**3, 'bytes_per_log': run.LIMIT},
              'board_measured': False, 'power_measured': False, 'temperature_measured': False,
              'hours_72_verified': False, 'thresholds_approved': False, 'installable': False}
    if os.name != 'nt' or not run.capability('node'):
        report.update(status='not_run', reason='Windows owned Job and existing Node22+ required', exit_code=3)
    else:
        check = next(c for c in CHECKS if c.name == PROFILES[profile])
        began = time.monotonic()
        for index in range(iterations):
            remaining = seconds - (time.monotonic() - began)
            if remaining <= 0:
                break
            result = run.windows_execute(check, run.ROOT, output/f'iteration-{index}.log',
                                         min(check.seconds, remaining), measure=True)
            status = run.classify(result, check.kind)
            report['results'].append({'status': status, **{k:v for k,v in result.items() if k != 'summary'}})
            if status != 'passed':
                break
        rows = report['results']
        failed = any(r['status'] == 'failed' for r in rows)
        incomplete = len(rows) != iterations or any(r['status'] != 'passed'
            or not r.get('resources', {}).get('rss_samples')
            or r.get('resources', {}).get('rss_gaps')
            or r.get('resources', {}).get('measurement_errors')
            or 'cpu_seconds' not in r.get('resources', {}) for r in rows)
        report.update(status='failed' if failed else 'not_run' if incomplete else 'measured',
                      exit_code=2 if failed else 3 if incomplete else 0)
        if rows and not incomplete and not failed:
            wall = sum(r['duration_seconds'] for r in rows)
            latency = sorted(r['duration_seconds'] * 1000 for r in rows)
            cpu = sum(r['resources']['cpu_seconds'] for r in rows)
            report['measurements'] = {'completed_iterations': len(rows), 'cpu_seconds': cpu,
                'measured_wall_seconds': wall, 'cpu_percent_one_core': 100*cpu/wall if wall > 0 else None,
                'peak_observed_rss_bytes': max(r['resources']['peak_observed_rss_bytes'] for r in rows),
                'peak_job_committed_bytes': max(r['resources']['peak_job_committed_bytes'] for r in rows),
                'median_fixture_ms': statistics.median(latency),
                'p95_fixture_ms_nearest_rank': latency[math.ceil(.95*len(latency))-1]}
    report['source_after'] = run.source(run.ROOT)
    if report['source_after'] != before:
        report.update(status='failed', reason='source changed during measurement', exit_code=2)
    report['completed_utc'] = datetime.now(timezone.utc).isoformat()
    with (output/'measurement.json').open('x', encoding='utf-8') as stream:
        json.dump(report, stream, indent=2)
        stream.write('\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=tuple(PROFILES), required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--iterations', type=int, default=8)
    parser.add_argument('--seconds', type=int, default=10)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 16 or not 1 <= args.seconds <= 20:
        parser.error('iterations 1..16, seconds 1..20 required')
    try:
        report = measure(args.profile, run.fresh_output(args.output), args.iterations, args.seconds)
    except (OSError, ValueError) as error:
        parser.exit(2, 'Host measurement refused: '+str(error)+'\n')
    print(json.dumps({k: report[k] for k in ('status', 'exit_code')}))
    return report['exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
