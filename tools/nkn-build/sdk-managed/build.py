#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Two clean-cache native/ARM builds of the complete managed SDK executable."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess

ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[2]
MODULE='github.com/Viper-Boss/RustSimHub/tools/nkn-engine'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(stage,output):
    assert stage.is_relative_to(REPO/'work') and output.is_relative_to(REPO/'work')
    assert not stage.is_symlink() and not output.exists()
    output.mkdir(mode=0o700)
    inputs={str(p.relative_to(stage)):sha(p) for p in stage.rglob('*') if p.is_file()}
    assert not any(p.is_symlink() for p in stage.rglob('*'))
    provenance=json.loads((stage.parent/'inputs.json').read_text())
    assert inputs == provenance['source_files'], 'stage changed after verification'
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip()
    assert head == provenance['head'], 'prepare the stage again at the exact build head'
    assert provenance['sdk_patch'] == sha(ROOT/'managed.patch')
    env=dict(os.environ,GOTOOLCHAIN='local',GOWORK='off',GOENV='off',GOPROXY='off',GOSUMDB='off',CGO_ENABLED='0',GOOS='linux',GOMAXPROCS='4')
    rows=[]
    for round in [1,2]:
        for arch,machine in [('amd64',62),('arm64',183)]:
            cache=output/f'clean-cache-{round}-{arch}'
            cache.mkdir(mode=0o700)
            build_env=dict(env,GOCACHE=str(cache),GOARCH=arch)
            for program in ['nkn-sdk-engine','nkn-root']:
                binary=output/f'{program}-{round}-{arch}'
                link='-s -w -buildid='
                if program=='nkn-sdk-engine':link+=' -X main.sourceCommit='+head
                args=['go','build','-tags=nkn_sdk_candidate','-mod=readonly','-trimpath','-buildvcs=false',
                      '-ldflags='+link,'-o',str(binary),MODULE+'/cmd/'+program]
                with (output/f'build-{program}-{round}-{arch}.log').open('wb') as log:
                    subprocess.run(args,cwd=stage,env=build_env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=180)
                header=binary.read_bytes()[:20]
                assert header[:6]==bytes([127,69,76,70,2,1]) and struct.unpack('<H',header[18:20])[0]==machine
                assert binary.stat().st_size<32*1024*1024
                metadata=subprocess.check_output(['go','version','-m',str(binary)],cwd=stage,env=build_env,text=True)
                assert 'nkn_sdk_fixture' not in metadata and 'nkn_sdk_candidate' in metadata
                (output/f'build-info-{program}-{round}-{arch}.txt').write_text(metadata)
                closure=subprocess.check_output(['go','list','-deps','-mod=readonly','-tags=nkn_sdk_candidate',
                    '-f','{{if .Module}}{{.Module.Path}}@{{.Module.Version}}{{end}}',MODULE+'/cmd/'+program],cwd=stage,env=build_env,text=True)
                closure=sorted(set(closure.split())-set(['@']))
                assert not any('pion/' in x or x.startswith('golang.org/x/net@') for x in closure)
                (output/f'modules-{program}-{round}-{arch}.json').write_text(json.dumps(closure,indent=2)+'\n')
                executed=arch=='amd64' and program=='nkn-sdk-engine'
                if executed:
                    version=json.loads(subprocess.check_output([str(binary),'--version'],timeout=3))
                    assert version['nkn_sdk_linked'] and version['root_launch_required'] and not version['isolated_fixture_build'] and version['source_commit']==head
                rows.append({'program':program,'round':round,'architecture':arch,'bytes':binary.stat().st_size,'sha256':sha(binary),
                             'elf_machine':machine,'modules':len(closure),'version_executed':executed,'network_executed':False})
            # Remove only this builder's newly created regenerable cache, never
            # module sources, old proof logs, unrelated outputs or the stage.
            assert cache.parent==output and cache.name.startswith('clean-cache-')
            shutil.rmtree(cache)
    for program in ['nkn-sdk-engine','nkn-root']:
        for arch in ['amd64','arm64']:
            pair=[r for r in rows if r['architecture']==arch and r['program']==program]
            assert pair[0]['sha256']==pair[1]['sha256'],program+arch+' independent build mismatch'
    assert inputs=={str(p.relative_to(stage)):sha(p) for p in stage.rglob('*') if p.is_file()}
    (output/'report.json').write_text(json.dumps({'format':1,'source_head':head,'stage_sha256':inputs,
        'artifacts':rows,'independent_cache_comparisons':4,'arm_executed':False,'production_enabled':False},indent=2)+'\n')
    print(json.dumps(rows))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    run(args.stage.resolve(),args.output.resolve())
