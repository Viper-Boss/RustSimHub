#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Derive a NEW managed SDK stage from the independently verified patch chain."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
spec = importlib.util.spec_from_file_location('status_eval', ROOT.parent / 'sdk-status/evaluate.py')
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(source, output):
    env = dict(os.environ, GOPROXY='off', GOSUMDB='off', GOWORK='off', GOENV='off', GOTOOLCHAIN='local')
    source = status.base.safe_path(str(source))
    output = status.base.safe_path(str(output))
    output.mkdir(mode=0o700)  # Never overwrite an old stage or executable.
    status.verified_stage(source, env, output)
    stage = output / 'stage'
    stage.mkdir(mode=0o700)
    for name in ['go.mod', 'go.sum', 'sdk_test.go', 'minimal_test.go', 'mask_test.go']:
        shutil.copyfile(source / name, stage / name)
    shutil.copytree(source / 'source/third_party', stage / 'source/third_party')
    shutil.copytree(source / 'source/gorilla-websocket', stage / 'source/gorilla-websocket')
    sdk = stage / 'source/third_party/nkn-sdk-go'
    manifest = json.loads((ROOT / 'patch-manifest.json').read_text())
    assert digest(ROOT / 'managed.patch') == manifest['patch_sha256']
    expected = status.security.tree(sdk)
    for row in manifest['files']:
        assert digest(sdk / row['path']) == row['before_sha256']
    subprocess.run(['patch', '--batch', '--fuzz=0', '-p1', '-i', str(ROOT / 'managed.patch')], cwd=sdk, check=True)
    for row in manifest['files']:
        assert digest(sdk / row['path']) == row['after_sha256']
        expected[row['path']] = row['after_sha256']
    assert status.security.tree(sdk) == expected  # No unlisted patch output.
    shutil.copyfile(ROOT / 'managed_test.go', sdk / 'rustsimhub_managed_test.go')
    engine = stage / 'source/engine'
    for path in (REPO / 'tools/nkn-engine').rglob('*'):
        if not path.is_file() or path.suffix not in ['.go', '.mod']:
            continue
        assert not path.is_symlink() and path.stat().st_size < 128 * 1024
        dest = engine / path.relative_to(REPO / 'tools/nkn-engine')
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(path.read_bytes())
    with (stage / 'go.mod').open('a') as file:
        file.write('\nrequire github.com/Viper-Boss/RustSimHub/tools/nkn-engine v0.0.0\nreplace github.com/Viper-Boss/RustSimHub/tools/nkn-engine => ./source/engine\n')
    (output / 'inputs.json').write_text(json.dumps({
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
        'sdk_patch': digest(ROOT / 'managed.patch'),
        'source_files': {str(p.relative_to(stage)): digest(p) for p in stage.rglob('*') if p.is_file()},
        'production_enabled': False,
    }, indent=2) + '\n')
    print(stage)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--verified-sdk-stage', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    prepare(args.verified_sdk_stage, args.output)
