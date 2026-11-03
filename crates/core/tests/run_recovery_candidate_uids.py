#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit actual D GATT/service -> A candidate -> original Core Wi-Fi fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid

PROJECT = Path(__file__).resolve().parents[3]
IMAGE = 'python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb'
CASES = ['confirm', 'cancel', 'disconnect', 'revoke_before_confirm', 'revoke_after_save', 'replay', 'wrong_pin', 'tamper', 'busy_network', 'call_busy', 'normal_usb_denied', 'rollback_unknown']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=CASES, action='append')
    args = parser.parse_args()
    env = dict(os.environ, CARGO_INCREMENTAL='0')
    for name in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH'):
        env.pop(name, None)
    docker = ['docker', '--host=unix:///var/run/docker.sock']
    image = subprocess.check_output(docker + ['image', 'inspect', IMAGE, '--format', '{{.Id}}'], env=env, text=True).strip()
    work = PROJECT / 'work/recovery-candidate-uids'
    work.mkdir(exist_ok=True)
    stage = work / ('input-' + uuid.uuid4().hex[:12])
    stage.mkdir(mode=0o755)
    stage.chmod(0o755)
    with (work / 'build.jsonl').open('w') as out, (work / 'build.log').open('w') as err:
        subprocess.run(['cargo', 'test', '-p', 'rustsimhub-core', '--test', 'recovery_candidate_uid',
                        '--features', 'secure-session-candidate', '--no-run', '--locked', '--offline', '--message-format=json'],
                       cwd=PROJECT, env=env, stdout=out, stderr=err, check=True)
    rows = [json.loads(line) for line in (work / 'build.jsonl').read_text().splitlines()]
    binary = Path(next(row['executable'] for row in rows if row.get('executable') and row['target']['name'] == 'recovery_candidate_uid'))
    daemon = Path('/usr/bin/dbus-daemon')
    libraries = stage / 'lib'
    libraries.mkdir(mode=0o755)
    libraries.chmod(0o755)
    for trusted in (binary, daemon):
        listing = subprocess.check_output(['ldd', str(trusted)], text=True)
        assert 'not found' not in listing
        for source in set(re.findall(r'(/(?:usr/)?lib[^\s()]*)', listing)):
            path = Path(source)
            shutil.copyfile(path, libraries / path.name)
            (libraries / path.name).chmod(0o755)
    for source, name in [(binary, 'recovery-test'), (daemon, 'dbus-daemon'),
                         (PROJECT / 'crates/core/tests/fixtures/recovery_candidate_runner.py', 'runner.py')]:
        shutil.copyfile(source, stage / name)
        (stage / name).chmod(0o644 if name.endswith('.py') else 0o755)
    subprocess.run(['strip', '--strip-debug', str(stage / 'recovery-test')], check=True)
    (stage / 'passwd').write_text(''.join(f'{name}:x:{uid}:{uid}::/nonexistent:/usr/sbin/nologin\n'
                                      for name, uid in [('root', 0), ('fixture-core', 1001), ('fixture-ble', 1002), ('fixture-backend', 1003)]))
    (stage / 'group').write_text(''.join(f'fixture-{uid}:x:{uid}:\n' for uid in (0, 1001, 1002, 1003)))
    for name in ('passwd', 'group'):
        (stage / name).chmod(0o644)
    proofs = []
    for case in args.case or CASES:
        container = 'rustsimhub-recovery-candidate-' + uuid.uuid4().hex[:12]
        command = docker + ['run', '--name', container, '--rm', '--pull=never', '--network=none', '--read-only',
                            '--cap-drop=ALL', '--cap-add=SETUID', '--cap-add=SETGID', '--cap-add=CHOWN', '--cap-add=FOWNER',
                            '--security-opt=no-new-privileges', '--pids-limit=32', '--memory=256m', '--cpus=2',
                            '--tmpfs=/tmp:rw,nosuid,nodev,size=64m', '--tmpfs=/fixture:rw,nosuid,nodev,size=64m,mode=1777',
                            '--tmpfs=/run:rw,nosuid,nodev,size=8m', '--mount', f'type=bind,src={stage},dst=/input,readonly',
                            '--mount', f'type=bind,src={stage}/passwd,dst=/etc/passwd,readonly',
                            '--mount', f'type=bind,src={stage}/group,dst=/etc/group,readonly', '--env=LD_LIBRARY_PATH=/input/lib',
                            '--env=RUSTSIMHUB_RECOVERY_CANDIDATE_FIXTURE=1', '--env=RUSTSIMHUB_RECOVERY_CASE=' + case,
                            image, 'python3', '-B', '/input/runner.py']
        log = work / (case + '.log')
        try:
            with log.open('w') as out:
                result = subprocess.run(command, env=env, stdout=out, stderr=subprocess.STDOUT, timeout=40)
        finally:
            subprocess.run(docker + ['rm', '-f', container], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        assert result.returncode == 0, log.read_text()[-16000:]
        proof = json.loads(log.read_text().splitlines()[-1])
        proof['log_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest()
        proofs.append(proof)
        print(json.dumps(proof, sort_keys=True), flush=True)
    report = {'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=PROJECT, text=True).strip(),
              'tracked_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=PROJECT, text=True).strip()),
              'candidate_head': '25bcaa4671278f25720b5c2fb9c2ca720a1375ef', 'crypto_wire_frozen': False, 'production_admitted': False,
              'image': image, 'network': 'none', 'read_only': True, 'memory_limit_bytes': 268435456, 'pids_limit': 32, 'cpus': 2,
              'runtime_binary_sha256': hashlib.sha256((stage / 'recovery-test').read_bytes()).hexdigest(),
              'build_log_sha256': hashlib.sha256((work / 'build.log').read_bytes()).hexdigest(), 'scenarios': proofs,
              'physical_bluetooth_or_network_tested': False}
    (work / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, sort_keys=True))


if __name__ == '__main__':
    main()
