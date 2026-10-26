# SPDX-License-Identifier: AGPL-3.0-only
"""Host admission/delivery tests. Synthetic ELF; real external-key signatures.

On-device shell execution is a separate chroot test, not emulated by these
inventory assertions. No network, services or modem are accessed here.
"""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('setup_ap_install',Path(__file__).with_name('setup_ap_install.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
spec=importlib.util.spec_from_file_location('setup_ap_bundle_fixture',module.ROOT/'tools/test-setup-ap-package.py')
fixture=importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)


def inventory(manifest):
    passwd='root:x:0:0:root:/root:/bin/sh\n'+''.join(f'rustsimhub-{name}:x:{uid}:{uid}::/nonexistent:/usr/sbin/nologin\n' for name,uid in [('core',986),('edge',985),('media',984),('forwarder',983)])
    group='root:x:0:\n'+''.join(f'rustsimhub-{name}:x:{uid}:\n' for name,uid in [('core',986),('edge',985),('media',984),('forwarder',983)])+'rustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder\n'
    i={'format':1,'local_passwd':passwd,'nss_passwd':passwd,'local_group':group,'nss_group':group,
       'application':{'format':2,'epoch':49,'schema_min':6,'schema_max':6,'source_sha256':manifest['source_sha256'],'core_sha256':manifest['builds']['core']['sha256'],'edge_sha256':manifest['builds']['edge']['sha256'],'manifest_sha256':'1'*64,'verified':True,'confirmed':{'epoch':49,'hash':'1'*64},'floor':49,'candidate':None,'attempted':False},
       'core_exec_start':module.BASE_EXEC,'systemd_version':257,'active_calls':0,'pcm_closed':True,'ordinary_data_connected':False,'backup_verified':True,'recovery_used':False,'owner_count':1,'owner_verified':True,
       'boot_id':'01234567-0123-0123-0123-0123456789ab','modem_path':'/org/freedesktop/ModemManager1/Modem/0',
       'normal_edge':{'listen':'192.0.2.5:8443','additional_listen_ips':['10.55.0.2']},
       'destinations':{p:'absent' for p in [*(row[1] for row in module.package.FILES),*module.DIRECTORIES]},'setup_code_sha256':'absent','backup_sha256':module.sha(b'synthetic coherent backup')}
    i.update({k:'1'*64 for k in module.BOUND.values()})
    i['passwd_sha256']=module.sha(passwd.encode());i['group_sha256']=module.sha(group.encode())
    return i


class Admission(unittest.TestCase):
    def setUp(self):
        self.bundle=fixture.Bundle();self.bundle.setUp();self.addCleanup(self.bundle.doCleanups)
        self.manifest=self.bundle.build();self.i=inventory(self.manifest)

    def rejects(self,i):
        with self.assertRaises((ValueError,KeyError,TypeError,AttributeError)):module.validate(i,self.manifest)

    def test_valid_source_and_existing_owner_accept(self):
        self.assertIs(module.validate(self.i,self.manifest),self.i)
        self.assertNotIn('setup-ap',module.BASE_EXEC)
        self.assertIn('--serve-device-forwarding-managed ',module.BASE_EXEC)

    def test_old_epoch_group_source_schema_or_bytes_never_force_install(self):
        for key,value in [('format',1),('schema_min',5),('schema_max',True),('source_sha256','b'*64),('core_sha256','b'*64),('edge_sha256','b'*64),('verified',False),('epoch',True),('floor',True),('candidate',{}),('attempted',True),('confirmed',{'epoch':48,'hash':'1'*64})]:
            with self.subTest(key=key):
                i=copy.deepcopy(self.i);i['application'][key]=value;self.rejects(i)

    def test_missing_idle_recovery_backup_owner_and_version_proofs_refuse(self):
        for key,value in [('active_calls',1),('active_calls',False),('pcm_closed',False),('ordinary_data_connected',True),('backup_verified',False),('recovery_used',True),('owner_count',0),('owner_count',True),('owner_verified',False),('systemd_version',246),('core_exec_start',module.BASE_EXEC+' extra'),('boot_id','other'),('modem_path','/bin/sh'),('setup_code_sha256','unknown')]:
            with self.subTest(key=key):
                i=copy.deepcopy(self.i);i[key]=value;self.rejects(i)

    def test_uid_alias_primary_and_supplementary_group_refuse(self):
        for field,line in [('passwd','foreign:x:986:20::/:/bin/sh\n'),('passwd','foreign:x:0986:20::/:/bin/sh\n'),('passwd','foreign:x:20:983::/:/bin/sh\n'),('passwd','foreign:x:20:987::/:/bin/sh\n'),('passwd','bad:x:20:NaN::/:/bin/sh\n'),('group','alias:x:985:\n'),('group','wheel:x:20:rustsimhub-core\n')]:
            for prefix in ['local_','nss_']:
                i=copy.deepcopy(self.i);i[prefix+field]+=line;self.rejects(i)
        for value in [('rustsimhub-core:x:986:','rustsimhub-core:x:986:foreign'),('rustsimhub-ipc:x:987:','rustsimhub-ipc:x:987:foreign,')]:
            i=copy.deepcopy(self.i);i['nss_group']=i['nss_group'].replace(*value);self.rejects(i)

    def test_listener_conflicts_wildcards_and_missing_destinations_refuse(self):
        for host in ['127.0.0.1','172.31.85.1','0.0.0.0','::','::ffff:127.0.0.1','::ffff:172.31.85.1']:
            i=copy.deepcopy(self.i);i['normal_edge']['additional_listen_ips']=[host];self.rejects(i)
        for path in self.i['destinations']:
            i=copy.deepcopy(self.i);i['destinations'][path]='present';self.rejects(i)
        i=copy.deepcopy(self.i);i['destinations'].pop(module.CORE+'/setup-ap-journal');self.rejects(i)

    def test_local_account_and_manifest_hash_bindings_refuse_changed_bytes(self):
        for key in ['manifest_sha256','passwd_sha256','group_sha256']:
            i=copy.deepcopy(self.i);i[key]='b'*64;self.rejects(i)
        i=copy.deepcopy(self.i);i['application']['manifest_sha256']=True;self.rejects(i)


class Delivery(unittest.TestCase):
    def setUp(self):
        Admission.setUp(self);self.output=self.bundle.base/'delivery';self.backup=self.bundle.base/'backup'
        self.backup.write_bytes(b'synthetic coherent backup')

    def create(self,public=None):
        return module.create_delivery(self.i,self.bundle.output,public or self.bundle.public,self.bundle.source,self.backup,self.output)

    def test_exact_delivery_hash_mapping_and_shell_syntax(self):
        result=self.create();self.assertFalse(result['device_changed'])
        raw=(self.output/'SHA256SUMS').read_bytes();self.assertNotIn(b'\r',raw)
        self.assertEqual(module.sha(raw),result['delivery_sha256'])
        lines=raw.decode().splitlines();self.assertEqual(len(lines),27)
        for line in lines:self.assertEqual(module.sha((self.output/line[66:]).read_bytes()),line[:64])
        self.assertEqual({line[66:] for line in lines},{p.name for p in self.output.iterdir()}-{'SHA256SUMS'})
        helper=(self.output/'apply.sh').read_text()
        mapping=helper.split("cat <<'FILES'\n",1)[1].split('\nFILES',1)[0]
        actual={parts[0]:(int(parts[1],8),parts[2]) for parts in (line.split() for line in mapping.splitlines())}
        self.assertEqual(actual,{row['payload']:(row['mode'],row['target']) for row in result['plan']['files']})
        subprocess.run(['sh','-n',str(self.output/'apply.sh')],check=True)
        self.assertFalse(result['plan']['starts_services']);self.assertFalse(result['plan']['changes_network'])
        self.assertFalse(result['plan']['restores_business_database'])
        self.assertEqual(module.sha((self.output/'apply.sh').read_bytes()),result['helper_sha256'])
        with self.assertRaises(ValueError):self.create()

    def test_external_key_backup_and_payload_corruption_refuse_before_output(self):
        other=self.bundle.base/'wrong.public';other.write_bytes(bytes(32))
        with self.assertRaises(ValueError):self.create(other)
        self.assertFalse(self.output.exists())
        self.backup.write_bytes(b'corrupt')
        with self.assertRaises(ValueError):self.create()
        self.assertFalse(self.output.exists())
        self.backup.write_bytes(b'synthetic coherent backup')
        path=self.bundle.output/'rustsimhub-setup-ap';path.write_bytes(path.read_bytes()+b'bad')
        with self.assertRaises(ValueError):self.create()
        self.assertFalse(self.output.exists())

    def test_linked_backup_and_inventory_digest_refuse(self):
        link=self.bundle.base/'linked';os.link(self.backup,link)
        with self.assertRaises(AssertionError):self.create()
        self.assertFalse(self.output.exists())


if __name__=='__main__':unittest.main()
