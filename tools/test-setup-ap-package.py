#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit synthetic ELF/build reports; real Ed25519 and external-key verification.

Production bundle creation executes actual component --build-info separately.
Nothing here installs trust, units, payloads or network settings.
"""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
import setup_ap_package as package
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


def elf():
    value=bytearray(128);value[:7]=b'\x7fELF\x02\x01\x01';value[16:18]=(3).to_bytes(2,'little');value[18:20]=(183).to_bytes(2,'little');value[52:54]=(64).to_bytes(2,'little');return bytes(value)


class Bundle(unittest.TestCase):
    def setUp(self):
        private=package.ROOT/'.private';private.mkdir(mode=0o700,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='ap-bundle-fixture-',dir=private);self.addCleanup(self.temp.cleanup);self.base=Path(self.temp.name);self.output=self.base/'bundle'
        self.source='a'*64;self.key=Ed25519PrivateKey.generate();self.seed=self.key.private_bytes(serialization.Encoding.Raw,serialization.PrivateFormat.Raw,serialization.NoEncryption())
        self.public=self.base/'trusted.public';self.public.write_bytes(self.key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw))
        self.blobs={name:elf() if name in package.BINARIES else (package.MATERIAL/name).read_bytes().replace(b'\r\n',b'\n') for name,_,_ in package.FILES}
        self.builds={name:{'sha256':package.sha(elf()),'info':{'component':name,'target_arch':'aarch64','source_archive_sha256':self.source,'development_fixtures':False,'setup_ap_installation_contract':copy.deepcopy(package.CONTRACT)}} for name in ['core','edge','setup-ap','core-launcher']}
        self.builds['core']['info']['database_schema']=6
        self.builds['core']['info']['service_launch']={'format':1,'modes':['device','forwarding_managed','setup_ap']}
        self.builds['core-launcher']['info']['launcher_protocol']=1

    def build(self):
        return package._assemble(self.blobs,self.builds,self.seed,self.output)

    def verify(self):
        return package.verify_bundle(self.output,self.public,self.source)

    def resign(self,raw):
        (self.output/'manifest.json').write_bytes(raw);(self.output/'manifest.sig').write_bytes(self.key.sign(package.DOMAIN+raw))

    def test_exact_separate_signature_domain_and_fixed_root_payloads(self):
        expected=self.build();self.assertEqual(self.verify(),expected)
        self.assertEqual(expected['format'],2)
        self.assertEqual(len(expected['files']),8);self.assertFalse(expected['starts_services'])
        self.assertNotIn('rustsimhub-core',{v['name'] for v in expected['files']})
        self.assertEqual(expected['builds']['core']['sha256'],package.sha(elf()))

    def test_cross_source_debug_and_incomplete_contract_refuse_before_output(self):
        for component,key,value in [('core','source_archive_sha256','b'*64),('edge','development_fixtures',True),('setup-ap','target_arch','x86_64'),('edge','setup_ap_installation_contract',{}),('core-launcher','source_archive_sha256','b'*64),('core-launcher','launcher_protocol',True),('core-launcher','launcher_protocol',2),('core','service_launch',{})]:
            saved=copy.deepcopy(self.builds);self.builds[component]['info'][key]=value
            with self.assertRaises(ValueError):self.build()
            self.assertFalse(self.output.exists());self.builds=saved
        self.builds['core']['info']['setup_ap_installation_contract']['format']=True
        with self.assertRaises(ValueError):self.build()

    def test_tampered_privileged_payload_and_unreviewed_service_refuse(self):
        original=self.blobs['rustsimhub-setup-ap'];self.blobs['rustsimhub-setup-ap']=original+b'changed'
        with self.assertRaises(ValueError):self.build()
        self.blobs['rustsimhub-setup-ap']=original;self.blobs['rustsimhub-setup-ap.service']+=b'\nExecStart=/bin/sh\n'
        with self.assertRaises(ValueError):self.build()

    def test_wrong_external_key_and_application_signature_domain_refuse(self):
        self.build();other=Ed25519PrivateKey.generate();self.public.write_bytes(other.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw))
        with self.assertRaises(ValueError):self.verify()
        self.public.write_bytes(self.key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw))
        raw=(self.output/'manifest.json').read_bytes();(self.output/'manifest.sig').write_bytes(self.key.sign(b'RustSimHub.AppManifest.v1\0'+raw))
        with self.assertRaises(ValueError):self.verify()

    def test_even_signed_wrong_paths_modes_types_and_versions_refuse(self):
        m=self.build()
        for mutate in [lambda v:v.update(format=True),lambda v:v.update(format=1),lambda v:v.update(source_sha256='b'*64),lambda v:v.update(schema=5),lambda v:v.update(starts_services=True),lambda v:v['files'][0].update(target='/etc/passwd'),lambda v:v['files'][0].update(mode=0o4755),lambda v:v['contract'].update(owner_adoption=True),lambda v:v['builds']['edge']['info'].update(development_fixtures=True),lambda v:v['builds']['core-launcher']['info'].update(launcher_protocol=2)]:
            wrong=copy.deepcopy(m);mutate(wrong);self.resign(json.dumps(wrong).encode())
            with self.assertRaises(ValueError):self.verify()

    def test_duplicated_manifest_fields_and_extra_files_refuse(self):
        self.build();raw=(self.output/'manifest.json').read_bytes();bad=raw.replace(b'"format":2',b'"format":2,"format":2',1);self.resign(bad)
        with self.assertRaises(AssertionError):self.verify()
        self.resign(raw);(self.output/'release.public').write_bytes(self.public.read_bytes())
        with self.assertRaises(ValueError):self.verify()

    def test_missing_oversize_corrupt_and_wrong_elf_refuse(self):
        self.blobs['rustsimhub-setup-ap']=b'not ELF';self.builds['setup-ap']['sha256']=package.sha(b'not ELF')
        with self.assertRaises(ValueError):self.build()
        self.blobs['rustsimhub-setup-ap']=elf();self.builds['setup-ap']['sha256']=package.sha(elf());self.build()
        path=self.output/'rustsimhub-setup-ap';path.write_bytes(path.read_bytes()+b'bad')
        with self.assertRaises(ValueError):self.verify()

    def test_payload_symlink_and_hardlink_refuse(self):
        self.build();path=self.output/'rustsimhub-setup-ap';original=path.read_bytes();path.unlink();outside=self.base/'outside';outside.write_bytes(original)
        path.symlink_to(outside)
        with self.assertRaises(OSError):self.verify()
        path.unlink();os.link(outside,path)
        with self.assertRaises(AssertionError):self.verify()

    def test_existing_output_and_foreign_source_refuse(self):
        self.build()
        with self.assertRaises(ValueError):self.build()
        with self.assertRaises(ValueError):package.verify_bundle(self.output,self.public,'b'*64)

    def test_launcher_bytes_and_missing_payload_cannot_be_substituted(self):
        original=self.blobs['rustsimhub-core-launcher']
        self.blobs['rustsimhub-core-launcher']=original+b'changed'
        with self.assertRaises(ValueError):self.build()
        self.blobs['rustsimhub-core-launcher']=original
        self.build()
        path=self.output/'rustsimhub-core-launcher'
        path.write_bytes(path.read_bytes()+b'changed')
        with self.assertRaises(ValueError):self.verify()
        path.unlink()
        with self.assertRaises(ValueError):self.verify()


if __name__=='__main__':unittest.main()
