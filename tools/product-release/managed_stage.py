# SPDX-License-Identifier: AGPL-3.0-only
"""Reuse the existing verified SDK/backport chain; never mutate input stages."""
from pathlib import Path
import importlib.util
import json
import shutil
import subprocess
import sys
import os
import stat


def source_file(path):
    # SDK archives legitimately contain an empty go.sum. Executables still use
    # the original nonempty arm.regular guard; only verified source permits 0.
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), 'rb') as stream:
        meta = os.fstat(stream.fileno())
        if not stat.S_ISREG(meta.st_mode) or meta.st_nlink != 1 or meta.st_size > 2*1024**2:
            raise ValueError('bounded SDK source required')
        raw = stream.read(2*1024**2+1)
        if len(raw) != meta.st_size:
            raise ValueError('SDK source changed while reading')
        return raw


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def prepare(repo, previous, output, env, revision):
    if not __debug__:
        raise ValueError('existing SDK assertion verifier requires ordinary Python')
    sys.path.insert(0, str(repo / 'tools'))
    import arm_build_manifest as arm
    producer = load(repo / 'tools/nkn-build/sdk-managed/prepare.py', 'existing_managed_prepare')
    assert previous.is_absolute() and previous.is_dir() and not previous.is_symlink()
    assert not output.exists()
    output.mkdir(mode=0o700)
    # Independent content/patch/module-sum verification from the existing code,
    # not acceptance of an arbitrary manifest supplied with the input stage.
    producer.status.verified_stage(previous, env, output)
    before = {str(p.relative_to(previous)): arm.sha(source_file(p))
              for p in previous.rglob('*') if p.is_file()}
    assert not any(p.is_symlink() for p in previous.rglob('*'))
    stage = output / 'stage'
    stage.mkdir(mode=0o700)
    for name in ['go.mod', 'go.sum', 'sdk_test.go', 'minimal_test.go', 'mask_test.go']:
        shutil.copyfile(previous/name, stage/name)
    for name in ['third_party', 'gorilla-websocket']:
        shutil.copytree(previous/'source'/name, stage/'source'/name)
    sdk = stage/'source/third_party/nkn-sdk-go'
    metadata = json.loads((producer.ROOT/'patch-manifest.json').read_text())
    assert producer.digest(producer.ROOT/'managed.patch') == metadata['patch_sha256']
    expected = producer.status.security.tree(sdk)
    for row in metadata['files']:
        assert producer.digest(sdk/row['path']) == row['before_sha256']
    subprocess.run(['patch', '--batch', '--fuzz=0', '-p1', '-i', str(producer.ROOT/'managed.patch')],
                   cwd=sdk, env=env, check=True, timeout=20, stdout=subprocess.DEVNULL)
    for row in metadata['files']:
        assert producer.digest(sdk/row['path']) == row['after_sha256']
        expected[row['path']] = row['after_sha256']
    assert producer.status.security.tree(sdk) == expected
    shutil.copyfile(producer.ROOT/'managed_test.go', sdk/'rustsimhub_managed_test.go')
    engine = stage/'source/engine'
    for file in (repo/'tools/nkn-engine').rglob('*'):
        if file.is_file() and file.suffix in ('.go', '.mod'):
            raw = arm.regular(file, 128*1024)
            target = engine/file.relative_to(repo/'tools/nkn-engine')
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
    with (stage/'go.mod').open('a') as stream:
        stream.write('\nrequire github.com/Viper-Boss/RustSimHub/tools/nkn-engine v0.0.0\n'
                     'replace github.com/Viper-Boss/RustSimHub/tools/nkn-engine => ./source/engine\n')
    assert before == {str(p.relative_to(previous)): arm.sha(source_file(p))
                      for p in previous.rglob('*') if p.is_file()}
    record = {'revision': revision, 'managed_patch_sha256': metadata['patch_sha256'],
              'source_files': {str(p.relative_to(stage)): arm.sha(source_file(p))
                               for p in stage.rglob('*') if p.is_file()}}
    (output/'inputs.json').write_text(json.dumps(record, sort_keys=True, indent=2)+'\n')
    return stage, record


def notices(repo, stage, programs, output, env):
    """Verify whole-program module closure and the existing exact license hashes."""
    from arm_build_manifest import bounded_run, regular, sha, require
    baseline = json.loads((repo/'docs/dependencies/nkn-websocket-mask-candidate.json').read_text())
    known = {r['path']: r for r in baseline['compiled_modules']}
    own = 'github.com/Viper-Boss/RustSimHub/tools/nkn-engine'
    modules = set()
    for name in ['nkn-root', 'nkn-sdk-engine']:
        raw = subprocess.check_output(['go', 'list', '-deps', '-mod=readonly', '-tags=nkn_sdk_candidate',
            '-f', '{{if .Module}}{{.Module.Path}}@{{.Module.Version}}{{end}}', own+'/cmd/'+name],
            cwd=stage, env=env, timeout=60)
        modules.update(raw.decode().split())
    modules.discard('@')
    selected = dict(item.rsplit('@', 1) for item in sorted(modules))
    not_linked = module_closure(set(selected), set(known), own)
    text, records = [], []
    for name, version in sorted(selected.items()):
        raw = subprocess.check_output(['go', 'list', '-mod=readonly', '-m', '-json', name], cwd=stage, env=env, timeout=20)
        info = json.loads(raw)
        require(info['Version'] == version, 'license module version mismatch')
        directory = Path(info.get('Replace', info)['Dir'])
        files = [{'path': 'LICENSE', 'sha256': sha(regular(repo/'LICENSE', 128*1024))}] if name == own else known[name]['license_files']
        if name == own:
            directory = repo
        else:
            require(known[name]['version'] == version, 'license version changed')
        require(files, 'module license unavailable')
        for row in files:
            part = Path(row['path'])
            require(not part.is_absolute() and '..' not in part.parts, 'license path refused')
            data = regular(directory/part, 128*1024, single_link=False)
            require(sha(data) == row['sha256'], 'license bytes changed')
            text.append(f'\n===== {name}@{version}: {part} =====\n'+data.decode()+'\n')
        records.append({'path': name, 'version': version, 'license_files': files})
    goroot = Path(bounded_run(['go', 'env', 'GOROOT']).decode().strip())
    version = bounded_run(['go', 'env', 'GOVERSION']).decode().strip()
    require(version == 'go1.27.3', 'pinned Go 1.27.3 required')
    data = regular(goroot/'LICENSE', 128*1024, single_link=False)
    text.append('\n===== Go runtime '+version+' =====\n'+data.decode())
    output.mkdir(mode=0o700)
    (output/'THIRD-PARTY-NOTICES.txt').write_text(''.join(text))
    result = {'modules': records, 'go': version, 'no_new_external_modules': True,
              'baseline_modules_not_linked_on_this_target': not_linked,
              'runtime_license_sha256': sha(data), 'dependency_approval': False}
    (output/'modules.json').write_text(json.dumps(result, sort_keys=True, indent=2)+'\n')
    return result


def module_closure(actual, baseline, own):
    # Build tags/architecture can omit an old module (e.g. x/sys on pure-Go
    # ARM). A missing linked module is not a new dependency or a source fix.
    if own not in actual or not actual <= baseline | {own}:
        raise ValueError('new SDK module closure requires separate review')
    return sorted(baseline - actual)
