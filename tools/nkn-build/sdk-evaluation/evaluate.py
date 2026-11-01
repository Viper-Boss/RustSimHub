#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stage checksum-verified public SDK source and execute only loopback/deny probes."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import struct
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
COMMIT = 'c32952d7586676fec10f71a30b22969cb24881f3'


def manifest():
    value = json.loads((ROOT / 'source-manifest.json').read_text())
    assert value['repository'] == 'Viper-Boss/nknguard' and value['commit'] == COMMIT
    rows = value['files']
    assert 1 <= len(rows) <= 40 and len({r['path'] for r in rows}) == len(rows)
    for row in rows:
        assert re.fullmatch(r'third_party/(?:nkn-sdk-go|ncp-go)/[A-Za-z0-9_./-]+', row['path'])
        assert '..' not in row['path'].split('/') and row['size'] <= 131072
        assert re.fullmatch('[a-f0-9]{40}', row['sha']) and re.fullmatch('[a-f0-9]{64}', row['sha256'])
    assert sum(r['size'] for r in rows) <= 512 * 1024
    return rows


def verify(row, data):
    assert len(data) == row['size']
    assert hashlib.sha256(data).hexdigest() == row['sha256']
    assert hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == row['sha']


def verified_cache(cache):
    result = {}
    for row in manifest():
        path = cache / row['path']
        for p in [path, *path.parents]:
            assert not p.is_symlink(), 'linked public source refused'
        assert path.is_file() and path.stat().st_size <= 131072
        data = path.read_bytes()
        verify(row, data)
        result[row['path']] = data
    return result


def fetch(cache):
    cache.mkdir(mode=0o700)
    def one(row):
        url = f'https://raw.githubusercontent.com/Viper-Boss/nknguard/{COMMIT}/' + row['path']
        with urllib.request.urlopen(url, timeout=20) as response:
            data = response.read(131073)
        verify(row, data)
        path = cache / row['path']
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(one, manifest()))


def command(args, stage, env, out, name, seconds=120):
    process = subprocess.Popen(args, cwd=stage, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, start_new_session=True)
    try:
        data, _ = process.communicate(timeout=seconds)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)  # Only this new, owned process group.
        data, _ = process.communicate(timeout=5)
        (out / (name + '.log')).write_bytes(data[:2 * 1024 * 1024])
        raise RuntimeError('owned probe timed out and was killed')
    assert len(data) <= 4 * 1024 * 1024, 'public tool output limit'
    (out / (name + '.log')).write_bytes(data)
    assert process.returncode == 0, name + ' failed; see work/ log'
    return data


def decode_stream(raw):
    text = raw.decode()
    decoder = json.JSONDecoder()
    rows = []
    while text.strip():
        text = text.lstrip()
        value, end = decoder.raw_decode(text)
        rows.append(value)
        text = text[end:]
    return rows


def apply_candidate(stage, env, out):
    metadata=json.loads((ROOT/'patch-manifest.json').read_text())
    patch=(ROOT/'sdk-compatibility.patch').read_bytes()
    assert metadata['source_commit']==COMMIT and metadata['production_approved'] is False
    assert hashlib.sha256(patch).hexdigest()==metadata['patch_sha256']
    paths=[row['path'] for row in metadata['files']]
    assert paths==['third_party/nkn-sdk-go/client.go','third_party/nkn-sdk-go/rpc.go']
    assert re.findall(r'^\+\+\+ b/(.+)$',patch.decode(),re.M)==paths
    for row in metadata['files']:
        assert hashlib.sha256((stage/'source'/row['path']).read_bytes()).hexdigest()==row['before_sha256']
    command(['patch','--batch','--fuzz=0','--dry-run','-p1','-i',str(ROOT/'sdk-compatibility.patch')],stage/'source',env,out,'patch-check')
    command(['patch','--batch','--fuzz=0','-p1','-i',str(ROOT/'sdk-compatibility.patch')],stage/'source',env,out,'patch-apply')
    return metadata


def verify_patched(stage, sources, metadata):
    expected={row['path']:row['after_sha256'] for row in metadata['files']}
    for name,before in sources.items():
        path=stage/'source'/name
        assert path.is_file() and not path.is_symlink() and path.stat().st_size<=131072
        data=path.read_bytes()
        if name in expected:
            assert hashlib.sha256(data).hexdigest()==expected[name]
        else:
            assert data==before


