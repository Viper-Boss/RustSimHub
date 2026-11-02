#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Isolated negative witness: lose post-PCM time observation, retain real test."""
import hashlib,io,json,os,pathlib,subprocess,tarfile,tempfile
PROJECT=pathlib.Path(__file__).resolve().parents[3]
def main():
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip()
    archive=subprocess.check_output(['git','archive','--format=tar',head],cwd=PROJECT)
    work=PROJECT/'work';work.mkdir(exist_ok=True)
    directory=pathlib.Path(tempfile.mkdtemp(prefix='remote-clock-negative-',dir=work))
    source=directory/'source';source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as tree:tree.extractall(source,filter='data')
    file=source/'crates/media/src/pump.rs';before=file.read_text()
    actual='self.step_inner(now, Some(clock))';injected='self.step_inner(now, None::<fn() -> u64>)'
    assert before.count(actual)==1,'exact fresh-clock entry required'
    file.write_text(before.replace(actual,injected))
    # Only private copied source changes. The assertion and original step(now)
    # are unchanged. This reproduces the initial remote candidate using a frozen
    # step time across external I/O; it is not a claim about a historical exploit.
    env=dict(os.environ,CARGO_TARGET_DIR=str(PROJECT/'target'),CARGO_INCREMENTAL='0')
    command=['cargo','test','-p','rustsimhub-media','--lib','--locked','--offline',
        'pump::tests::live_clock_rechecks_expiry_and_frame_age_after_external_capture','--','--exact','--nocapture']
    with (directory/'negative.log').open('w') as log:
        result=subprocess.run(command,cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT)
    log=(directory/'negative.log').read_text()
    assert result.returncode==101 and 'written.iter().all' in log and '1 failed;' in log,'unchanged real regression assertion must fail'
    output={'source_head':head,'source_archive_sha256':hashlib.sha256(archive).hexdigest(),
        'fault_injection':'only lose post-I/O clock refresh in new live-clock entry','production_source_modified':False,
        'test_unchanged':True,'exit_code':result.returncode,'expected_tests_failed':1,'log_sha256':hashlib.sha256((directory/'negative.log').read_bytes()).hexdigest(),
        'pump_before_sha256':hashlib.sha256(before.encode()).hexdigest(),'pump_injected_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),
        'log_path':str(directory/'negative.log')}
    (directory/'result.json').write_text(json.dumps(output,indent=2)+'\n');print(json.dumps(output,sort_keys=True))
if __name__=='__main__':main()
