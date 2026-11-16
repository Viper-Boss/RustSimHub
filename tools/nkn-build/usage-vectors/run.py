#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compare fixed SDK with independent encoding; offline, unprivileged Linux only."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def absolute(value):
    p = Path(value)
    if not p.is_absolute() or any(x.is_symlink() for x in [p, *p.parents]):
        raise ValueError('absolute unlinked path required')
    return p


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--go', required=True)
    parser.add_argument('--source-cache', required=True)
    parser.add_argument('--module-cache', required=True)
    parser.add_argument('--build-cache', required=True)
    parser.add_argument('--work-root', required=True)
    args = parser.parse_args()
    if sys.platform != 'linux' or os.geteuid() == 0:
        raise ValueError('ordinary Linux test user required')
    os.umask(0o077)
    go = absolute(args.go)
    work = absolute(args.work_root)
    if str(work).startswith('/mnt/'):
        raise ValueError('Linux-native work filesystem required')
    work.mkdir(parents=True, exist_ok=True)
    if work.stat().st_uid != os.getuid():
        raise ValueError('work root must belong to test user')
    run = Path(tempfile.mkdtemp(prefix='usage-vectors-', dir=work))
    stage = run / 'stage'
    stage.mkdir()
    env = {'HOME': str(Path.home()), 'PATH': str(go.parent) + ':/usr/bin:/bin',
           'LANG': 'C.UTF-8', 'TZ': 'UTC', 'TMPDIR': str(run)}
    env.update(GOWORK='off', GOENV='off', GOTOOLCHAIN='local', GOPROXY='off',
               GOSUMDB='off', GOMAXPROCS='4', CGO_ENABLED='0',
               GOMODCACHE=str(absolute(args.module_cache)),
               GOCACHE=str(absolute(args.build_cache)), GOPATH=str(run / 'gopath'))
    report = {'format': 1, 'passed': False, 'steps': [], 'source_sha256': {},
              'scope': 'offline reference vectors only; no production transport, Root capability or chain confirmation'}

    def command(name, argv, expected=0):
        process = subprocess.Popen(argv, cwd=stage, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            out, err = process.communicate(timeout=240)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            out, err = process.communicate(timeout=5)
            code = 124
        else:
            code = process.returncode
        if len(out) + len(err) > 2 * 1024 * 1024:
            raise ValueError('probe output bound exceeded')
        (run / (name + '.stdout')).write_bytes(out)
        (run / (name + '.stderr')).write_bytes(err)
        report['steps'].append({'name': name, 'command': argv, 'exit_code': code,
                                'stdout_sha256': sha(out), 'stderr_sha256': sha(err)})
        if code != expected:
            raise ValueError(name + ': unexpected exit; inspect bounded logs')
        return out, err

    try:
        version, _ = command('go-version', [str(go), 'version'])
        if version.strip() != b'go version go1.27.3 linux/amd64':
            raise ValueError('fixed Go 1.27.3 linux/amd64 required')
        evaluation = ROOT.parent / 'sdk-evaluation'
        spec = importlib.util.spec_from_file_location('fixed_sdk_evaluation', evaluation / 'evaluate.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        files = module.verified_cache(absolute(args.source_cache))
        for name, data in files.items():
            target = stage / 'source' / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            report['source_sha256'][name] = sha(data)
        for name in ['go.mod', 'go.sum']:
            data = (evaluation / name).read_bytes()
            (stage / name).write_bytes(data)
            report['source_sha256'][name] = sha(data)
        program = (ROOT / 'main.go').read_bytes()
        (stage / 'main.go').write_bytes(program)
        report['source_sha256']['usage-vectors/main.go'] = sha(program)
        report['source_sha256']['usage-vectors/run.py'] = sha(Path(__file__).read_bytes())
        command('gofmt', [str(go.parent / 'gofmt'), '-d', 'main.go'])
        if (run / 'gofmt.stdout').stat().st_size:
            raise ValueError('Go source is not formatted')
        command('module-verify', [str(go), 'mod', 'verify'])
        command('build', [str(go), 'build', '-mod=readonly', '-o', str(run / 'vectors'), '.'])
        output, _ = command('positive', [str(run / 'vectors')])
        expected = (ROOT / 'vectors.json').read_bytes()
        report['source_sha256']['usage-vectors/vectors.json'] = sha(expected)
        if output != expected:
            raise ValueError('fresh fixed-SDK output differs from committed vectors')
        payload = json.loads(output)
        if payload['http_calls'] != 0 or len(payload['vectors']) != 15:
            raise ValueError('vector coverage or zero-HTTP guard failed')
        report['vector_count'] = 15
        report['fresh_signature_rejections'] = 30
        report['vectors_sha256'] = sha(output)
        for mutation in ['intent-hash', 'single-hash', 'big-endian', 'sdk-attributes']:
            _, stderr = command('negative-' + mutation,
                                [str(run / 'vectors'), '-negative', mutation], 1)
            if b'offline vector assertion:' not in stderr:
                raise ValueError('negative failed without a behavioral assertion')
        command('vet', [str(go), 'vet', '-mod=readonly', '.'])
        if any((stage / n).read_bytes() != (evaluation / n).read_bytes() for n in ['go.mod', 'go.sum']):
            raise ValueError('original module/lock changed')
        report['passed'] = True
    except Exception as error:
        report['error'] = str(error)
    finally:
        (run / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'passed': report['passed'], 'result': str(run / 'result.json'),
                      'steps': [{'name': r['name'], 'exit_code': r['exit_code']} for r in report['steps']]}))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