def run(out, cache, advisory=False):
    inputs=['evaluate.py','test_sources.py','sdk_test.go','go.mod','go.sum','source-manifest.json',
            'sdk-compatibility.patch','patch-manifest.json']
    input_hashes={name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in inputs}
    head_start=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    dirty_start=subprocess.check_output(['git','status','--porcelain','--',str(ROOT)],cwd=ROOT,text=True).strip()
    sources = verified_cache(cache)
    stage = out / 'stage'
    stage.mkdir(mode=0o700)
    for path, data in sources.items():
        target = stage / 'source' / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    for name in ['go.mod', 'go.sum', 'sdk_test.go']:
        shutil.copyfile(ROOT / name, stage / name)
    # Preserve inherited proxy/auth/CA variables. Downloads are disabled here;
    # prepare exact checksummed modules separately, never resolve new versions.
    env = dict(os.environ, GOWORK='off', GOENV='off', GOTOOLCHAIN='local', GOMAXPROCS='4',
               GOPROXY='off', GOSUMDB='off', CGO_ENABLED='1',RSH_NKN_PATCHED='0')
    for key in ['GOMODCACHE', 'GOCACHE', 'GOPATH']:
        assert key in env and Path(env[key]).is_absolute(), 'explicit non-home Go cache required'
        assert Path(env[key]).is_relative_to('/workspace'), 'use workspace tool caches'
    version = command(['go', 'version'], stage, env, out, 'go-version').decode().strip()
    assert version.startswith('go version go1.27.3 '), 'unpinned Go toolchain'
    toolchain=Path(command(['go','env','GOROOT'],stage,env,out,'go-root').decode().strip())
    license_path=toolchain/'LICENSE'
    assert license_path.is_file() and not license_path.is_symlink() and license_path.stat().st_size<=65536
    runtime_license=license_path.read_bytes()
    assert b'Go Authors' in runtime_license
    source_env = dict(env,RSH_SDK_EVAL_SOURCE_CACHE=str(cache))
    command([sys.executable,'-m','unittest','-v','test_sources.py'],ROOT,source_env,out,'source-tests')
    command(['go', 'mod', 'verify'], stage, env, out, 'module-verify')
    command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v', '.'], stage, env, out, 'sdk-tests')
    command(['go', 'test', '-race', '-mod=readonly', '-count=1', '-timeout=15s', '-v',
             'github.com/nknorg/nkn-sdk-go', 'github.com/nknorg/ncp-go'], stage, env, out, 'upstream-tests')
    assert verified_cache(stage/'source')==sources
    patched_stage=out/'patched-stage'
    shutil.copytree(stage,patched_stage)
    metadata=apply_candidate(patched_stage,env,out)
    stage=patched_stage
    env=dict(env,RSH_NKN_PATCHED='1')
    command(['go','test','-race','-mod=readonly','-count=1','-timeout=15s','-v','.'],stage,env,out,'patched-sdk-tests')
    command(['go','test','-race','-mod=readonly','-count=1','-timeout=15s','-v',
             'github.com/nknorg/nkn-sdk-go','github.com/nknorg/ncp-go'],stage,env,out,'patched-upstream-tests')
    command(['go', 'vet', '-mod=readonly', '.', 'github.com/nknorg/nkn-sdk-go', 'github.com/nknorg/ncp-go'], stage, env, out, 'vet')
    packages = decode_stream(command(['go', 'list', '-deps', '-test', '-json', '.'], stage, env, out, 'package-metadata'))
    # Inventory the actually compiled/tested package closure. Asking old Go
    # module graphs for all .info files can trigger unrelated network lookups
    # even after a successful offline build; do not silently turn networking on.
    modules = {p['Module']['Path']:p['Module'] for p in packages if p.get('Module')}.values()
    active = {p['Module']['Path'] for p in packages if p.get('Module')}
    rows = []
    for module in sorted(modules,key=lambda m:m['Path']):
        if module.get('Main'):
            continue
        replacement = module.get('Replace')
        item = {'path': module['Path'], 'version': module.get('Version'),
                'selected_by_probe': module['Path'] in active,
                'source': 'checksum-pinned-local-fork' if replacement else 'Go-module',
                'sum': module.get('Sum'), 'go_mod_sum': module.get('GoModSum'), 'license_files': []}
        directory = (replacement or module).get('Dir')
        if directory:
            directory = Path(directory)
            assert directory.is_relative_to(stage) or directory.is_relative_to(env['GOMODCACHE'])
            for path in sorted(directory.iterdir()):
                if path.is_file() and path.name.upper().startswith(('LICENSE', 'COPYING', 'COPYRIGHT', 'UNLICENSE')):
                    assert not path.is_symlink() and path.stat().st_size <= 128 * 1024
                    item['license_files'].append({'path': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
        rows.append(item)
    artifacts = []
    for arch, machine in [('amd64', 62), ('arm64', 183)]:
        path = out / ('sdk-probe-linux-' + arch + '.test')
        selected = dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH=arch)
        command(['go', 'test', '-c', '-mod=readonly', '-trimpath', '-buildvcs=false', '-o', str(path), '.'], stage, selected, out, 'build-' + arch)
        assert path.stat().st_size <= 64 * 1024 * 1024
        with path.open('rb') as file:
            header = file.read(20)
        assert header[:6] == b'\x7fELF\x02\x01' and struct.unpack('<H',header[18:20])[0] == machine
        if arch == 'amd64':
            command([str(path), '-test.v', '-test.timeout=15s'], stage, env, out, 'native-artifact-tests', seconds=20)
        artifacts.append({'path':path.name,'architecture':arch,'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
                          'kind':'test probe; NOT an engine release','executed':arch=='amd64'})
    verify_patched(stage,sources,metadata)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', str(ROOT)], cwd=ROOT, text=True).strip()
    assert head==head_start and dirty==dirty_start,'candidate revision changed during evaluation'
    assert input_hashes=={name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in inputs},'candidate inputs changed'
    report = {'schema':1,'source_commit':head,'candidate_sources_dirty':bool(dirty),'upstream_commit':COMMIT,
              'input_sha256':input_hashes,
              'go':version,'upstream_files_verified':len(sources),'modules':rows,'artifacts':artifacts,
              'go_runtime_license':{'declaration':'Go BSD-style license','sha256':hashlib.sha256(runtime_license).hexdigest()},
              'native_runtime_cases':5,'upstream_runtime_cases':4,
              'compatibility_patch':metadata,'patched_runtime_cases':5,'patched_upstream_runtime_cases':4,
              'observed_blockers':['constructor_failure_retains_reconnect_worker','wallet_rpc_does_not_inherit_fixed_dialer',
                  'close_does_not_stop_active_reconnect_loop','malformed_rpc_body_echoed_to_logger','unbounded_upstream_rpc_response'],
              'network_scope':'synthetic in-memory and explicitly owned loopback only; denied other dial before DNS/socket',
              'production_ready':False,'real_nkn_network_tested':False,'engine_stdio_sdk_linked':False,
              'arm_executed':False,'inventory_scope':'actually compiled native test dependency closure; excludes unselected module graph',
              'license_review':'raw file evidence only; missing source remains unknown'}
    report['advisories'] = {'status':'not_run'}
    if advisory:
        report['advisories'] = advisories(rows,out)
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'sdk_cases':5,'upstream_cases':4,'verified_source_files':len(sources),'selected_module_closure':len(rows),
                      'selected_modules':sum(r['selected_by_probe'] for r in rows),'production_ready':False}))


