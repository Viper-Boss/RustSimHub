# SPDX-License-Identifier: AGPL-3.0-only
"""Offline first-install admission and exact bundle tests; no device mutations."""
import hashlib
import importlib.util
import json
from pathlib import Path
import os
import re
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('forwarding_install', Path(__file__).with_name('forwarding_install.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def inventory():
    passwd = 'root:x:0:0:root:/root:/bin/sh\n' + ''.join(f'rustsimhub-{name}:x:{uid}:{uid}::{"/nonexistent"}:/usr/sbin/nologin\n' for name, uid in [('core', 986), ('edge', 985), ('media', 984)])
    group = 'root:x:0:\nrustsimhub-core:x:986:\nrustsimhub-edge:x:985:\nrustsimhub-media:x:984:\nrustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media\n'
    return {'format': 1, 'local_passwd': passwd, 'local_group': group, 'nss_passwd': passwd, 'nss_group': group,
        'application': {'source_sha256': module.RUNTIME_SOURCE, 'manifest_sha256': module.RUNTIME_MANIFEST,
            'manifest_format': 2, 'epoch': 47, 'schema_min': 6, 'schema_max': 6,
            'confirmed': {'epoch': 47, 'hash': module.RUNTIME_MANIFEST}, 'floor': 47, 'candidate': None, 'attempted': False, 'verified': True},
        'core_exec_start': module.BASE_EXEC, 'systemd_version': 257, 'forwarder_uid_pids': [],
        'active_calls': 0, 'pcm_closed': True, 'ordinary_data_connected': False, 'recovery_used': False,
        'backup_verified': True, 'boot_id': '01234567-0123-0123-0123-0123456789ab',
        'ledger_sha256': '1'*64, 'local_passwd_sha256': module.sha(passwd.encode()),
        'local_group_sha256': module.sha(group.encode()), 'core_unit_sha256': '2'*64,
        'backup_sha256': module.sha(b'synthetic coherent backup'),
        'destinations': {x: 'absent' for x in [*[row[1] for row in module.FILES], module.PERSISTENT, module.RUNTIME]}}


class AdmissionTests(unittest.TestCase):
    def test_baseline_accepts(self):
        module.validate(inventory())

    def test_uid_gid_name_alias_and_unrelated_primary_group_refuse(self):
        for field, record in [('passwd', 'foreign:x:983:99::/:/bin/sh\n'), ('passwd', 'foreign:x:20:983::/:/bin/sh\n'),
                ('passwd', 'foreign:x:986:99::/:/bin/sh\n'), ('passwd', 'foreign:x:20:986::/:/bin/sh\n'),
                ('passwd', 'rustsimhub-forwarder:x:20:20::/:/bin/sh\n'), ('group', 'foreign:x:983:\n'),
                ('group', 'foreign:x:987:\n'), ('group', 'rustsimhub-forwarder:x:20:\n'),
                ('group', 'other:x:20:rustsimhub-forwarder\n')]:
            for prefix in ['local_', 'nss_']:
                with self.subTest(field=field, prefix=prefix, record=record):
                    data=inventory();data[prefix+field] += record
                    with self.assertRaises(ValueError):module.validate(data)

    def test_malformed_duplicate_and_missing_identity_refuse(self):
        for mutate in [lambda d:d.update(nss_passwd=d['nss_passwd']+'bad'),
                lambda d:d.update(nss_group=d['nss_group']+'root:x:2:\n'),
                lambda d:d.update(nss_passwd=d['nss_passwd'].replace(':986:986:', ':986:20:')),
                lambda d:d.update(nss_group=d['nss_group'].replace('rustsimhub-ipc:x:987:', 'missing:x:987:'))]:
            data=inventory();mutate(data)
            with self.assertRaises(ValueError):module.validate(data)

    def test_legacy_pending_different_and_unverified_application_refuse(self):
        for key, value in [('manifest_format', 1), ('epoch', 46), ('schema_min', 5), ('source_sha256', '0'*64),
                ('manifest_sha256', '0'*64), ('candidate', {'hash':'0'*64,'epoch':48}), ('attempted', True),
                ('verified', False), ('floor', 48), ('confirmed', {'epoch':46,'hash':'0'*64})]:
            with self.subTest(key=key):
                data=inventory();data['application'][key]=value
                with self.assertRaises(ValueError):module.validate(data)

    def test_activity_missing_backup_old_systemd_and_changed_command_refuse(self):
        for key, value in [('active_calls', 1), ('pcm_closed', False), ('ordinary_data_connected', True),
                ('recovery_used', True), ('backup_verified', False), ('systemd_version', 246),
                ('core_exec_start', module.BASE_EXEC+' arbitrary'), ('forwarder_uid_pids', [123]),
                ('boot_id', 'not-a-boot'), ('ledger_sha256', '../outside')]:
            data=inventory();data[key]=value
            with self.assertRaises(ValueError):module.validate(data)

    def test_each_existing_path_refuses_even_identical_install(self):
        for path in inventory()['destinations']:
            data=inventory();data['destinations'][path]='present'
            with self.assertRaises(ValueError):module.validate(data)

    def test_unbound_account_bytes_and_missing_path_refuse(self):
        data=inventory();data['local_passwd'] += 'user:x:20:20::/:/bin/sh\n'
        with self.assertRaises(ValueError):module.validate(data)
        data=inventory();data['destinations'].pop(module.RUNTIME)
        with self.assertRaises(ValueError):module.validate(data)


class BundleTests(unittest.TestCase):
    def test_checksum_file_is_posix_bytes_on_actual_host(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);backup=root/'backup';backup.write_bytes(b'synthetic coherent backup')
            output=root/'bundle';module.create_bundle(inventory(),output,backup)
            raw=(output/'SHA256SUMS').read_bytes()
            self.assertNotIn(b'\r',raw)
            self.assertEqual(raw.count(b'\n'),18)
            for line in raw.split(b'\n')[:-1]:
                name=line[66:].decode('ascii');self.assertTrue((output/name).is_file())
                self.assertEqual(module.sha((output/name).read_bytes()).encode('ascii'),line[:64])

    def test_actual_bundle_checksums_paths_and_shell_syntax(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);backup=root/'backup';backup.write_bytes(b'synthetic coherent backup')
            output=root/'bundle';plan=module.create_bundle(inventory(),output,backup)
            subprocess.run(['sh','-n',str(output/'apply.sh')],check=True)
            sums=(output/'SHA256SUMS').read_text().splitlines();self.assertEqual(len(sums),18)
            self.assertEqual({line[66:] for line in sums},{p.name for p in output.iterdir()}-{'SHA256SUMS'})
            for line in sums:self.assertEqual(hashlib.sha256((output/line[66:]).read_bytes()).hexdigest(),line[:64])
            self.assertFalse(plan['starts_services']);self.assertFalse(plan['default_enabled']);self.assertFalse(plan['restores_business_database'])
            self.assertEqual(plan['files'][-1]['target'],'/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf')
            self.assertEqual((output/'payload-8').read_bytes(),module.MANAGED_DROPIN)
            self.assertNotIn(b'enabled.json', (output/'payload-8').read_bytes())
            mapping=(output/'apply.sh').read_text().split("cat <<'FILES'\n",1)[1].split('\nFILES',1)[0].splitlines()
            self.assertEqual([line.split() for line in mapping],[[row['name'],format(row['mode'],'o'),row['target']] for row in plan['files']])
            self.assertEqual(json.loads((output/'plan.json').read_text()),plan)
            with self.assertRaises(ValueError):module.create_bundle(inventory(),output,backup)

    def test_backup_corruption_or_symlink_refuses_before_output_creation(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);backup=root/'backup';backup.write_bytes(b'changed')
            with self.assertRaises(ValueError):module.create_bundle(inventory(),root/'bundle',backup)
            self.assertFalse((root/'bundle').exists())
            link=root/'link';link.symlink_to(backup)
            with self.assertRaises(ValueError):module.create_bundle(inventory(),root/'bundle',link)


@unittest.skipUnless(hasattr(os, 'geteuid') and os.geteuid() == 0, 'Root metadata fixture; no real accounts/services')
class ActualScriptTests(unittest.TestCase):
    """Run the actual helper with only paths and command transport substituted.

    Files/modes/checksums/publish/rollback are real POSIX operations. NSS,
    account creation and systemctl are confined synthetic command transports,
    so these tests are not evidence of actual system account/service creation.
    """
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='rsh-fwd-install-',dir='/run')
        self.root=Path(self.temp.name);self.fs=self.root/'device';self.fs.mkdir(mode=0o700)
        self.bin=self.root/'bin';self.bin.mkdir(mode=0o700)
        data=inventory()
        for directory in ['etc/systemd/system','etc/polkit-1/rules.d','etc/rustsimhub','etc/tmpfiles.d',
                'run/lock','run/rustsimhub-app/selected','proc/sys/kernel/random','var/lib/rustsimhub/rustsimhub-v1/system/app']:
            (self.fs/directory).mkdir(parents=True,exist_ok=True,mode=0o755)
        for path,content in [('etc/passwd',data['local_passwd']),('etc/group',data['local_group']),
                ('etc/systemd/system/rustsimhub-core.service','synthetic original core'),
                ('proc/sys/kernel/random/boot_id',data['boot_id']),
                ('var/lib/rustsimhub/rustsimhub-v1/system/app/state.json','synthetic confirmed ledger')]:
            (self.fs/path).write_text(content)
        data['core_unit_sha256']=module.sha(b'synthetic original core');data['ledger_sha256']=module.sha(b'synthetic confirmed ledger')
        self.backup=self.root/'backup';self.backup.write_bytes(b'synthetic coherent backup')
        self.bundle=self.root/'bundle';module.create_bundle(data,self.bundle,self.backup)
        # bind the synthetic manifest content without changing installer logic
        (self.fs/'run/rustsimhub-app/selected/manifest.json').write_bytes(b'synthetic selected manifest')
        (self.bundle/'confirmed.sha256').write_text(module.sha(b'synthetic selected manifest')+'\n')
        script=(self.bundle/'apply.sh').read_text()
        # One pass: the fixture itself lives below /run; recursive replacement
        # would rewrite a prefix inserted by an earlier substitution.
        script=re.sub(r'/(?:etc|run|var/lib|proc/sys)/',lambda match:str(self.fs)+match[0],script)
        script=script.replace('PATH=/usr/sbin:/usr/bin:/sbin:/bin','PATH='+str(self.bin)+':/usr/sbin:/usr/bin:/sbin:/bin')
        (self.bundle/'apply.sh').write_text(script)
        transport='''#!/usr/bin/python3
import json,pathlib,sys
r=pathlib.Path(__file__).resolve().parent.parent/'device';name=pathlib.Path(sys.argv[0]).name;a=sys.argv[1:]
def users():return [s.split(':') for s in (r/'etc/passwd').read_text().splitlines()]
def groups():return [s.split(':') for s in (r/'etc/group').read_text().splitlines()]
if name=='getent':
 rows=users() if a[0]=='passwd' else groups();rows=[x for x in rows if x[0]==a[1] or x[2]==a[1]]
 if rows:print('\\n'.join(':'.join(x) for x in rows))
 sys.exit(0 if rows else 2)
if name=='id':
 if len(a)==1:print(0);sys.exit()
 row=next(x for x in users() if x[0]==a[1]);print(row[2] if a[0]=='-u' else row[3] if a[0]=='-g' else row[3]+' 987');sys.exit()
if name=='systemctl':
 print('0' if 'MainPID' in a else 'inactive');sys.exit()
if name=='ps':sys.exit()
if name=='passwd':print('rustsimhub-forwarder L synthetic');sys.exit()
if name=='groupadd':
 with (r/'etc/group').open('a') as f:f.write('rustsimhub-forwarder:x:983:\\n')
 sys.exit()
if name=='useradd':
 with (r/'etc/passwd').open('a') as f:f.write('rustsimhub-forwarder:!:983:983::/nonexistent:/usr/sbin/nologin\\n')
 sys.exit()
sys.exit(99)
'''
        for name in ['getent','id','systemctl','ps','passwd','groupadd','useradd']:
            file=self.bin/name;file.write_text(transport);file.chmod(0o700)
        self.resign()

    def resign(self):
        sums=self.bundle/'SHA256SUMS'
        sums.write_text(''.join(module.sha(file.read_bytes())+'  '+file.name+'\n' for file in sorted(self.bundle.iterdir()) if file.name not in ['SHA256SUMS','operation.log']))
        for file in self.bundle.iterdir():file.chmod(0o600)

    def tearDown(self):self.temp.cleanup()

    def call(self,action):
        return subprocess.run(['sh',str(self.bundle/'apply.sh'),str(self.bundle),action],capture_output=True,text=True,timeout=20)

    def test_actual_check_has_no_writes_then_apply_and_bounded_file_rollback(self):
        before={p.relative_to(self.fs).as_posix():p.read_bytes() for p in self.fs.rglob('*') if p.is_file()}
        result=self.call('--check');self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(before,{p.relative_to(self.fs).as_posix():p.read_bytes() for p in self.fs.rglob('*') if p.is_file()})
        result=self.call('--apply');self.assertEqual(result.returncode,0,result.stderr)
        for index,(_,target,mode) in enumerate(module.FILES):
            actual=self.fs/target.lstrip('/');self.assertEqual(actual.read_bytes(),(self.bundle/('payload-'+str(index))).read_bytes());self.assertEqual(actual.stat().st_mode&0o7777,mode)
        self.assertFalse((self.fs/module.PERSISTENT.lstrip('/')/'enabled.json').exists())
        self.assertFalse((self.fs/module.RUNTIME.lstrip('/')/'settings.json').exists())
        result=self.call('--apply');self.assertNotEqual(result.returncode,0,'first-install must refuse a repeat, not repair accounts')
        result=self.call('--rollback');self.assertEqual(result.returncode,0,result.stderr)
        self.assertTrue(all(not (self.fs/row[1].lstrip('/')).exists() for row in module.FILES))
        self.assertIn('rustsimhub-forwarder:!:',(self.fs/'etc/passwd').read_text())
        self.assertEqual((self.fs/'var/lib/rustsimhub/rustsimhub-v1/system/app/state.json').read_text(),'synthetic confirmed ledger')

    def test_changed_target_refuses_before_accounts_or_other_files(self):
        path=self.fs/module.FILES[3][1].lstrip('/');path.write_bytes(b'unrelated existing unit')
        original=(self.fs/'etc/passwd').read_bytes()
        result=self.call('--apply');self.assertNotEqual(result.returncode,0)
        self.assertEqual((self.fs/'etc/passwd').read_bytes(),original)
        self.assertFalse((self.fs/module.FILES[0][1].lstrip('/')).exists())
        self.assertEqual(path.read_bytes(),b'unrelated existing unit')

    def test_modified_published_file_blocks_all_rollback(self):
        result=self.call('--apply');self.assertEqual(result.returncode,0,result.stderr)
        path=self.fs/module.FILES[3][1].lstrip('/');path.write_bytes(b'changed after install')
        result=self.call('--rollback');self.assertNotEqual(result.returncode,0)
        self.assertTrue((self.fs/module.FILES[0][1].lstrip('/')).exists())

    def test_symlink_or_digest_corruption_refuses_before_accounts(self):
        for kind in ['symlink','corrupt']:
            file=self.bundle/'payload-0';original=file.read_bytes()
            if kind=='symlink':
                other=self.root/'other';other.write_bytes(original);file.unlink();file.symlink_to(other)
            else:file.write_bytes(b'corrupt')
            result=self.call('--apply');self.assertNotEqual(result.returncode,0)
            self.assertNotIn('rustsimhub-forwarder:',(self.fs/'etc/passwd').read_text())
            file.unlink();file.write_bytes(original);file.chmod(0o600)


if __name__ == '__main__':unittest.main()
