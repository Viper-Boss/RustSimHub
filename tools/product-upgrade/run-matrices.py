#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run bounded migration matrices using readonly synthetic inputs, no network.
Build the fixed inputs per README first. Does not build/pull/install or use NSS.
"""
import argparse
import os
from pathlib import Path
import subprocess

IMAGE = 'python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', required=True, type=Path)
    parser.add_argument('--only', choices=['recovery', 'email', 'both'], default='both')
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[2]
    inputs = args.inputs.resolve(strict=True)
    required = []
    if args.only in ['recovery', 'both']:
        required += ['recovery-child', 'store6-fixture', 'product-admission-tests', 'core-base.service']
    if args.only in ['email', 'both']:
        required += ['outbox3-fixture', 'email-candidate', 'test-email4-migration.py']
    for name in required:
        if not (inputs / name).is_file(): parser.error('missing fixed synthetic input: ' + name)
    env = os.environ.copy()
    for name in ['DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH']:
        env.pop(name, None)
    common = ['docker', '--host', 'unix:///var/run/docker.sock', 'run', '--rm', '--init', '--pull=never', '--network=none', '--cap-drop=ALL']
    for cap in ['SETUID', 'SETGID', 'CHOWN', 'FOWNER', 'KILL', 'DAC_OVERRIDE']:
        common += ['--cap-add=' + cap]
    common += ['--security-opt=no-new-privileges', '--tmpfs=/var/lib/rustsimhub:rw,exec,nosuid,nodev,mode=0755,size=64m', '--mount=type=bind,source=' + str(inputs) + ',target=/input,readonly']
    if args.only in ['recovery', 'both']:
        subprocess.run(common + ['--read-only', '--tmpfs=/etc:rw,nosuid,nodev,mode=0755,size=8m', '--tmpfs=/fixture:rw,nosuid,nodev,mode=0700,uid=986,gid=986,size=32m', '--mount=type=bind,source=' + str(source / 'tools/product-upgrade/test-recovery-child.py') + ',target=/driver.py,readonly', IMAGE, 'python3', '-B', '/driver.py'], env=env, check=True, timeout=60)
    if args.only in ['email', 'both']:
        # The UNMODIFIED old driver creates /owned. Its writable container layer
        # is disposable; the host inputs remain readonly and network is none.
        subprocess.run(common + [IMAGE, 'python3', '-B', '/input/test-email4-migration.py', '--legacy', '/input/outbox3-fixture', '--candidate', '/input/email-candidate'], env=env, check=True, timeout=60)

if __name__ == '__main__': main()