def advisories(rows,out):
    findings=[]
    try:
        for offset in range(0,len(rows),100):
            batch=rows[offset:offset+100]
            queries=[{'package':{'ecosystem':'Go','name':r['path']},'version':r['version'].removeprefix('v')} for r in batch]
            request=urllib.request.Request('https://api.osv.dev/v1/querybatch',data=json.dumps({'queries':queries}).encode(),headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(request,timeout=30) as response: data=response.read(4*1024*1024+1)
            assert len(data)<=4*1024*1024
            (out/f'osv-{offset}.json').write_bytes(data)
            values=json.loads(data)['results'];assert len(values)==len(batch)
            for module,value in zip(batch,values):
                assert not value.get('next_page_token'),'incomplete advisory query'
                ids=sorted(v['id'] for v in value.get('vulns',[]))
                if ids: findings.append({'module':module['path'],'version':module['version'],'selected_by_probe':module['selected_by_probe'],
                                         'local_fork':module['source']=='checksum-pinned-local-fork','advisory_ids':ids})
        return {'status':'queried','queried_at_utc':datetime.now(timezone.utc).isoformat(),'source':'https://api.osv.dev/v1/querybatch',
                'findings':findings,'scope':'module/version matches only; no function reachability or local-fork patch validation'}
    except Exception as error:
        return {'status':'incomplete','error_class':type(error).__name__,'partial_findings':findings}


def safe_path(value):
    path = Path(value).absolute()
    assert '..' not in path.parts and path.is_relative_to(REPO/'work'), 'use repository work/'
    assert not any(p.is_symlink() for p in [path,*path.parents]), 'linked work path refused'
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run',action='store_true')
    parser.add_argument('--fetch-public-source',action='store_true')
    parser.add_argument('--source-cache')
    parser.add_argument('--output')
    parser.add_argument('--online-advisories',action='store_true')
    args = parser.parse_args()
    if not args.run and not args.fetch_public_source:
        print(json.dumps({'mode':'plan','execute':False,'network':False,'production_ready':False}))
        return 0
    try:
        if not args.source_cache:
            raise ValueError('explicit work/ source-cache required')
        cache = safe_path(args.source_cache)
        if args.fetch_public_source:
            fetch(cache)
        if args.run:
            if not args.output: raise ValueError('new work/ output required')
            out = safe_path(args.output); out.mkdir(mode=0o700)
            run(out,cache,args.online_advisories)
    except Exception as error:
        print('SDK evaluation incomplete:',type(error).__name__,str(error),file=sys.stderr)
        return 2
    return 0


if __name__=='__main__':
    raise SystemExit(main())
