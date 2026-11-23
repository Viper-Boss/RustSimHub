#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual Core/Auth/Machine -> original Root/cgroup -> fixed SDK -> private TLS.

Requires a real disposable private cgroup/network/PID/mount container. Never run
on the host. The node is a synthetic protocol peer, not a public chain proof.
"""
from pathlib import Path
import ctypes,errno,hashlib,importlib.util,json,os,resource,signal,struct,subprocess,sys,time

base_path=Path(__file__).resolve().parents[1]/'nkn-build/sdk-managed/root_kernel_fixture.py'
spec=importlib.util.spec_from_file_location('original_root_fixture',base_path)
base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)

def rpc(request):
    assert request.get('id')=='rustsimhub-statistics-transaction-v1'
    assert set(request)=={'id','method','params'}
    method=request['method'];params=request['params'];assert isinstance(params,dict) and len(params)==1
    result=None;record={'method':method}
    if method=='getnoncebyaddr':
        assert set(params)=={'address'} and isinstance(params['address'],str) and len(params['address'])<80
        record['address']=params['address'];result={'nonce':40,'nonceInTxPool':41}
    elif method=='sendrawtransaction':
        assert set(params)=={'tx'};raw=bytes.fromhex(params['tx']);assert 0<len(raw)<=768
        tx=base.proto_parse(raw);assert set(tx)=={1,2} and all(len(v)==1 for v in tx.values())
        unsigned=base.proto_parse(tx[1][0]);assert set(unsigned)=={1,2} and unsigned[2]==[41]
        payload=base.proto_parse(unsigned[1][0]);assert payload[1]==[6] and set(payload)=={1,2}
        subscription=base.proto_parse(payload[2][0]);assert set(subscription)=={1,3,5,6}
        public=json.loads(Path('/tmp/statistics-public.json').read_text())['statistics_public_key']
        assert subscription[1]==[bytes.fromhex(public)]
        contract=json.loads(subscription[6][0]);assert isinstance(contract,dict)
        data=payload[2][0]
        size=bytes([len(data)]) if len(data)<253 else b'\xfd'+struct.pack('<H',len(data))
        preimage=struct.pack('<I',6)+size+data+struct.pack('<QQ',41,0)+b'\x00'
        digest=hashlib.sha256(hashlib.sha256(preimage).digest()).hexdigest()
        record.update({'digest':digest,'intent':contract,'transaction_sha256':hashlib.sha256(raw).hexdigest(),'nonce':41,'bytes':len(raw)})
        if os.environ['RSH_STATS_SCENARIO']=='root_expiry_300':
            expires=int(Path('/tmp/statistics-expiry-boot-ms').read_text())
            remaining=expires-int(time.clock_gettime(time.CLOCK_BOOTTIME)*1000)
            assert 0<remaining<=800, 'synthetic node did not receive the original submit before expiry'
            record['node_submit_remaining_ms']=remaining
        result=digest
    elif method=='gettransaction':
        assert set(params)=={'hash'} and len(params['hash'])==64
        records=[json.loads(x) for x in Path('/tmp/statistics-rpc.jsonl').read_text().splitlines()]
        original=[x for x in records if x['method']=='sendrawtransaction'];assert len(original)==1 and params['hash']==original[0]['digest']
        record['digest']=params['hash'];result={'fixture':'no approved chain inclusion evidence'}
    else:raise AssertionError('unexpected RPC method '+method)
    with Path('/tmp/statistics-rpc.jsonl').open('a') as stream:stream.write(json.dumps(record,separators=(',',':'))+'\n')
    if method=='sendrawtransaction':
        base.write('/tmp/statistics-submit-entered','1')
        scenario=os.environ['RSH_STATS_SCENARIO']
        if scenario in ['off','cancel','root_revoke','owner_inactive_after','owner_restore_after']:
            base.eventually(lambda:Path('/tmp/statistics-interruption-complete').exists(),.7)
            time.sleep(.08)
        if scenario=='late_reply':time.sleep(2.3)
        if scenario in ['lost_ack','process_restart']:return None
        if scenario=='root_expiry_300':
            time.sleep(max(0,(expires+100)/1000-time.clock_gettime(time.CLOCK_BOOTTIME)))
            base.write('/tmp/statistics-late-expiry-reply','1')
    return {'id':request['id'],'jsonrpc':'2.0','result':result}

def main():
    if len(sys.argv)>1 and sys.argv[1]=='--node':base.node(rpc,fail_bootstrap=os.environ['RSH_STATS_SCENARIO']=='bootstrap_failure');return 0
    scenario=os.environ['RSH_STATS_SCENARIO'];assert scenario in ['startup','shared','lost_ack','off','cancel','off_before','cancel_before','expired_prepare','late_reply','root_revoke','save_failure','save_full','start_failure','bootstrap_failure','owner_inactive_before','owner_restore_before','owner_inactive_after','owner_restore_after','process_restart','root_expiry_300']
    _,node=base.install_fixture(usage_transactions=True,node_entry=Path(__file__).resolve())
    if scenario=='start_failure':Path('/opt/rustsimhub/selected/nkn-sdk-engine').chmod(0o444)
    if scenario=='save_full':
        Path('/tmp/statistics-disk').mkdir()
        assert ctypes.CDLL(None,use_errno=True).mount(b'tmpfs',b'/tmp/statistics-disk',b'tmpfs',0,b'size=65536,mode=0700,uid=1001,gid=1001')==0, 'owned tmpfs mount failed'
    last_network_sample = None
    def network_status():
        nonlocal last_network_sample
        sample = int(time.time())
        if sample == last_network_sample:
            return
        last_network_sample = sample
        snapshot={'schema':1,'available':True,'boot_id':Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'sample_unix_seconds':sample,'devices':[{'interface':'wlan0','kind':'wifi','state':'100 (connected)','ipv4':['9.9.9.8/24'],'ipv6':[],'dns_ipv4':['9.9.9.9'],'dns_ipv6':[]}]}
        target = Path('/run/rustsimhub-network/status.json')
        pending = target.with_suffix(f'.fixture-{sample}')
        pending.write_text(json.dumps(snapshot))
        if target.exists():
            # Exchange, retaining the old inode in the disposable container.
            # An in-flight Root read must still see the required single link.
            rename_exchange = ctypes.CDLL(None, use_errno=True).renameat2
            if rename_exchange(-100, os.fsencode(pending), -100, os.fsencode(target), 2) != 0:
                error = ctypes.get_errno()
                raise OSError(error, os.strerror(error), target)
        else:
            pending.replace(target)
    network_status()
    log=Path('/tmp/core-statistics.log')
    with log.open('wb') as output:
        def start_core(reopen=False):
            env=dict(os.environ,RSH_STATS_KERNEL_FIXTURE='1')
            env.pop('RSH_STATS_REOPEN',None)
            if reopen:env['RSH_STATS_REOPEN']='1'
            return subprocess.Popen(['/core-test','--ignored','--exact','nkn_root::check_in::tests::statistics_transport_kernel_sdk','--nocapture'],user=1001,group=1001,extra_groups=[],env=env,stdout=output,stderr=subprocess.STDOUT,restore_signals=False,preexec_fn=lambda:signal.signal(signal.SIGXFSZ,signal.SIG_IGN))
        core=start_core()
        base.eventually(lambda:Path('/tmp/statistics-public.json').exists(),8)
        public=json.loads(Path('/tmp/statistics-public.json').read_text());assert set(public)=={'core_pin','statistics_public_key'}
        document=json.loads(Path('/etc/rustsimhub/nkn-policy.json').read_text());document.update(public)
        Path('/etc/rustsimhub/nkn-policy.json').write_text(json.dumps(document,separators=(',',':')))
        root=os.fork()
        if root==0:
            base.write(base.UNIT/'cgroup.procs',os.getpid())
            assert ctypes.CDLL(None,use_errno=True).prctl(38,1,0,0,0)==0
            os.execv('/root-provider',['/root-provider','--serve-fixed'])
        try:
            root_stopped=lambda:os.waitid(os.P_PID,root,os.WEXITED|os.WNOHANG|os.WNOWAIT) is not None
            if scenario=='start_failure':base.eventually(root_stopped,5)
            else:base.eventually(lambda:Path(base.ROOT_SOCKET).exists(),5)
            base.write('/tmp/statistics-fixture-ready','1')
            deadline=time.monotonic()+(320 if scenario=='root_expiry_300' else 30)
            original_file_limit=None;file_limit_restored=False;disk_filled=False;partial_sdk_pids=set()
            while core.poll() is None and time.monotonic()<deadline:
                if scenario=='bootstrap_failure' and Path('/tmp/statistics-bootstrap-refused').exists():
                    procs=base.UNIT/'engine/cgroup.procs'
                    if procs.is_file():
                        for pid in procs.read_text().split():
                            process=Path('/proc')/pid
                            try:
                                if process.stat().st_uid==1002 and (process/'exe').resolve()==Path('/opt/rustsimhub/selected/nkn-sdk-engine'):partial_sdk_pids.add(pid)
                            except FileNotFoundError:pass
                if scenario=='save_failure':
                    requested=Path('/tmp/statistics-write-limit-request').exists()
                    if requested and original_file_limit is None:
                        original_file_limit=resource.prlimit(core.pid,resource.RLIMIT_FSIZE)
                        resource.prlimit(core.pid,resource.RLIMIT_FSIZE,(0,original_file_limit[1]))
                        base.write('/tmp/statistics-write-limit-active','1')
                    elif original_file_limit is not None and not requested and not file_limit_restored:
                        resource.prlimit(core.pid,resource.RLIMIT_FSIZE,original_file_limit);file_limit_restored=True
                        base.write('/tmp/statistics-write-limit-restored','1')
                elif scenario=='save_full':
                    requested=Path('/tmp/statistics-write-limit-request').exists()
                    filler=Path('/tmp/statistics-disk/fill')
                    if requested and not disk_filled:
                        try:filler.write_bytes(b'x'*65536)
                        except OSError as error:assert error.errno==errno.ENOSPC
                        else:raise AssertionError('small owned tmpfs did not fill')
                        assert os.statvfs(filler).f_bavail==0;disk_filled=True
                        base.write('/tmp/statistics-write-limit-active','1')
                    elif disk_filled and not requested and not file_limit_restored:
                        filler.unlink();file_limit_restored=True
                        base.write('/tmp/statistics-write-limit-restored','1')
                network_status();time.sleep(.05)
            if core.poll() is None:core.kill();core.wait();raise AssertionError('bounded Core fixture timed out')
            first_pid=core.pid
            if scenario=='process_restart':
                assert core.returncode==0, log.read_text()
                assert not Path(f'/proc/{first_pid}').exists(), 'first Core is still running'
                first=json.loads(Path('/tmp/statistics-result.json').read_text())
                assert first['pending_preserved'] and first['original_sdk_reaped']
                core=start_core(reopen=True)
                assert core.pid!=first_pid
                deadline=time.monotonic()+18
                while core.poll() is None and time.monotonic()<deadline:
                    network_status();time.sleep(.05)
                if core.poll() is None:core.kill();core.wait();raise AssertionError('restarted Core timed out')
            print(log.read_text(),flush=True)
            print(json.dumps({'core_exit_code':core.returncode,'scenario':scenario}),flush=True)
            if core.returncode:
                print(Path('/tmp/node.log').read_text(),flush=True);raise AssertionError('real Core transport fixture failed')
            records_path=Path('/tmp/statistics-rpc.jsonl')
            records=[json.loads(x) for x in records_path.read_text().splitlines()] if records_path.exists() else []
            assert sum(x['method']=='getnoncebyaddr' for x in records)==(0 if scenario in ['start_failure','bootstrap_failure'] else 1)
            expected_submits=0 if scenario in ['off_before','cancel_before','expired_prepare','save_failure','save_full','start_failure','bootstrap_failure','owner_inactive_before','owner_restore_before'] else 1
            assert sum(x['method']=='sendrawtransaction' for x in records)==expected_submits
            assert sum(x['method']=='gettransaction' for x in records)==(1 if scenario in ['shared','lost_ack','process_restart'] else 0)
            summary=json.loads(Path('/tmp/statistics-result.json').read_text())
            if scenario=='process_restart':
                assert summary['resumed_in_pid']==core.pid
                summary.update({'first_core_exited':True,'distinct_core_processes':True})
                del summary['resumed_in_pid']
                with Path('/tmp/statistics-result.json').open('r+') as stream:
                    json.dump(summary,stream);stream.truncate()
            if scenario=='save_failure':assert original_file_limit is not None and file_limit_restored
            if scenario=='save_full':assert disk_filled and file_limit_restored
            if scenario=='bootstrap_failure':
                assert partial_sdk_pids and all(not (Path('/proc')/pid).exists() for pid in partial_sdk_pids), 'actual partially started SDK was not observed and reaped'
            if scenario=='start_failure':assert root_stopped(), 'invalid executable did not fail closed before Root listen'
            # The Root daemon legitimately remains in /manager until finally.
            # Only its /engine subtree belongs to the SDK being released.
            manager=base.UNIT/'manager/cgroup.procs'
            if scenario=='start_failure':assert not manager.exists()
            else:assert manager.read_text().split()==[str(root)], 'unexpected Root manager process'
            engine=base.UNIT/'engine'
            groups=lambda:{str(p.relative_to(engine)):p.read_text().strip() for p in engine.rglob('cgroup.procs')}
            base.eventually(lambda:not any(groups().values()),1)
            assert not any(groups().values()), 'owned SDK child cgroup still has live processes: '+json.dumps(groups())
            print(json.dumps({'scenario':scenario,'actual_core_auth_machine_root_sdk':scenario!='start_failure','root_refused_invalid_executable':scenario=='start_failure','partial_sdk_start_observed':bool(partial_sdk_pids),'kernel_uid_cgroup_nft':scenario!='start_failure','local_tls_rpc_records':records,'summary':summary,'chain_confirmation':False,'public_network':False,'physical_device':False,'real_systemd':False}),flush=True)
        finally:
            if core.poll() is None:core.kill();core.wait()
            if not root_stopped():os.kill(root,signal.SIGTERM)
            os.waitpid(root,0)
            node.terminate();node.wait(timeout=3)
    return 0

if __name__=='__main__':sys.exit(main())
