#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Replay fixed C19 synthetic cases against the real C18 CLI. Plan by default."""
import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CORPUS = ROOT/'docs/kernel-artifact-audit/corpus.json'
spec = importlib.util.spec_from_file_location('matrix_io', HERE/'audit.py')
io = importlib.util.module_from_spec(spec)
spec.loader.exec_module(io)
FILES = {'image':'Image', 'dtb':'board.dtb', 'module':'synthetic.ko', 'firmware':'firmware.bin'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load_corpus():
    with io.Directory(ROOT) as directory:
        raw = directory.read('docs/kernel-artifact-audit/corpus.json', 128*1024)
    corpus = io.parse_json(raw)
    io.fields(corpus, ['schema','synthetic','kernel_release','board_compatible','baseline','cases'])
    io.need(type(corpus['schema']) is int and corpus['schema'] == 1
            and corpus['synthetic'] is True, 'corpus_schema')
    io.need(len(corpus['baseline']) == 4 and len(corpus['cases']) == 22, 'corpus_count')
    io.need({r['id']:r['path'] for r in corpus['baseline']} == FILES, 'corpus_paths')
    ids = [r['id'] for r in corpus['cases']]
    io.need(len(set(ids)) == 22 and all(re.fullmatch(r'[a-z][a-z0-9-]{0,63}', s) for s in ids), 'case_id')
    for row in corpus['baseline']:
        io.need(len(bytes.fromhex(row['hex'])) <= 4096, 'fixture_budget')
    for row in corpus['cases']:
        io.need(row['expected']['exit_code'] in (0,1,3,4), 'expected_exit')
        if 'hex' in row['mutation']:
            io.need(len(bytes.fromhex(row['mutation']['hex'])) <= 4096, 'fixture_budget')
    return corpus, sha(raw)


def snapshot(paths):
    result = {}
    for path in paths:
        if not os.path.lexists(path):
            result[path.name] = {'type':'missing'}
            continue
        facts = path.lstat()
        if stat.S_ISLNK(facts.st_mode):
            result[path.name] = {'type':'symlink','target':os.readlink(path)}
        else:
            io.need(stat.S_ISREG(facts.st_mode) and facts.st_size <= 128*1024, 'snapshot_budget')
            data = path.read_bytes()
            result[path.name] = dict(type='file',bytes=len(data),sha256=sha(data),mode=stat.S_IMODE(facts.st_mode))
    return result


def prepare(corpus, case, directory):
    artifacts = directory/'artifacts'
    artifacts.mkdir(mode=0o700)
    manifest = dict(schema=1,mode='synthetic',kernel_release=corpus['kernel_release'],
                    board_compatible=corpus['board_compatible'],entries=[])
    for original in corpus['baseline']:
        entry = copy.deepcopy(original)
        data = bytes.fromhex(entry.pop('hex'))
        (artifacts/entry['path']).write_bytes(data)
        entry.update(bytes=len(data),sha256=sha(data))
        manifest['entries'].append(entry)
    entries = {r['id']:r for r in manifest['entries']}
    mutation = case['mutation']
    op = mutation['op']
    if op == 'replace_bytes' or op == 'dependencies':
        entry = entries[mutation.get('entry','module')]
        data = bytes.fromhex(mutation['hex'])
        (artifacts/entry['path']).write_bytes(data)
        if mutation.get('rehash',True):
            entry.update(bytes=len(data),sha256=sha(data))
        if op == 'dependencies':
            entry['metadata']['depends'] = mutation['value']
    elif op == 'remove_file':
        (artifacts/FILES[mutation['entry']]).unlink()
    elif op == 'release':
        manifest['kernel_release'] = mutation['value']
    elif op == 'srcversion':
        entries['module']['metadata']['srcversion'] = mutation['value']
    elif op == 'remove_entry':
        manifest['entries'].remove(entries[mutation['entry']])
    elif op == 'license_null':
        entries['firmware']['metadata']['license'] = None
    elif op == 'schema':
        manifest['schema'] = mutation['value']
    elif op == 'path':
        entries['image']['path'] = mutation['value']
    elif op == 'symlink':
        keeper = directory/'keeper'
        keeper.write_bytes(b'synthetic-do-not-read-or-change')
        path = artifacts/FILES['firmware']
        path.unlink()
        path.symlink_to('../keeper')
    elif op == 'duplicate_path':
        entries['dtb']['path'] = entries['image']['path']
    elif op == 'existing_report':
        (directory/'report.json').write_bytes(b'previous-evidence')
    elif op == 'oversize_declaration':
        entries['image']['bytes'] = io.LIMITS['kernel_image']+1
    elif op == 'passed_assertion':
        manifest['passed'] = True
    else:
        io.need(op in ('none','duplicate_schema'), 'mutation')
    raw = json.dumps(manifest, sort_keys=True)
    if op == 'duplicate_schema':
        raw = raw.replace('"schema": 1', '"schema": 1, "schema": 1', 1)
    path = directory/'manifest.json'
    path.write_text(raw+'\n',encoding='utf-8')
    return artifacts, path


def matches(expected, code, report):
    if code != expected['exit_code'] or report.get('exit_code') != code:
        return False
    if report.get('authorization_verified') is not False:
        return False
    if code != 1 and (report.get('source_trust') != 'synthetic_only'
                     or any(report.get(s) != 'not_verified' for s in
                            ('boot','recovery','release_support','firmware_license'))):
        return False
    if 'reason' in expected and report.get('reason') != expected['reason']:
        return False
    if 'gap' in expected and expected['gap'] not in report.get('gaps',[]):
        return False
    if 'unresolved' in expected and expected['unresolved'] != report.get('unresolved_module_dependencies'):
        return False
    if 'cycle' in expected and expected['cycle'] != report.get('dependency_cycle'):
        return False
    rows = report.get('observations',[])
    if expected.get('all_observed') and (len(rows) != 4 or any(r['status'] != 'observed' for r in rows)):
        return False
    if 'observation' in expected:
        target = expected['observation']
        found = [r for r in rows if r['id'] == target['id']]
        if len(found) != 1 or target['reason'] not in found[0]['reasons']:
            return False
        if found[0]['status'] != ('mismatch' if code == 4 else 'not_verified'):
            return False
    return True


def run(corpus, corpus_sha, output):
    rows = []
    for case in corpus['cases']:
        directory = output/case['id']
        directory.mkdir(mode=0o700)
        artifacts, manifest = prepare(corpus, case, directory)
        report_path = directory/'report.json'
        paths = [manifest, *[artifacts/n for n in FILES.values()]]
        if (directory/'keeper').exists():
            paths.append(directory/'keeper')
        before = snapshot(paths)
        existing = report_path.read_bytes() if report_path.exists() else None
        command = [sys.executable,'-B',str(HERE/'audit.py'),'--run',
                   '--manifest',str(manifest),'--artifacts',str(artifacts),'--report',str(report_path)]
        completed = subprocess.run(command,env={'PATH':'/usr/bin:/bin','LANG':'C.UTF-8'},
                                   stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
        io.need(len(completed.stdout) <= 65536 and not completed.stderr, 'checker_output')
        report = io.parse_json(completed.stdout)
        unchanged = snapshot(paths) == before
        persisted = (existing is not None and report_path.read_bytes() == existing
                     or existing is None and (not report_path.exists() if completed.returncode == 1
                     else io.parse_json(report_path.read_bytes()) == report))
        matched = matches(case['expected'], completed.returncode, report)
        row = dict(id=case['id'],category=case['category'],expected=case['expected'],
                   actual_exit=completed.returncode,result=report,input_snapshot=before,
                   inputs_unchanged=unchanged,report_preserved_or_consistent=persisted,
                   passed=matched and unchanged and persisted)
        (directory/'result.json').write_text(json.dumps(row,sort_keys=True)+'\n',encoding='utf-8')
        rows.append(row)
    result = dict(schema=1,synthetic=True,execute=True,corpus_sha256=corpus_sha,
                  checker_sha256=sha((HERE/'audit.py').read_bytes()),
                  uid=os.geteuid(),cases_run=len(rows),cases_passed=sum(r['passed'] for r in rows),
                  passed=len(rows)==22 and all(r['passed'] for r in rows),cases=rows,
                  release_support='not_verified',real_artifacts_verified=False)
    (output/'matrix-results.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run',action='store_true')
    parser.add_argument('--output',type=Path)
    args = parser.parse_args()
    if not args.run:
        print(json.dumps(dict(execute=False,case_count=22,synthetic=True,device_access=False)))
        return 0
    try:
        io.need(sys.platform=='linux' and os.geteuid()!=0,'unprivileged_linux_required')
        corpus, digest = load_corpus()
        if args.output is None:
            with tempfile.TemporaryDirectory(prefix='rustsimhub-c19-') as temp:
                result = run(corpus,digest,Path(temp))
        else:
            with io.Directory(args.output.parent) as parent:
                parts = io.relative(args.output.name)
                io.need(len(parts)==1,'output')
                os.mkdir(parts[0],mode=0o700,dir_fd=parent.fd)
            result = run(corpus,digest,args.output)
        print(json.dumps(result,sort_keys=True))
        return 0 if result['passed'] else 1
    except (io.Invalid,io.Missing,OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError):
        print(json.dumps(dict(execute=True,passed=False,error='fixture_or_execution_error')))
        return 1


if __name__=='__main__':
    sys.dont_write_bytecode=True
    raise SystemExit(main())
