#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Execute the actual fixed installer inside disposable chroots.

Real filesystem ownership, process UIDs and Core/Edge/AP preparation CLIs.
Explicit fixtures replace PID1, modem, network status and process enumeration;
this is NOT a live systemd/radio/ARM/signature integration test. The production
delivery verifier has no fixture switch. Native payload admission below exists
only in this test's synthetic delivery, whose digest is pinned by the caller.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('setup_ap_install_fixture',ROOT/'tools/test-setup-ap-install.py')
host=importlib.util.module_from_spec(spec);spec.loader.exec_module(host)
module=host.module


def command(args,**kwargs):
    return subprocess.run(args,capture_output=True,timeout=35,**kwargs)


def put(root,name,data,mode=0o600,uid=0,gid=0):
    path=root/name.lstrip('/');path.parent.mkdir(parents=True,exist_ok=True)
    path.write_bytes(data if isinstance(data,bytes) else data.encode());path.chmod(mode);os.chown(path,uid,gid)
    return path


def copy_program(root,path,destination=None):
    path=Path(path).resolve(strict=True)
    target=root/(destination or str(path)).lstrip('/');target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(path,target);target.chmod(0o755)
    result=command(['ldd',str(path)],text=True)
    if result.returncode:
        if 'not a dynamic executable' in result.stderr:return
        raise AssertionError('ldd failed')
    for library in re.findall(r'(?:=>\s*)?(/[^\s]+)',result.stdout):
        target=root/library.lstrip('/');target.parent.mkdir(parents=True,exist_ok=True)
        if not target.exists():shutil.copyfile(library,target);target.chmod(0o755)


