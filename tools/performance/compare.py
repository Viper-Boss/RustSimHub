#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compare complete synthetic reports; never infer device budget acceptance."""
import argparse
import json
import math
import os
import pathlib
import re
import stat
from measure import MAX_BYTES, PROFILES, SCOPE, write_report


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def load(path, expected_sha):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        meta = os.fstat(stream.fileno())
        if not stat.S_ISREG(meta.st_mode) or meta.st_size > MAX_BYTES:
            raise ValueError('bounded regular report required')
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError('report grew beyond limit')
    report = json.loads(data)
    if not re.fullmatch('[a-f0-9]{40}', expected_sha) or report.get('source_sha') != expected_sha:
        raise ValueError('source SHA mismatch')
    if (report.get('schema') != 1 or report.get('kind') != 'synthetic-functional'
            or report.get('scope') != SCOPE or report.get('profile') not in PROFILES
            or report.get('source_dirty') is not False or report.get('outcome') != 'completed'
            or report.get('cleanup_verified') is not True
            or report.get('device_performance_verified') is not False
            or report.get('budget_approved') is not False):
        raise ValueError('incomplete or unsupported synthetic evidence')
    for key in ('runner_sha256', 'workload_sha256'):
        if not re.fullmatch('[a-f0-9]{64}', report.get(key, '')):
            raise ValueError('missing provenance')
    samples = report.get('samples')
    if not isinstance(samples, list) or not 10 <= len(samples) <= 500 or report.get('sample_count') != len(samples):
        raise ValueError('insufficient/bounded samples required')
    if type(report.get('completed_iterations')) is not int or not 1 <= report['completed_iterations'] <= 100:
        raise ValueError('no successful functional iterations')
    previous = -1
    valid_cpu = []
    rss = []
    for sample in samples:
        elapsed = sample.get('elapsed_s')
        if not finite(elapsed) or elapsed <= previous:
            raise ValueError('invalid monotonic samples')
        previous = elapsed
        if not finite(sample.get('rss_bytes')) or type(sample['rss_bytes']) is not int:
            raise ValueError('RSS must be nonnegative integer bytes')
        if type(sample.get('processes')) is not int or not 1 <= sample['processes'] <= 32:
            raise ValueError('invalid process count')
        cpu = sample.get('cpu_percent')
        if cpu is not None:
            if not finite(cpu):
                raise ValueError('invalid CPU percent')
            valid_cpu.append(cpu)
        rss.append(sample['rss_bytes'])
    if len(valid_cpu) < 9 or samples[-1]['elapsed_s'] - samples[0]['elapsed_s'] < 4:
        raise ValueError('short/missing samples cannot establish a comparison')
    # Recompute rather than trusting user-provided summary metrics or units.
    return report, {'mean_cpu_percent': sum(valid_cpu) / len(valid_cpu), 'peak_rss_bytes': max(rss)}


def compare(old, new, old_sha, new_sha):
    a, ma = load(old, old_sha)
    b, mb = load(new, new_sha)
    if a.get('profile') != b.get('profile') or a.get('environment') != b.get('environment'):
        raise ValueError('same profile/environment evidence required')
    if a.get('runner_sha256') != b.get('runner_sha256'):
        raise ValueError('same measurement runner required')
    return {'schema': 1, 'scope': SCOPE, 'profile': a['profile'],
            'old_sha': old_sha, 'new_sha': new_sha,
            'metrics': {key: {'old': ma[key], 'new': mb[key],
                              'change_percent': (mb[key] / ma[key] - 1) * 100 if ma[key] else None}
                        for key in ma},
            'workload_bytes_match': a['workload_sha256'] == b['workload_sha256'],
            'same_function_review_required': True,
            'budget_verdict': 'not_assessed', 'device_performance_verified': False,
            'note': 'Synthetic fixture measurements include launch overhead; source changes still require human same-function review.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('old', type=pathlib.Path)
    parser.add_argument('new', type=pathlib.Path)
    parser.add_argument('--old-sha', required=True)
    parser.add_argument('--new-sha', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    write_report(args.output, compare(args.old, args.new, args.old_sha, args.new_sha))
    print('Synthetic comparison written; device budget NOT assessed')

if __name__ == '__main__':
    main()
