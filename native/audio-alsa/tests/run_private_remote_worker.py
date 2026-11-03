#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit offline actual v2 worker/TLS/UID fixture; never installs on host."""
import argparse,hashlib,json,os,pathlib,shutil,subprocess,uuid
PROJECT=pathlib.Path(__file__).resolve().parents[3]
MANIFEST=PROJECT/'native/audio-alsa/Cargo.toml'
WORK=PROJECT/'work/remote-worker-final'
BASE='rustsimhub-private-turn-pinned:20261101'
TAG='rustsimhub-private-worker-uids:20261102'
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-fixture',action='store_true',help='build only synthetic NSS over the existing pinned private TLS image, without network')
    parser.add_argument('--image',default=TAG)
    args=parser.parse_args();WORK.mkdir(parents=True,exist_ok=True)
    env=dict(os.environ)
    for key in ('DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH'):env.pop(key,None)
    docker=['docker','--host=unix:///var/run/docker.sock']
    subprocess.run(docker+['info','--format','{{.ServerVersion}}'],env=env,check=True)
    if args.build_fixture:
        base_id=subprocess.check_output(docker+['image','inspect',BASE,'--format','{{.Id}}'],env=env,text=True).strip()
        # No implicit pull or new server packages; the parent fixture has its own
        # explicit proxy/CA build and pinned server/libraries.
        with (WORK/'image-build.log').open('w') as log:
            subprocess.run(docker+['build','--pull=false','--network=none','--build-arg',f'FIXTURE_BASE={BASE}',
                '-f',str(PROJECT/'native/audio-alsa/tests/fixtures/Dockerfile.worker-uids'),'-t',args.image,
                str(PROJECT/'native/audio-alsa/tests/fixtures')],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    else:base_id=None
    image_id=subprocess.check_output(docker+['image','inspect',args.image,'--format','{{.Id}}'],env=env,text=True).strip()
    with (WORK/'build.jsonl').open('w') as out,(WORK/'build.log').open('w') as err:
        subprocess.run(['cargo','test','--manifest-path',str(MANIFEST),'--test','remote_worker_private','--features','turn-relay',
            '--no-run','--locked','--offline','--message-format=json'],cwd=PROJECT,stdout=out,stderr=err,check=True)
    rows=[json.loads(line) for line in (WORK/'build.jsonl').read_text().splitlines()]
    binary=pathlib.Path(next(row['executable'] for row in rows if row.get('executable') and row['target']['name']=='remote_worker_private'))
    with (WORK/'native-build.log').open('w') as out:
        subprocess.run(['cargo','build','--manifest-path',str(MANIFEST),'--features','turn-relay','--locked','--offline'],cwd=PROJECT,stdout=out,stderr=subprocess.STDOUT,check=True)
    with (WORK/'core-build.jsonl').open('w') as out,(WORK/'core-build.log').open('w') as err:
        subprocess.run(['cargo','test','-p','rustsimhub-core','--bin','rustsimhub-core','--features','remote-media-tests',
            '--no-run','--locked','--offline','--message-format=json'],cwd=PROJECT,stdout=out,stderr=err,check=True)
    core_rows=[json.loads(line) for line in (WORK/'core-build.jsonl').read_text().splitlines()]
    core_binary=pathlib.Path(next(row['executable'] for row in core_rows if row.get('executable') and row['target']['kind']==['bin']))
    stage=WORK/'input';stage.mkdir(exist_ok=True);stage.chmod(0o755)
    for source,name,mode in [(binary,'worker-test',0o755),(MANIFEST.parent/'target/debug/rustsimhub-audio-alsa','native-worker',0o755),
        (PROJECT/'native/audio-alsa/tests/fixtures/remote_worker_uids.py','remote_worker_uids.py',0o644),
        (PROJECT/'native/audio-alsa/tests/fixtures/normal_remote_uids.py','normal_remote_uids.py',0o644),(core_binary,'core-test',0o755)]:
        shutil.copyfile(source,stage/name);(stage/name).chmod(mode)
    common=['--rm','--pull=never','--network=none','--read-only','--tmpfs','/tmp:rw,nosuid,nodev,size=64m',
        '--cap-drop=ALL','--security-opt=no-new-privileges','--pids-limit=16','--memory=128m','--cpus=2',
        '--mount',f'type=bind,src={stage},dst=/input,readonly','--env','RUSTSIMHUB_PRIVATE_TURN_FIXTURE=1']
    def run(name,extra,command,deadline):
        container='rustsimhub-private-worker-'+uuid.uuid4().hex[:12]
        try:
            with (WORK/(name+'.log')).open('w') as log:
                subprocess.run(docker+['run','--name',container]+common+extra+[image_id]+command,
                    env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=deadline)
        finally:
            subprocess.run(docker+['rm','-f',container],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=False)
    run('matrix',['--user','1000:1000'],['/input/worker-test','--nocapture','--test-threads=1'],100)
    # Retain all five new shared RTC regressions in the exact matrix count.
    matrix=(WORK/'matrix.log').read_text();assert 'test result: ok. 43 passed; 0 failed; 2 ignored;' in matrix
    run('uids',['--user','0:0','--group-add','983','--tmpfs','/fixture:rw,nosuid,nodev,size=64m','--tmpfs','/run:rw,nosuid,nodev,size=8m',
        '--cap-add=SETUID','--cap-add=SETGID','--cap-add=CHOWN','--cap-add=FOWNER','--env','RUSTSIMHUB_PRIVATE_UID_FIXTURE=1'],
        ['python3','/input/remote_worker_uids.py'],45)
    proof=json.loads((WORK/'uids.log').read_text().splitlines()[-1]);assert proof['actors_capabilities']==[] and proof['proofs']['media']['closed']==1
    run('normal-core',['--user','0:0','--group-add','983','--tmpfs','/fixture:rw,nosuid,nodev,size=64m','--tmpfs','/run:rw,nosuid,nodev,size=8m',
        '--cap-add=SETUID','--cap-add=SETGID','--cap-add=CHOWN','--cap-add=FOWNER','--env','RUSTSIMHUB_PRIVATE_UID_FIXTURE=1'],
        ['python3','/input/normal_remote_uids.py'],45)
    normal=json.loads((WORK/'normal-core.log').read_text().splitlines()[-1])
    assert normal['core_auth_controller_media_authority_mocked'] is False
    def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
    result={'fixture':'actual-remote-worker-v2','source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip(),
        'tracked_dirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=PROJECT,text=True).strip()),
        'image':image_id,'built_parent_image':base_id,'network':'none','read_only':True,'memory_limit_bytes':134217728,'pids_limit':16,'cpus':2,
        'matrix_tests_passed':43,'matrix_tests_ignored':2,'ignored_reason':'actual UID role runs separately in disposable image',
        'new_worker_integration_functions':10,'duplicate_sdk_and_source_unit_tests':33,
        'matrix_uid':1000,'matrix_capabilities':[],'uid_fixture':proof,'normal_core_fixture':normal,'uid_fixture_controller_capabilities':['SETUID','SETGID','CHOWN','FOWNER'],
        'binary_sha256':sha(stage/'worker-test'),'non_test_binary_sha256':sha(stage/'native-worker'),'matrix_log_sha256':sha(WORK/'matrix.log'),'uids_log_sha256':sha(WORK/'uids.log'),'normal_core_log_sha256':sha(WORK/'normal-core.log'),'core_binary_sha256':sha(stage/'core-test')}
    (WORK/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,sort_keys=True))
if __name__=='__main__':main()
