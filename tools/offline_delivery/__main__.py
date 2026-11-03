# SPDX-License-Identifier: AGPL-3.0-only
import argparse
import contextlib
import sys
import subprocess
from . import evidence, initramfs, relay, rootfs
from .common import Directory, Invalid, canonical, parse, need, audit


def main():
    parser = argparse.ArgumentParser(description='C25/27/29/30 offline candidates; default prints a no-read plan')
    parser.add_argument('operation', choices=('rootfs', 'compare-rootfs', 'initramfs', 'evidence', 'relay', 'check-relay'))
    parser.add_argument('--manifest')
    parser.add_argument('--inputs')
    parser.add_argument('--output')
    parser.add_argument('--other')
    parser.add_argument('--kernel-manifest')
    parser.add_argument('--kernel-inputs')
    parser.add_argument('--run', action='store_true')
    args = parser.parse_args()
    if not args.run:
        print(canonical({'operation': args.operation, 'plan_only': True, 'reads_performed': False, 'authorization_verified': False}).decode(), end='')
        return 0
    with contextlib.ExitStack() as stack:
        def directory(path):
            need(path is not None, 'missing_directory')
            return stack.enter_context(Directory(path))
        def manifest(path):
            from pathlib import Path
            need(path is not None, 'missing_manifest')
            p = Path(path)
            return parse(directory(p.parent).read(p.name, 256 * 1024))
        if args.operation == 'compare-rootfs':
            result = rootfs.compare(directory(args.inputs), directory(args.other))
        elif args.operation == 'check-relay':
            result = relay.validate(manifest(args.manifest), directory(args.inputs))
        elif args.operation == 'evidence':
            result = evidence.validate(manifest(args.manifest))
        else:
            need(args.output is not None, 'missing_output')
            m = manifest(args.manifest)
            if args.operation == 'rootfs':
                result = rootfs.compose(m, directory(args.inputs), args.output)
            elif args.operation == 'initramfs':
                need(bool(args.kernel_manifest) == bool(args.kernel_inputs), 'kernel_pair')
                km = manifest(args.kernel_manifest) if args.kernel_manifest else None
                kd = stack.enter_context(audit.Directory(args.kernel_inputs)) if km else None
                result = initramfs.compose(m, directory(args.inputs), args.output, km, kd)
            else:
                result = relay.compose(m, args.output)
        # Reports contain only finite metadata, never input contents/errors.
        print(canonical(result).decode(), end='')
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (Invalid, audit.Missing, OSError, ValueError, KeyError, TypeError, RecursionError, TimeoutError, subprocess.SubprocessError):
        print('offline_delivery: rejected input or incomplete output', file=sys.stderr)
        raise SystemExit(2)
