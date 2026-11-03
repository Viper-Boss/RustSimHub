#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build the explicit, local-only recovery product candidate. Does not publish."""
import argparse,hashlib,importlib.util,json,os,pathlib,subprocess
ROOT=pathlib.Path(__file__).resolve().parents[1]
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def build(output,bindgen):
    output=output.absolute()
    if not output.is_relative_to(ROOT/'work') or output.exists() or any(p.is_symlink() for p in [output,*output.parents]):raise ValueError('new workspace work directory required')
    version=subprocess.check_output([str(bindgen),'--version'],text=True).strip()
    if version!='wasm-bindgen 0.2.129':raise ValueError('locked wasm-bindgen 0.2.129 required')
    output.mkdir(mode=0o700)
    env=dict(os.environ,CARGO_INCREMENTAL='0')
    with (output/'cargo.log').open('wb') as log:subprocess.run(['cargo','build','--offline','--locked','--release','--target','wasm32-unknown-unknown','-p','rustsimhub-recovery-session'],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    wasm=output/'wasm';subprocess.run([str(bindgen),'--target','web','--out-dir',str(wasm),str(ROOT/'target/wasm32-unknown-unknown/release/rustsimhub_recovery_session.wasm')],check=True)
    spec=importlib.util.spec_from_file_location('site_build',ROOT/'tools/site-build/build.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    report=module.assemble(output/'site',recovery_wasm=wasm)
    report['head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    report['tracked_dirty']=bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT))
    report['wasm_bindgen']=version
    report['sha256']={str(p.relative_to(output/'site')):digest(p) for p in (output/'site').rglob('*') if p.is_file()}
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'files':len(report['files']),'published':False,'candidate':True}))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--wasm-bindgen',type=pathlib.Path,required=True);a=p.parse_args();build(a.output,a.wasm_bindgen)
