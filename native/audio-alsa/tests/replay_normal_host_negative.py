#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Keep the real Core cancellation assertion; omit only the host's outer fence."""
import hashlib,io,json,os,pathlib,subprocess,tarfile,tempfile
PROJECT=pathlib.Path(__file__).resolve().parents[3]
def main():
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=PROJECT,text=True).strip()
    raw=subprocess.check_output(['git','archive','--format=tar',head],cwd=PROJECT)
    directory=pathlib.Path(tempfile.mkdtemp(prefix='normal-host-negative-',dir=PROJECT/'work'));source=directory/'source';source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:archive.extractall(source,filter='data')
    path=source/'crates/media/src/remote_signaling/mod.rs';before=path.read_text();start=before.index('    pub async fn poll(');end=before.index('    pub async fn activate(',start)
    poll=before[start:end];arm='        let guard = self.worker.cancellation_guard();\n';disarm='        guard.disarm();\n'
    assert poll.count(arm)==1 and poll.count(disarm)==1
    path.write_text(before[:start]+poll.replace(arm,'').replace(disarm,'')+before[end:])
    env=dict(os.environ,CARGO_TARGET_DIR=str(PROJECT/'target'),CARGO_INCREMENTAL='0')
    case='calls::checks::remote_media::cancelling_host_during_normal_core_observation_closes_retained_worker_before_its_own_exchange'
    command=['cargo','test','-p','rustsimhub-core','--bin','rustsimhub-core','--features','remote-media-tests','--locked','--offline',case,'--','--exact','--nocapture']
    with (directory/'negative.log').open('w') as log:result=subprocess.run(command,cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT)
    text=(directory/'negative.log').read_text();assert result.returncode==101 and '1 failed;' in text and 'Elapsed' in text,'same original cancellation/EOF assertion must fail'
    report={'source_head':head,'source_archive_sha256':hashlib.sha256(raw).hexdigest(),'fault':'only omit outer worker-connection fence during normal Core poll await',
        'test_unchanged':True,'production_tree_modified':False,'expected_failed_tests':1,'exit_code':result.returncode,
        'before_sha256':hashlib.sha256(before.encode()).hexdigest(),'injected_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
        'log_sha256':hashlib.sha256((directory/'negative.log').read_bytes()).hexdigest(),'log_path':str(directory/'negative.log')}
    (directory/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,sort_keys=True))
if __name__=='__main__':main()
