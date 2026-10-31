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



def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pull',action='store_true',help='explicitly pull the pinned public image')
    parser.add_argument('--rust-gatt',action='store_true',help='build and run actual Rust GATT roles with standard D-Bus bodies')
    args=parser.parse_args()
    work=PROJECT/('work/gatt-policy-integration' if args.rust_gatt else 'work/ble-policy-kernel-uids')
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
    work.mkdir(parents=True,exist_ok=True)
    stage=work/('input-'+uuid.uuid4().hex[:12]);stage.mkdir(mode=0o755);stage.chmod(0o755)
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
    if args.rust_gatt:
        with (work/'build.jsonl').open('w') as output, (work/'build.log').open('w') as errors:
            subprocess.run(['cargo','test','-p','rustsimhub-ble','--test','gatt_kernel_uid',
                '--features','bluez-notify-fd','--no-run','--locked','--offline','--message-format=json'],
                cwd=PROJECT,stdout=output,stderr=errors,check=True)
        rows=[json.loads(line) for line in (work/'build.jsonl').read_text().splitlines()]
        binary=next(row['executable'] for row in rows if row.get('reason')=='compiler-artifact'
                    and row.get('target',{}).get('name')=='gatt_kernel_uid')
        shutil.copyfile(binary,stage/'gatt_kernel_uid');(stage/'gatt_kernel_uid').chmod(0o755)
        shutil.copyfile(MATERIAL/'fixtures/rust_gatt_scenario.py',stage/'rust_scenario.py')
        (stage/'rust_scenario.py').chmod(0o644)
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
        with (work/'container.log').open('w') as output:
            subprocess.run(command,env=environment,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=30)
    finally:
        subprocess.run(docker+['rm','-f',container],env=environment,stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL,check=False)
    log=(work/'container.log').read_text()
    expected=['one_fd_unique_owner_allowed','two_fds_blocked','wrong_path_denied',
              'wrong_characteristic_denied','wrong_member_denied',
              'incoming_fd_denied','signal_value_denied','signal_metadata_allowed','foreign_uid_denied']
    if args.rust_gatt:
        for marker in ['GATT_FOREIGN_DENIED','GATT_ACQUIRED','GATT_PACKETS','GATT_WRITE_ACK','GATT_FENCED','GATT_FDS_CLOSED']:
            assert log.count(marker)==1,marker
        assert log.count('1 passed; 0 failed')==3,'all three actual Rust role tests required'
    else:
        for case in expected:assert log.count('POLICY_CASE '+case+'\n')==1,case
    result=json.loads(next(line for line in log.splitlines() if line.startswith('{"fixture":')))
    result.update(image=IMAGE,input_sha256=inputs,
        source_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip(),
        tracked_dirty=bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=PROJECT,text=True).strip()),
        log_sha256=hashlib.sha256((work/'container.log').read_bytes()).hexdigest())
    (work/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,sort_keys=True))


if __name__=='__main__':main()