class Installer(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        assert os.geteuid()==0,'Root required for disposable chroot/UID fixtures'
        cls.temporary=tempfile.TemporaryDirectory(prefix='rsh-ap-install-')
        cls.base=Path(cls.temporary.name);cls.template=cls.base/'template';cls.template.mkdir(mode=0o755)
        root=cls.template
        programs=['sh','id','stat','grep','wc','cut','sort','sha256sum','cat','awk','tr','find','dirname','flock','ps','head','tail','install','sync','mv','rm','timeout','getent','setpriv']
        for name in programs:
            path=shutil.which(name);assert path,name
            copy_program(root,path,'/usr/bin/'+name)
        (root/'bin').symlink_to('usr/bin');(root/'sbin').symlink_to('usr/bin')
        inv=host.inventory({'source_sha256':'a'*64,'builds':{'core':{'sha256':'a'*64},'edge':{'sha256':'a'*64}}})
        put(root,'/etc/passwd',inv['local_passwd'],0o644);put(root,'/etc/group',inv['local_group'],0o644)
        put(root,'/etc/nsswitch.conf','passwd: files\ngroup: files\n',0o644)
        put(root,'/dev/null',b'',0o666)
        put(root,'/proc/sys/kernel/random/boot_id',inv['boot_id']+'\n',0o444)
        put(root,'/fixture/calls','ao 0\n');put(root,'/fixture/call-state','i 7\n');put(root,'/fixture/network','30 (disconnected)\n')
        put(root,'/usr/bin/busctl','#!/bin/sh\ncase "$*" in *" Calls") cat /fixture/calls;; *" State") cat /fixture/call-state;; *) exit 1;; esac\n',0o755)
        put(root,'/usr/bin/nmcli','#!/bin/sh\ncat /fixture/network\n',0o755)
        put(root,'/usr/bin/ps','#!/bin/sh\nprintf "0\\n"\n',0o755)
        put(root,'/usr/bin/systemctl','#!/bin/sh\ncase "$*" in *MainPID*|*ControlPID*) echo 0;; *Job*) if test -e /fixture/job;then echo "12 queued-job";else echo 0;fi;; *ActiveState*) if test -e /fixture/busy;then echo active;else echo inactive;fi;; *) exit 1;; esac\n',0o755)
        put(root,'/usr/bin/runuser','#!/bin/sh\ncase "$2" in rustsimhub-core) uid=986;; rustsimhub-edge) test ! -e /fixture/fail-edge || exit 73;uid=985;; *) exit 1;; esac\nshift 3\nexec setpriv --reuid="$uid" --regid="$uid" --clear-groups "$@"\n',0o755)
        selected='/run/rustsimhub-app/selected'
        for path,name in [(CORE,'rustsimhub-core'),(EDGE,'rustsimhub-edge')]:copy_program(root,path,selected+'/'+name)
        # Libraries for the subsequently installed native AP executable.
        copy_program(root,AP,'/fixture/ap-native')
        put(root,selected+'/manifest.json','synthetic selected app manifest\n')
        put(root,'/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json','synthetic confirmed ledger\n')
        put(root,'/etc/systemd/system/rustsimhub-core.service','synthetic base unit\n',0o644)
        put(root,'/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf','synthetic existing forwarding mode\n',0o644)
        put(root,'/var/lib/rustsimhub/rustsimhub-v1/edge/installation/config.json','synthetic ordinary edge\n',0o600,985,985)
        core=root/module.CORE.lstrip('/')
        for name in ['state','identity','setup']:
            folder=core/name;folder.mkdir(parents=True,mode=0o700);os.chown(folder,986,986)
        core.chmod(0o700);os.chown(core,986,986)
        prefix=['chroot',str(root),'/usr/bin/setpriv','--reuid=986','--regid=986','--clear-groups',selected+'/rustsimhub-core']
        exported=command([*prefix,'--prepare-device-ap-binding',module.CORE+'/state',module.CORE+'/identity',module.CORE+'/setup'])
        assert exported.returncode==0,exported.stderr.decode(errors='replace')
        info=json.loads(exported.stdout)
        from cryptography.hazmat.primitives import serialization
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
        key=Ed25519PrivateKey.from_private_bytes(bytes([120])*32)
        public=key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw).hex()
        process=subprocess.Popen([*prefix,'--provision-owner',module.CORE+'/state',module.CORE+'/identity',public],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            assert select.select([process.stdout],[],[],5)[0]
            challenge=json.loads(process.stdout.readline(16385))
            output,error=process.communicate(json.dumps(list(key.sign(bytes(challenge['transcript'])))).encode(),timeout=10)
            assert process.returncode==0,error.decode(errors='replace')
            assert json.loads(output)['owner_initialized']
        finally:
            if process.poll() is None:process.kill();process.wait(timeout=5)
        put(root,module.CORE+'/setup/setup.used',b'RustSimHub.DeviceSetup.Window.v1\0'+bytes.fromhex(info['device_public_key_hex']),0o600,986,986)
        put(root,module.CORE+'/state/protected-business-sentinel',b'business data must never be restored or removed',0o600,986,986)
        cls.db=next((core/'state').glob('*.sqlite3')).relative_to(root)
        cls.baseline=cls.business(root)

    @classmethod
    def tearDownClass(cls):cls.temporary.cleanup()

    @classmethod
    def business(cls,root):
        connection=sqlite3.connect('file:'+str(root/cls.db)+'?mode=ro',uri=True)
        try:
            tables=connection.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").fetchall()
            return [(name,connection.execute('SELECT * FROM "'+name.replace('"','""')+'"').fetchall()) for name, in tables]
        finally:connection.close()

    def setUp(self):
        self.root=self.base/('case-'+self._testMethodName);shutil.copytree(self.template,self.root,symlinks=True)
        # copytree intentionally does not preserve ownership; restore fixture IDs.
        for directory,dirs,files in os.walk(self.template,followlinks=False):
            for name in [*dirs,*files]:
                src=Path(directory)/name;dst=self.root/src.relative_to(self.template)
                if not src.is_symlink():s=src.stat();os.chown(dst,s.st_uid,s.st_gid)
        self.bundle=self.root/'bundle';self.bundle.mkdir(mode=0o700)
        material=module.package.MATERIAL
        blobs={f'payload-{n}':Path(AP).read_bytes() if n==0 else (material/name).read_bytes().replace(b'\r\n',b'\n') for n,(name,_,_) in enumerate(module.package.FILES)}
        paths={'application-manifest.sha256':'/run/rustsimhub-app/selected/manifest.json','application-ledger.sha256':'/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json','core-unit.sha256':'/etc/systemd/system/rustsimhub-core.service','forwarding-unit.sha256':'/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf','passwd.sha256':'/etc/passwd','group.sha256':'/etc/group','edge-config.sha256':'/var/lib/rustsimhub/rustsimhub-v1/edge/installation/config.json','identity.sha256':module.CORE+'/identity/identity.seed','setup-used.sha256':module.CORE+'/setup/setup.used','core.sha256':'/run/rustsimhub-app/selected/rustsimhub-core','edge.sha256':'/run/rustsimhub-app/selected/rustsimhub-edge'}
        blobs.update({name:(module.sha((self.root/path.lstrip('/')).read_bytes())+'\n').encode() for name,path in paths.items()})
        blobs.update({'setup-code.sha256':b'absent\n','boot-id':(self.root/'proc/sys/kernel/random/boot_id').read_bytes(),'modem-path':b'/org/freedesktop/ModemManager1/Modem/0\n','business-before.tar':b'explicit synthetic backup, never extracted','signed-manifest.json':b'explicit native fixture, not production-admitted','signed-manifest.sig':b'fixture','signer.public':b'fixture','apply.sh':(material/'install.sh').read_bytes().replace(b'\r\n',b'\n'),'plan.json':b'{}\n'})
        self.assertEqual(len(blobs),27)
        for name,data in blobs.items():put(self.root,'/bundle/'+name,data)
        self.sums()

    def sums(self):
        sums=''.join(module.sha(path.read_bytes())+'  '+path.name+'\n' for path in sorted(self.bundle.iterdir()) if path.name not in ['SHA256SUMS','operation.log']).encode()
        put(self.root,'/bundle/SHA256SUMS',sums);self.digest=module.sha(sums)

    def run_helper(self,action='--check',success=True,digest=None):
        result=command(['chroot',str(self.root),'/bin/sh','/bundle/apply.sh','/bundle',action,digest or self.digest])
        if success:self.assertEqual(result.returncode,0,result.stderr.decode(errors='replace'))
        else:self.assertNotEqual(result.returncode,0);return None
        return json.loads(result.stdout)

    def snapshot(self):
        # No devices/mounts are exposed to the chroot. Its regular-file null
        # stand-in receives discarded diagnostics and is not a system target.
        return {str(p.relative_to(self.root)):(p.lstat().st_uid,p.lstat().st_gid,p.lstat().st_mode,hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None) for p in self.root.rglob('*') if not p.is_symlink() and p!=self.root/'dev/null'}

    def test_check_is_readonly_and_wrong_digest_changes_nothing(self):
        before=self.snapshot();self.assertTrue(self.run_helper()['preflight']);self.assertEqual(self.snapshot(),before)
        self.run_helper(digest='0'*64,success=False);self.assertEqual(self.snapshot(),before)

    def test_real_offline_clis_apply_repeat_and_exact_rollback_preserve_business(self):
        self.assertTrue(self.run_helper('--apply')['prepared'])
        self.assertEqual(self.business(self.root),self.baseline)
        material=self.root/module.PERSISTENT.lstrip('/')/'installation/config.json';private=material.read_bytes()
        binding=(self.root/module.CORE.lstrip('/')/'setup/binding.json').read_bytes()
        self.assertTrue(self.run_helper('--apply')['prepared']);self.assertEqual(material.read_bytes(),private)
        self.assertEqual((self.root/module.CORE.lstrip('/')/'setup/binding.json').read_bytes(),binding)
        self.assertTrue(self.run_helper('--rollback')['files_removed'])
        for _,target,_ in module.package.FILES:self.assertFalse((self.root/target.lstrip('/')).exists())
        self.assertEqual(material.read_bytes(),private);self.assertEqual(self.business(self.root),self.baseline)
        self.assertEqual((self.root/module.CORE.lstrip('/')/'state/protected-business-sentinel').read_bytes(),b'business data must never be restored or removed')

    def test_partial_cli_failure_keeps_activation_unpublished_and_resume_reuses_material(self):
        put(self.root,'/fixture/fail-edge',b'1');self.run_helper('--apply',False)
        for n in [4,5]:self.assertFalse((self.root/module.package.FILES[n][1].lstrip('/')).exists())
        binding=(self.root/module.CORE.lstrip('/')/'setup/binding.json').read_bytes()
        (self.root/'fixture/fail-edge').unlink();self.assertTrue(self.run_helper('--apply')['prepared'])
        self.assertEqual((self.root/module.CORE.lstrip('/')/'setup/binding.json').read_bytes(),binding)
        self.assertEqual(self.business(self.root),self.baseline)

    def test_changed_last_target_prevents_any_rollback_deletion(self):
        self.run_helper('--apply');last=self.root/module.package.FILES[5][1].lstrip('/');last.write_bytes(b'foreign change')
        before=self.snapshot();self.run_helper('--rollback',False)
        self.assertEqual(self.snapshot(),before)
        self.assertTrue((self.root/module.package.FILES[0][1].lstrip('/')).is_file())

    def test_runtime_busy_call_data_and_pending_journal_refuse(self):
        for name,data,action in [('/fixture/busy',b'1','--apply'),('/fixture/network',b'100 (connected)\n','--check'),('/fixture/calls',b'ao 1 "/org/freedesktop/ModemManager1/Call/1"\n','--check')]:
            original=(self.root/name.lstrip('/')).read_bytes() if (self.root/name.lstrip('/')).exists() else None
            put(self.root,name,data)
            if name.endswith('calls'):put(self.root,'/fixture/call-state','i 3\n')
            self.run_helper(action,False);self.assertFalse((self.bundle/'operation.log').exists())
            if original is None:(self.root/name.lstrip('/')).unlink()
            else:put(self.root,name,original)
        self.run_helper('--apply');put(self.root,module.PERSISTENT+'/journal/active.json',b'unknown operation')
        self.run_helper('--rollback',False);self.assertTrue((self.root/module.package.FILES[0][1].lstrip('/')).exists())

    def test_foreign_destination_symlink_and_extra_bundle_file_refuse(self):
        target=self.root/module.package.FILES[0][1].lstrip('/');target.parent.mkdir(parents=True,exist_ok=True);target.symlink_to('/etc/passwd')
        self.run_helper(success=False);target.unlink()
        put(self.root,'/bundle/.foreign',b'unknown');self.run_helper(success=False)
        self.assertFalse((self.bundle/'operation.log').exists())

    def test_interruption_after_begin_can_rollback_without_target_parents(self):
        put(self.root,'/bundle/operation.log','begin-first-setup-ap-install\n')
        self.assertTrue(self.run_helper('--rollback')['files_removed'])
        self.assertEqual(self.business(self.root),self.baseline)
        for _,target,_ in module.package.FILES:self.assertFalse((self.root/target.lstrip('/')).exists())

    def test_changed_staging_link_or_identity_refuses_before_any_file_change(self):
        self.run_helper('--apply')
        target=self.root/module.package.FILES[0][1].lstrip('/');staging=target.with_name(target.name+'.rsh-ap-new')
        os.link(target,staging);before=self.snapshot();self.run_helper('--rollback',False)
        self.assertEqual(self.snapshot(),before);staging.unlink()
        identity=self.root/module.CORE.lstrip('/')/'identity/identity.seed';identity.write_bytes(bytes([9])*32)
        before=self.snapshot();self.run_helper('--rollback',False);self.assertEqual(self.snapshot(),before)

    def test_queued_job_and_malformed_modem_data_are_not_idle_proofs(self):
        put(self.root,'/fixture/job','1');self.run_helper('--apply',False)
        self.assertFalse((self.bundle/'operation.log').exists());(self.root/'fixture/job').unlink()
        for data in ['ao 2 "/org/freedesktop/ModemManager1/Call/1"\n','ao 1 "/bin/sh;touch /fixture/injected"\n','unknown\n']:
            put(self.root,'/fixture/calls',data);self.run_helper(success=False)
            self.assertFalse((self.root/'fixture/injected').exists())


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['core','edge','ap']:parser.add_argument('--'+name,required=True,type=Path)
    args,rest=parser.parse_known_args();CORE,EDGE,AP=[str(getattr(args,name).resolve(strict=True)) for name in ['core','edge','ap']]
    unittest.main(argv=[sys.argv[0],*rest])
