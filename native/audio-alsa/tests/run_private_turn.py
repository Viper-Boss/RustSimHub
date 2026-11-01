#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit private TLS/TURN test; no host packages, accounts or external TURN."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import uuid

PROJECT = Path(__file__).resolve().parents[3]
MANIFEST = PROJECT/'native/audio-alsa/Cargo.toml'
WORK = PROJECT/'work/turn-private-final'
TAG = 'rustsimhub-private-turn-pinned:20261101'

def run():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-fixture',action='store_true',help='build the fixed public synthetic-server image through the configured proxy/CA')
    parser.add_argument('--image',default=TAG,help='local fixture image; its actual ID/packages are recorded')
    args=parser.parse_args()
    WORK.mkdir(parents=True,exist_ok=True)
    env=dict(os.environ)
    for name in ('DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH'):
        env.pop(name,None)
    docker=['docker','--host=unix:///var/run/docker.sock']
    subprocess.run(docker+['info','--format','{{.ServerVersion}}'],env=env,check=True)
    if args.build_fixture:
        ca=Path('/etc/ssl/certs/ca-certificates.crt')
        with (WORK/'image-build.log').open('w') as log:
            subprocess.run(docker+['build','--secret',f'id=proxy_ca,src={ca}',
                '-f',str(PROJECT/'native/audio-alsa/tests/fixtures/Dockerfile.turn'),
                '-t',args.image,str(PROJECT/'native/audio-alsa/tests/fixtures')],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    image_id=subprocess.check_output(docker+['image','inspect',args.image,'--format','{{.Id}}'],env=env,text=True).strip()
    assert image_id.startswith('sha256:'),'actual local image ID required'
    with (WORK/'build.jsonl').open('w') as output,(WORK/'build.log').open('w') as errors:
        subprocess.run(['cargo','test','--manifest-path',str(MANIFEST),'--test','turn_private_tls','--features','turn-relay',
                        '--no-run','--locked','--offline','--message-format=json'],cwd=PROJECT,stdout=output,stderr=errors,check=True)
    rows=[json.loads(line) for line in (WORK/'build.jsonl').read_text().splitlines()]
    binary=next(row['executable'] for row in rows if row.get('executable') and row['target']['name']=='turn_private_tls')
    stage=WORK/'input';stage.mkdir(exist_ok=True);stage.chmod(0o755)
    shutil.copy2(binary,stage/'turn_private_tls');(stage/'turn_private_tls').chmod(0o755)
    container='rustsimhub-private-turn-'+uuid.uuid4().hex[:12]
    command=docker+['run','--name',container,'--rm','--pull=never','--network=none','--read-only','--user','1000:1000',
        '--tmpfs','/tmp:rw,nosuid,nodev,size=64m,uid=1000,gid=1000','--cap-drop=ALL','--security-opt=no-new-privileges',
        '--pids-limit=12','--memory=128m','--cpus=2','--mount',f'type=bind,src={stage},dst=/input,readonly',
        '--env','RUSTSIMHUB_PRIVATE_TURN_FIXTURE=1',image_id,'/input/turn_private_tls','--nocapture','--test-threads=1']
    try:
        with (WORK/'container.log').open('w') as log:
            subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=55)
    finally:
        subprocess.run(docker+['rm','-f',container],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False)
    log=(WORK/'container.log').read_text()
    if 'test result: ok. 23 passed; 0 failed; 0 ignored;' not in log:
        raise RuntimeError('missing complete actual private test result')
    result={'fixture':'actual-private-turn-tls-rtc','source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip(),
        'tracked_dirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=PROJECT,text=True).strip()),
        'tests_passed':23,'image':image_id,'network':'none','uid':1000,'capabilities':[],
        'memory_limit_bytes':128*1024*1024,'pids_limit':12,'cpus':2,
        'binary_sha256':hashlib.sha256((stage/'turn_private_tls').read_bytes()).hexdigest(),
        'log_sha256':hashlib.sha256((WORK/'container.log').read_bytes()).hexdigest()}
    (WORK/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,sort_keys=True))

if __name__=='__main__':
    run()
