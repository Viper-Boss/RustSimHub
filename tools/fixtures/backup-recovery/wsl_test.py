#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Fixed Git source -> unprivileged WSL2 Linux filesystem -> actual test logs.

Default prints a plan. Windows --run uses the existing Ubuntu test environment.
No installation, elevation, devices, services, source edits or new dependencies.
The ref may be a commit or an explicitly supplied staged tree (reported as such).
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

SUITES = ('recovery', 'compat', 'workspace', 'native', 'update')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def mount_path(path):
    path = str(path.resolve()).replace('\\', '/')
    if not re.match(r'^[A-Za-z]:/', path):
        raise ValueError('Expected an absolute Windows drive path')
    return '/mnt/' + path[0].lower() + path[2:]


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root).decode('utf-8').strip()


def execute_step(source, results, name, command, env, timeout):
    print('START ' + name, flush=True)
    started = time.monotonic()
    with (results / (name + '.log')).open('w', encoding='utf-8') as log:
        log.write(json.dumps(command) + '\n')
        log.flush()
        try:
            child = subprocess.Popen(command, cwd=source, env=env,
                                     stdin=subprocess.DEVNULL, stdout=log,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                log.write('\nbounded timeout\n')
                code = 124
        except OSError as error:
            log.write(type(error).__name__ + ': ' + str(error) + '\n')
            code = 127
        log.write('\nexit_code=' + str(code) + '\n')
    print('END ' + name + ' exit=' + str(code), flush=True)
    return {'name':name, 'command':command, 'exit_code':code,
            'elapsed_seconds':round(time.monotonic() - started, 3),
            'log':name + '.log'}


def worker(meta_path):
    results = meta_path.parent
    metadata = json.loads(meta_path.read_text(encoding='utf-8'))
    report = {'source':metadata, 'steps':[], 'passed':False, 'error':None,
              'started_at':datetime.now(timezone.utc).isoformat()}
    try:
        if sys.platform != 'linux' or os.geteuid() == 0:
            raise ValueError('Tests require an ordinary Linux user')
        if 'wsl2' not in platform.release().lower():
            raise ValueError('Expected an actual WSL2 kernel')
        if digest(Path(__file__).resolve()) != metadata['executor_sha256']:
            raise ValueError('Executor changed after snapshot creation')
        archive = results / 'source.tar'
        if digest(archive) != metadata['archive_sha256']:
            raise ValueError('Git archive digest mismatch')
        os.umask(0o077)
        native_base = Path.home() / 'rustsimhub-tests'
        if str(native_base.resolve()).startswith('/mnt/'):
            raise ValueError('Linux-native filesystem required')
        native_base.mkdir(mode=0o700, exist_ok=True)
        work = Path(tempfile.mkdtemp(prefix=metadata['revision'][:12] + '-', dir=native_base))
        source = work / 'source'
        source.mkdir(mode=0o700)
        with tarfile.open(archive, 'r:') as bundle:
            names = set()
            total = 0
            for member in bundle.getmembers():
                path = PurePosixPath(member.name)
                if (path.is_absolute() or '..' in path.parts or '\\' in member.name
                        or any(p in {'.git','.local','.private'} for p in path.parts)
                        or not (member.isdir() or member.isfile())
                        or member.name in names):
                    raise ValueError('Unexpected Git archive entry')
                names.add(member.name)
                total += member.size
                if len(names) > 8192 or total > 128 * 1024 * 1024:
                    raise ValueError('Source archive budget exceeded')
                target = source.joinpath(*path.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with bundle.extractfile(member) as reader, target.open('xb') as output:
                        while block := reader.read(1024 * 1024):
                            output.write(block)
                    target.chmod(member.mode & 0o777)
        temporary = work / 'tmp'
        temporary.mkdir(mode=0o700)
        report.update({'kernel':platform.release(), 'uid':os.geteuid(),
                       'source_files':len([p for p in source.rglob('*') if p.is_file()]),
                       'native_workspace':str(work)})
        env = os.environ.copy()
        env.update({'PATH':str(Path.home()/'.cargo/bin') + ':/usr/local/bin:/usr/bin:/bin',
                    'RUSTUP_TOOLCHAIN':'1.98.0', 'CARGO_BUILD_JOBS':'2',
                    'CARGO_TARGET_DIR':str(work/'target'), 'TMPDIR':str(temporary),
                    'GO111MODULE':'off'})
        for name in ('CARGO_MAKEFLAGS','MAKEFLAGS'):
            env.pop(name, None)
        suite = metadata['suite']
        native = ['--manifest-path','native/audio-alsa/Cargo.toml'] if suite == 'native' else []
        fetch = ['cargo','fetch',*native,'--locked'] + ([] if metadata['fetch'] else ['--offline'])
        steps = [('01-rustc',['rustc','--version'],30),
                 ('02-cargo',['cargo','--version'],30),
                 ('03-dependencies',fetch,600),
                 ('04-format',['cargo','fmt','--all','--','--check'],120)]
        if suite == 'recovery':
            steps += [('05-controller',['python3','-m','unittest','discover','-s',
                       'tools/fixtures/backup-recovery','-p','test_runner.py'],120),
                      ('06-matrix',['python3','tools/test-business-recovery-matrix.py','--run'],900)]
        if suite == 'update':
            steps += [('06-matrix',['python3','tools/test-update-failure-matrix.py','--run'],1000)]
        package = [] if suite in ('workspace','native') else ['-p','rustsimhub-'+suite]
        features = ['--all-features'] if suite == 'workspace' else []
        steps += [('07-cargo-test',['cargo','test',*native,*package,*features,'--locked','--offline'],1800),
                  ('08-clippy',['cargo','clippy',*native,*package,'--all-targets',*features,
                                 '--locked','--offline','--','-D','warnings'],1200)]
        if suite == 'compat':
            steps += [('09-go-test',['go','test','-count=1','-timeout=60s','./tools/nas-adapter'],120),
                      ('10-go-vet',['go','vet','./tools/nas-adapter'],120)]
        for name, command, timeout in steps:
            item = execute_step(source, results, name, command, env, timeout)
            report['steps'].append(item)
            if name == '03-dependencies' and item['exit_code']:
                raise ValueError('Dependency preparation failed; see its log')
        report['passed'] = all(item['exit_code'] == 0 for item in report['steps'])
    except (OSError, ValueError, tarfile.TarError) as error:
        report['error'] = type(error).__name__ + ': ' + str(error)
    finally:
        report['finished_at'] = datetime.now(timezone.utc).isoformat()
        (results/'summary.json').write_text(json.dumps(report, indent=2)+'\n',encoding='utf-8')
        with zipfile.ZipFile(results/'evidence.zip','w',compression=zipfile.ZIP_DEFLATED) as output:
            for file in sorted(results.iterdir()):
                if file.name in ('source.json','summary.json') or file.suffix == '.log':
                    output.write(file,file.name)
        print('RESULT passed=' + str(report['passed']).lower(), flush=True)
    return 0 if report['passed'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--suite', choices=SUITES, default='recovery')
    parser.add_argument('--ref', default='HEAD')
    parser.add_argument('--fetch', action='store_true')
    parser.add_argument('--distribution', default='Ubuntu-24.04')
    parser.add_argument('--user', default='rsh-test')
    parser.add_argument('--worker', type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.worker is not None:
        return worker(args.worker)
    if not args.run:
        print(json.dumps({'execute':False,'suite':args.suite,'ref':args.ref,
                          'platform':'WSL2','native_linux_filesystem':True}))
        return 0
    if sys.platform != 'win32':
        parser.error('Invoke the WSL launcher from Windows')
    root = Path(__file__).resolve().parents[3]
    revision = git(root,'rev-parse','--verify',args.ref)
    kind = git(root,'cat-file','-t',revision)
    if kind not in ('commit','tree'):
        parser.error('Ref must resolve to a commit or an explicitly supplied tree')
    location = root/'.local/wsl-tests'
    location.mkdir(parents=True, exist_ok=True)
    results = Path(tempfile.mkdtemp(prefix=datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ-'), dir=location))
    archive = results/'source.tar'
    subprocess.run(['git','archive','--format=tar','--output',str(archive),revision],cwd=root,check=True)
    metadata = {'revision':revision,'kind':kind,'tree':git(root,'rev-parse',revision+'^{tree}'),
                'base_head':git(root,'rev-parse','HEAD'),'suite':args.suite,'fetch':args.fetch,
                'archive_sha256':digest(archive),'executor_sha256':digest(Path(__file__).resolve()),
                'distribution':args.distribution,'test_user':args.user}
    meta = results/'source.json'
    meta.write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf-8')
    print('RESULTS ' + str(results), flush=True)
    command = ['wsl.exe','--distribution',args.distribution,'--user',args.user,
               '--exec','python3',mount_path(Path(__file__)),'--worker',mount_path(meta)]
    return subprocess.run(command,cwd=root).returncode


if __name__ == '__main__':
    sys.exit(main())
