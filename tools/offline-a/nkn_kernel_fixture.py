#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""A2 actual Root/SDK/Noise/Core fixture, private namespaces only; no BLE/media."""
from pathlib import Path
import ctypes,importlib.util,json,os,select,signal,subprocess,sys,time
spec=importlib.util.spec_from_file_location('original_root_fixture',Path(__file__).resolve().parents[1]/'nkn-build/sdk-managed/root_kernel_fixture.py')
base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)

def main():
    _,node=base.install_fixture()
    for path,owner,group,mode in [('/run/rustsimhub/nkn',1001,1002,0o2750),('/tmp/sdk-counterpart',1001,1007,0o2770),('/tmp/nkn-secure-peer',1007,1007,0o700)]:
        Path(path).mkdir(parents=True,exist_ok=True);os.chown(path,owner,group);os.chmod(path,mode)
    def network_status():
        value={'schema':1,'available':True,'boot_id':Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'sample_unix_seconds':int(time.time()),'devices':[{'interface':'wlan0','kind':'wifi','state':'100 (connected)','ipv4':['9.9.9.8/24'],'ipv6':[],'dns_ipv4':['9.9.9.9'],'dns_ipv6':[]}]}
        target=Path('/run/rustsimhub-network/status.json');temporary=target.with_suffix('.pending');temporary.write_text(json.dumps(value));temporary.replace(target)
    network_status()
    with Path('/tmp/a2-core.log').open('wb') as log:
        core=subprocess.Popen(['/core-test','--ignored','--exact','nkn_product::a2_tests::kernel_remote','--nocapture'],user=1001,group=1001,extra_groups=[],env=dict(os.environ,RSH_A2_KERNEL='1'),stdout=log,stderr=subprocess.STDOUT)
        base.eventually(lambda:Path('/tmp/a2-public.json').exists(),8)
        public=json.loads(Path('/tmp/a2-public.json').read_text());assert set(public)=={'pin'}
        Path('/tmp/sdk-counterpart/public.json').write_text(json.dumps(public))
        policy_path=Path('/etc/rustsimhub/nkn-policy.json');policy=json.loads(policy_path.read_text());pin=bytes(public['pin']);assert len(pin)==32
        policy['core_pin']=pin.hex();policy_path.write_text(json.dumps(policy,separators=(',',':')))
        member=Path('/etc/rustsimhub/nkn-membership.bin');member.write_bytes(b'RSMK\x01'+bytes([5])*16+bytes([7])*32+(1).to_bytes(8,'big')+pin);os.chown(member,0,1001);member.chmod(0o640)
        root=os.fork()
        if root==0:
            base.write(base.UNIT/'cgroup.procs',os.getpid())
            assert ctypes.CDLL(None,use_errno=True).prctl(38,1,0,0,0)==0
            os.execv('/root-provider',['/root-provider','--serve-fixed'])
        peers=[]
        try:
            base.eventually(lambda:Path(base.ROOT_SOCKET).exists(),5);base.write('/tmp/a2-root-ready','1')
            deadline=time.monotonic()+15;broken=False;restored=False
            while core.poll() is None and not Path('/tmp/a2-peer-ready').exists():
                assert time.monotonic()<deadline,'A2 lifecycle startup deadline'
                if Path('/tmp/a2-break-membership').exists() and not broken:member.chmod(0o600);broken=True
                if Path('/tmp/a2-restore-membership').exists() and not restored:member.chmod(0o640);base.write('/tmp/a2-membership-restored','1');restored=True
                network_status();time.sleep(.02)
            if core.poll() is not None:
                print(Path('/tmp/a2-core.log').read_text(),flush=True);raise AssertionError('real Core lifecycle failed before peer admission')
            assert broken and restored
            cases=['query','old_token','query','wrong_pin','query','wrong_membership','query','tamper','query','old_cipher','query','revoked']
            for index,case in enumerate(cases):
                endpoint=Path('/tmp/sdk-counterpart/core.sock')
                if endpoint.exists():endpoint.unlink()
                with Path(f'/tmp/a2-sdk-peer-{index}.log').open('wb') as sdk_log:
                    sdk=subprocess.Popen(['/sdk-client'],user=1007,group=1007,extra_groups=[],stdout=sdk_log,stderr=subprocess.STDOUT)
                    try:
                        base.eventually(endpoint.exists,5)
                        client=subprocess.Popen(['/core-test','--ignored','--exact','nkn_uid_tests::isolated_nkn_client','--nocapture'],user=1007,group=1007,extra_groups=[],env=dict(os.environ,RSH_COMBINED_SDK_CLIENT='1',RSH_A2_ONLY='1',RUSTSIMHUB_NKN_CASE=case,RUSTSIMHUB_UID_FIXTURE_DIR='/tmp/sdk-counterpart'),stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
                        prefix=b''
                        try:
                            if case=='revoked':
                                until=time.monotonic()+8
                                while b'NKN_BOUND_REVOKE' not in prefix:
                                    assert time.monotonic()<until and select.select([client.stdout],[],[],1)[0],'real remote token was not issued'
                                    line=client.stdout.readline();assert line;prefix+=line
                                base.write('/tmp/a2-revoke-request','1');base.eventually(lambda:Path('/tmp/a2-client-revoked').exists(),3)
                            output,_=client.communicate(input=b'R' if case=='revoked' else None,timeout=12)
                            Path(f'/tmp/a2-peer-{index}.log').write_bytes(prefix+output)
                            assert client.returncode==0,case+' original Noise/Auth client assertions failed'
                        finally:
                            if client.poll() is None:client.kill();client.wait()
                        sdk_code=sdk.wait(timeout=4)
                        assert case!='query' or sdk_code==0,'positive SDK peer failed'
                        peers.append({'case':case,'core_client_exit':client.returncode,'sdk_counterpart_exit':sdk_code,'negative_sdk_timeout_expected':case!='query'})
                    finally:
                        if sdk.poll() is None:sdk.kill();sdk.wait()
                base.write('/tmp/a2-drain-request',str(index))
                base.eventually(lambda:Path('/tmp/a2-drain-ack').exists() and Path('/tmp/a2-drain-ack').read_text()==str(index),4)
                network_status()
            base.write('/tmp/a2-peers-done','1');core.wait(timeout=12)
            print(Path('/tmp/a2-core.log').read_text(),flush=True)
            assert core.returncode==0,'A2 original Core did not complete cleanup'
            assert base.empty(),'original SDK cgroup/nft policy not released'
            assert not Path('/run/rustsimhub/nkn/core.sock').exists(),'original listener was not removed'
            result={'passed':True,'actual_root_sdk_noise_auth':True,'real_membership_file_permissions':True,'peers':peers,'core':json.loads(Path('/tmp/a2-core-result.json').read_text()),'public_network':False,'physical_device':False,'ble_or_media':False,'real_systemd':False}
            Path('/tmp/a2-result.json').write_text(json.dumps(result));print(json.dumps(result),flush=True)
        finally:
            if core.poll() is None:core.kill();core.wait()
            os.kill(root,signal.SIGTERM);os.waitpid(root,0);node.terminate();node.wait(timeout=3)
    return 0
if __name__=='__main__':sys.exit(main())
