#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Collect actual whole-program licenses; preserve previous advisory boundaries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[3]
OWN='github.com/Viper-Boss/RustSimHub/tools/nkn-engine'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(stage,build,out):
    assert stage.is_relative_to(ROOT/'work') and build.is_relative_to(ROOT/'work') and out.is_relative_to(ROOT/'work')
    out.mkdir(mode=0o700)
    report=json.loads((build/'report.json').read_text())
    previous=json.loads((ROOT/'docs/dependencies/nkn-websocket-mask-candidate.json').read_text())
    known={row['path']:row for row in previous['compiled_modules']}
    selected={}
    for artifact in report['artifacts']:
        name=f"{artifact['program']}-{artifact['round']}-{artifact['architecture']}"
        assert sha((build/name).read_bytes())==artifact['sha256']
        modules=json.loads((build/f"modules-{name}.json").read_text())
        for item in modules:
            path,version=item.rsplit('@',1)
            if path in selected:assert selected[path]==version
            selected[path]=version
    assert set(selected)==set(known)|{OWN}, 'whole engine changed external module closure'
    env=dict(os.environ,GOWORK='off',GOENV='off',GOTOOLCHAIN='local',GOPROXY='off',GOSUMDB='off')
    rows=[];texts=[]
    for path,version in sorted(selected.items()):
        module=json.loads(subprocess.check_output(['go','list','-mod=readonly','-m','-json',path],cwd=stage,env=env,text=True))
        assert module['Version']==version
        directory=Path(module.get('Replace',module)['Dir'])
        if path==OWN:
            candidates=[{'path':'LICENSE','sha256':sha((ROOT/'LICENSE').read_bytes())}]
            directory=ROOT
            row={'path':path,'version':version,'source':'this repository, AGPL-3.0-only','license_files':candidates}
        else:
            row=dict(known[path]);assert row['version']==version
            row.pop('selected_by_probe',None)
            candidates=row['license_files']
            assert candidates
        row['selected_by_complete_programs']=True
        for item in candidates:
            filename=Path(item['path']);assert not filename.is_absolute() and '..' not in filename.parts
            file=directory/filename
            assert file.is_file() and not file.is_symlink() and file.stat().st_size<128*1024
            data=file.read_bytes();assert sha(data)==item['sha256']
            texts.append(f"\n===== {path}@{version}: {filename} =====\n"+data.decode('utf-8')+'\n')
        rows.append(row)
    goroot=Path(subprocess.check_output(['go','env','GOROOT'],env=env,text=True).strip())
    go_version=subprocess.check_output(['go','env','GOVERSION'],env=env,text=True).strip()
    assert go_version=='go1.27.3'
    runtime_license=(goroot/'LICENSE').read_bytes();assert len(runtime_license)<128*1024
    texts.append('\n===== Go runtime/standard library '+go_version+': LICENSE =====\n'+runtime_license.decode('utf-8'))
    (out/'THIRD-PARTY-NOTICES.txt').write_text(''.join(texts).rstrip('\n')+'\n')
    result={'format':1,'source_head':report['source_head'],'whole_program_modules':rows,
            'external_modules_unchanged_from_previous_candidate':True,
            'compiled_golang_x_net_or_pion':False,'new_external_modules':[],
            'managed_patch_sha256':sha((ROOT/'tools/nkn-build/sdk-managed/managed.patch').read_bytes()),
            'notices_sha256':sha((out/'THIRD-PARTY-NOTICES.txt').read_bytes()),
            'runtime_license':{'go':go_version,'path':'GOROOT/LICENSE','sha256':sha(runtime_license)},
            'previous_advisory_disposition':'unchanged; x/crypto, x/sys and prior recorded advisories still need review',
            'dependency_or_production_approval':False,
            'system_tools':[{'name':'nftables','scope':'separate installed Root tool, not linked into SDK/Root Go programs; actual version/license must be checked at installation'},
                            {'name':'Linux cgroup v2/netfilter','scope':'kernel/runtime requirements; namespace tests do not prove device readiness'}]}
    (out/'modules.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'modules':len(rows),'new_external_modules':[],'notices_sha256':result['notices_sha256']}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stage',required=True,type=Path)
    parser.add_argument('--build',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path)
    args=parser.parse_args()
    run(args.stage.resolve(),args.build.resolve(),args.output.resolve())
