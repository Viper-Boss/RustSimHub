#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the real finite launcher three-way comparison; no migration or approval."""
from pathlib import Path
import argparse
import json
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import release


def compare(launcher, runner, request):
    raw = release.encoded(request)
    code, data = release.arm.bounded_run([*runner, str(launcher), '--check-upgrade'],
        stdin_data=raw, result_codes=(0, 3, 4), with_status=True)
    result = release.strict_json(data)
    release.require(result.get('authorization_verified') is False and
        {'compatible': 0, 'maintenance_required': 3, 'rejected': 4}.get(result.get('decision')) == code,
        'comparison exit/decision mismatch')
    return code, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher', required=True, type=Path)
    parser.add_argument('--sysroot', required=True, type=Path)
    parser.add_argument('--request', required=True, type=Path)
    args = parser.parse_args()
    try:
        release.arm.elf(release.arm.regular(args.launcher))
        code, value = compare(args.launcher, ['qemu-aarch64', '-L', str(args.sysroot)],
            release.strict_json(release.arm.regular(args.request, 128*1024)))
    except (ValueError, OSError):
        parser.exit(1, 'Product compatibility comparison refused\n')
    print(json.dumps(value))
    raise SystemExit(code)


if __name__ == '__main__':
    main()
