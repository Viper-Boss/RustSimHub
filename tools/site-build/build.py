#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Deterministic allowlisted dashboard + configurator assembly; does not publish."""
import argparse,ast,hashlib,json,pathlib,re,shutil,tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
FILES=('index.html','manifest.webmanifest','assets/icon.svg','sw.js','src/contracts.js','src/pending-store.js','src/recovery-controller.js','src/static-policy.js','src/app.js')
DASHBOARD=('index.html','progress.json','remaining.md')

def read_file(root,relative,limit):
    path=root/relative
    for part in [path,*path.parents]:
        if part==root.parent:break
        if part.is_symlink():raise ValueError('symlink is not a static artifact')
    actual=path.resolve(strict=True)
    if not actual.is_relative_to(root.resolve()) or not actual.is_file() or actual.stat().st_size>limit:raise ValueError('unsafe or oversized static file')
    data=actual.read_bytes()
    if len(data)>limit:raise ValueError('static file grew')
    return data

def assemble(output,root=ROOT):
    if output.exists() or output.is_symlink():raise ValueError('output must be new')
    config=root/'configurator'
    policy=read_file(config,'src/static-policy.js',524288).decode()
    declared=ast.literal_eval(re.search(r'export const FILES=(\[[^;]+\]);',policy).group(1))
    if declared!=list(FILES) or len(FILES)>32:raise ValueError('reviewed static allowlist mismatch')
    assets={p:read_file(config,p,524288) for p in FILES}
    if sum(map(len,assets.values()))>2097152:raise ValueError('configurator total limit')
    digest=hashlib.sha256(b''.join(p.encode()+b'\0'+hashlib.sha256(assets[p]).digest() for p in FILES)).hexdigest()
    if assets['sw.js'].count(b'__STATIC_VERSION__')!=1:raise ValueError('exact version placeholder required')
    assets['sw.js']=assets['sw.js'].replace(b'__STATIC_VERSION__',digest.encode())
    if any(len(v)>524288 for v in assets.values()) or sum(map(len,assets.values()))>2097152:raise ValueError('built static limit')
    dashboard={p:read_file(root/'docs/progress',p,4194304) for p in DASHBOARD}
    with tempfile.TemporaryDirectory(prefix='rsh-site-',dir=output.parent) as tmp:
        staging=pathlib.Path(tmp)/'site';staging.mkdir()
        for name,data in dashboard.items():(staging/name).write_bytes(data)
        (staging/'.nojekyll').touch()
        for name,data in assets.items():
            target=staging/'configurator'/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        staging.rename(output)
    return {'schema':1,'files':list(DASHBOARD)+['.nojekyll']+['configurator/'+p for p in FILES],
            'configurator_source_digest':digest,'published':False,
            'required_browser_path':'/RustSimHub/configurator/',
            'scope_warning':'A site served under /RustSimHub-progress/ does not match the frozen path; lead approval is required before deployment.'}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--run',action='store_true');p.add_argument('--output',type=pathlib.Path);a=p.parse_args()
    if not a.run:print(json.dumps({'mode':'plan','copies':list(DASHBOARD)+list(FILES),'publishes':False},indent=2))
    elif a.output is None:p.error('--output is required')
    else:print(json.dumps(assemble(a.output),indent=2))
