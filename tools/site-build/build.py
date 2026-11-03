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

def assemble(output,root=ROOT,recovery_wasm=None):
    if output.exists() or output.is_symlink():raise ValueError('output must be new')
    config=root/'configurator'
    policy=read_file(config,'src/static-policy.js',524288).decode()
    declared=ast.literal_eval(re.search(r'export const FILES=(\[[^;]+\]);',policy).group(1))
    if declared!=list(FILES) or len(FILES)>32:raise ValueError('reviewed static allowlist mismatch')
    assets={p:read_file(config,p,524288) for p in FILES}
    if recovery_wasm is not None:
        extras=('src/production-entry.js','src/production-backend.js','src/gatt-framing.js','src/crypto/secure-session.js','src/security/recovery-binding.js')
        assets.update({p:read_file(config,p,524288) for p in extras})
        for name in ('rustsimhub_recovery_session.js','rustsimhub_recovery_session_bg.wasm'):
            assets['wasm/'+name]=read_file(recovery_wasm,name,524288)
        if not assets['wasm/rustsimhub_recovery_session_bg.wasm'].startswith(b'\0asm\x01\0\0\0'):raise ValueError('invalid WASM header')
        if b'eval(' in assets['wasm/rustsimhub_recovery_session.js']:raise ValueError('dynamic JavaScript forbidden')
        import base64
        html=assets['index.html'].decode()
        style=re.search(r'<style>(.*?)</style>',html,re.S).group(1).encode()
        style_hash=base64.b64encode(hashlib.sha256(style).digest()).decode()
        csp="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'sha256-"+style_hash+"'; connect-src 'self'; worker-src 'self'; img-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'"
        html=html.replace('<meta charset="utf-8">','<meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="'+csp+'">')
        form='<details><summary>通过原可信管理入口预登记恢复身份</summary><label>管理入口提供的登记区域标识<input id="binding-realm" maxlength="36" autocomplete="off"></label><button id="binding-key" type="button">生成或读取本浏览器独立公钥</button><label>待原管理入口批准的独立公钥<input id="binding-public" readonly></label><label>管理入口返回的授权编号<input id="binding-grant" maxlength="36" autocomplete="off"></label><label>管理入口返回的授权代次<input id="binding-generation" maxlength="19" autocomplete="off"></label><button id="binding-save" type="button">保存原管理入口的登记回执</button><p id="binding-state" role="status"></p></details>'
        html=html.replace('<button id="connect"',form+'<button id="connect"').replace('src="./src/app.js"','src="./src/production-entry.js"')
        assets['index.html']=html.encode()
        assets['src/static-policy.js']=re.sub(rb'export const FILES=\[[^;]+\];',('export const FILES='+json.dumps(list(assets),separators=(',',':'))+';').encode(),assets['src/static-policy.js'])
    if sum(map(len,assets.values()))>2097152:raise ValueError('configurator total limit')
    digest=hashlib.sha256(b''.join(p.encode()+b'\0'+hashlib.sha256(assets[p]).digest() for p in assets)).hexdigest()
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
    return {'schema':1,'files':list(DASHBOARD)+['.nojekyll']+['configurator/'+p for p in assets],
            'configurator_source_digest':digest,'published':False,
            'secure_recovery_candidate':recovery_wasm is not None,
            'required_browser_path':'/RustSimHub/configurator/',
            'scope_warning':'A site served under /RustSimHub-progress/ does not match the frozen path; lead approval is required before deployment.'}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--run',action='store_true');p.add_argument('--output',type=pathlib.Path);a=p.parse_args()
    if not a.run:print(json.dumps({'mode':'plan','copies':list(DASHBOARD)+list(FILES),'publishes':False},indent=2))
    elif a.output is None:p.error('--output is required')
    else:print(json.dumps(assemble(a.output),indent=2))
