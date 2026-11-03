#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Record exact binaries and run the owned, network-isolated kernel fixture."""
import argparse,hashlib,json,os,pathlib,subprocess
ROOT=pathlib.Path(__file__).resolve().parents[3]
IMAGE='python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb'
def run(inputs,tools,output,combined):
    inputs=inputs.absolute();tools=tools.absolute();output=output.absolute()
    for path in [inputs,tools,output]:
        if not path.is_relative_to(ROOT/'work') or any(p.is_symlink() for p in [path,*path.parents]):raise ValueError('owned workspace work path required')
    if output.exists() or not (inputs/'tools').is_dir():raise ValueError('new output and precreated nested mount point required')
    programs=['engine','root','core-test']+(['sdk-client','recovery-client'] if combined else [])
    hashes={}
    for name in programs:
        p=inputs/name
        if p.is_symlink() or not p.is_file() or p.stat().st_nlink!=1:raise ValueError('independent program snapshot required')
        hashes[name]=hashlib.sha256(p.read_bytes()).hexdigest()
    output.mkdir(mode=0o700)
    env=dict(os.environ)
    for key in ['DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH']:env.pop(key,None)
    command=['docker','--host=unix:///var/run/docker.sock','run','--rm','--privileged','--cgroupns=private','--network=none',
        '--mount',f'type=bind,src={inputs},dst=/inputs,readonly','--mount',f'type=bind,src={tools},dst=/inputs/tools,readonly',
        '--mount',f'type=bind,src={ROOT}/tools/nkn-build/sdk-managed/root_kernel_fixture.py,dst=/fixture.py,readonly',IMAGE,'python','/fixture.py']
    if combined:command.append('--combined')
    with (output/'run.log').open('wb') as log:subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=90)
    result=json.loads((output/'run.log').read_text().splitlines()[-1])
    assert result['public_network'] is False and result['physical_device'] is False and result['real_systemd_unit'] is False
    if combined:assert all(result[k] is True for k in ['actual_sdk_both_ends','actual_auth_contention_queue','actual_normal_noise_membership_auth_core_health','actual_ble_uid_noise_grant_store9_listener','three_owner_reference_release','original_process_exit_verified'])
    else:assert len(result['results'])==5
    report={'format':1,'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'tracked_dirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT)),
        'image':IMAGE,'program_sha256':hashes,'fixture_sha256':hashlib.sha256((ROOT/'tools/nkn-build/sdk-managed/root_kernel_fixture.py').read_bytes()).hexdigest(),'result':result}
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'combined':combined,'passed':True,'public_network':False,'physical_device':False}))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--inputs',type=pathlib.Path,required=True);p.add_argument('--tools',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--combined',action='store_true');a=p.parse_args();run(a.inputs,a.tools,a.output,a.combined)
