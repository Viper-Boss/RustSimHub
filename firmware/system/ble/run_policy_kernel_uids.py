#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit manual private D-Bus policy test, never touches host NSS/system bus."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid

IMAGE = 'python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb'
MATERIAL = Path(__file__).resolve().parent
PROJECT = MATERIAL.parents[2]
WORK = PROJECT/'work/ble-policy-kernel-uids'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pull',action='store_true',help='explicitly pull the pinned public image')
    args=parser.parse_args()
    # Linux x86_64 host libraries are used only as public fixture input. ldd
    # inspects this fixed trusted OS binary, never arbitrary user executables.
    daemon=Path('/usr/bin/dbus-daemon')
    if os.uname().machine != 'x86_64' or not daemon.is_file():
        raise SystemExit('fixture requires Linux x86_64 and the installed dbus-daemon')
    environment=dict(os.environ)
    for name in ('DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH'):
        environment.pop(name,None)
    docker=['docker','--host=unix:///var/run/docker.sock']
    subprocess.run(docker+['info','--format','{{.ServerVersion}}'],env=environment,check=True)
    if args.pull:subprocess.run(docker+['pull',IMAGE],env=environment,check=True)
    subprocess.run(docker+['image','inspect',IMAGE,'--format','{{.Id}}'],env=environment,check=True)
    WORK.mkdir(parents=True,exist_ok=True)
    stage=WORK/('input-'+uuid.uuid4().hex[:12]);stage.mkdir(mode=0o755);stage.chmod(0o755)
    libs=stage/'lib';libs.mkdir(mode=0o755);libs.chmod(0o755)
    listing=subprocess.check_output(['ldd',str(daemon)],text=True)
    if 'not found' in listing:raise SystemExit('missing host fixture libraries')
    dependencies=set(re.findall(r'(/(?:usr/)?lib[^\s()]*)',listing))
    assert dependencies and any('ld-linux' in path for path in dependencies)
    for source in sorted(dependencies):
        path=Path(source)
        assert path.is_file(), 'actual installed dependency required'
        shutil.copyfile(path,libs/path.name);(libs/path.name).chmod(0o755)
    for source,target in [(daemon,'dbus-daemon'),(MATERIAL/'fixtures/policy_uid_fixture.py','fixture.py'),
                          (MATERIAL/'rustsimhub-ble.policy.in','policy.in')]:
        shutil.copyfile(source,stage/target);(stage/target).chmod(0o755 if target=='dbus-daemon' else 0o644)
    roles=[('root',0),('fixture-bus',1001),('rustsimhub-ble',1002),
           ('fixture-bluez',1003),('fixture-foreign',1004)]
    (stage/'passwd').write_text(''.join(f'{name}:x:{uid}:{uid}::/nonexistent:/usr/sbin/nologin\n' for name,uid in roles))
    (stage/'group').write_text(''.join(f'{name}:x:{uid}:\n' for name,uid in roles))
    for name in ['passwd','group']:(stage/name).chmod(0o644)
    inputs={str(path.relative_to(stage)):hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(stage.rglob('*')) if path.is_file()}
    container='rustsimhub-policy-uids-'+uuid.uuid4().hex[:12]
    command=docker+['run','--name',container,'--rm','--pull=never','--network=none','--read-only',
        '--tmpfs','/tmp:rw,nosuid,nodev,size=32m','--cap-drop=ALL','--cap-add=SETUID',
        '--cap-add=SETGID','--cap-add=CHOWN','--security-opt=no-new-privileges',
        '--pids-limit=16','--memory=128m','--cpus=2',
        '--mount',f'type=bind,src={stage},dst=/input,readonly',
        '--mount',f'type=bind,src={stage}/passwd,dst=/etc/passwd,readonly',
        '--mount',f'type=bind,src={stage}/group,dst=/etc/group,readonly',
        '--env','LD_LIBRARY_PATH=/input/lib',IMAGE,'python3','-B','/input/fixture.py']
    try:
        with (WORK/'container.log').open('w') as output:
            subprocess.run(command,env=environment,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=30)
    finally:
        subprocess.run(docker+['rm','-f',container],env=environment,stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL,check=False)
    log=(WORK/'container.log').read_text()
    expected=['one_fd_unique_owner_allowed','two_fds_blocked','wrong_path_denied',
              'wrong_characteristic_denied','wrong_member_denied',
              'incoming_fd_denied','signal_value_denied','signal_metadata_allowed','foreign_uid_denied']
    for case in expected:assert log.count('POLICY_CASE '+case+'\n')==1,case
    result=json.loads(next(line for line in log.splitlines() if line.startswith('{"fixture":')))
    result.update(image=IMAGE,input_sha256=inputs,
        source_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip(),
        tracked_dirty=bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=PROJECT,text=True).strip()),
        log_sha256=hashlib.sha256((WORK/'container.log').read_bytes()).hexdigest())
    (WORK/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,sort_keys=True))


if __name__=='__main__':main()
