#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Offline product-maintenance packager, never an installer or an enable command.

Consumes the finite PR205 profile output and an independently verified signed
application group. The unsigned build report contributes provenance, never an
installation permission. The output requires a separate product signing seed;
the verifier obtains installed trust from its caller, never this directory.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import application_package as application

DOMAIN = b'RustSimHub.ProductMaintenance.v1\0'
PROFILES = {'legacy-v1':False,'product-v1':False,'product-email-v1':False,
            'product-recovery-v1':True,'product-email-recovery-v1':True,'product-recovery-health-v1':True}
BASE_ROLES = ['core','edge','media','forwarder']
SOURCE_FILES = {'profile_config':('materials','product-profile.json'),
                'core_dropin':('materials','50-product-profile.conf'),
                'launcher':('programs','rustsimhub-core-launcher'),
                'nkn_root':('programs','nkn-root'), 'nkn_sdk':('programs','nkn-sdk-engine'),
                'nkn_unit':('materials','rustsimhub-nkn-root.service'),
                'product_tmpfiles':('materials','product.tmpfiles'),
                'ble_host':('programs','rustsimhub-ble-recovery'),
                'ble_unit':('materials','rustsimhub-ble-recovery.service'),
                'ble_policy':('materials','ble-product.policy'),
                'health_dropin':('materials','55-product-health.conf'),
                'health_tmpfiles':('materials','health.tmpfiles'),
                'health_service':('materials','rustsimhub-update-health.service'),
                'health_timer':('materials','rustsimhub-update-health.timer')}


def need(condition,message):
    if not condition: raise ValueError(message)


def digest(value): return hashlib.sha256(value).hexdigest()


def read(path,limit):
    path=Path(path)
    need(not path.parent.is_symlink(),'linked input directory')
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    with os.fdopen(fd,'rb') as stream:
        m=os.fstat(stream.fileno())
        need(stat.S_ISREG(m.st_mode) and m.st_nlink==1 and 0<m.st_size<=limit,'bounded single-link regular input')
        value=stream.read(limit+1)
        after=os.fstat(stream.fileno())
        need(len(value)==m.st_size and (m.st_dev,m.st_ino,m.st_size,m.st_mtime_ns,m.st_ctime_ns)==
             (after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns),'changing input')
        return value


def parse(raw):
    def unique(pairs):
        result=dict(pairs)
        need(len(result)==len(pairs),'duplicate JSON field')
        return result
    def invalid(_): raise ValueError('nonfinite JSON')
    return json.loads(raw,object_pairs_hook=unique,parse_constant=invalid)


def encode(value): return json.dumps(value,separators=(',',':'),ensure_ascii=True).encode()


def selected(profile):
    need(profile in PROFILES,'unknown fixed profile')
    ids=['profile_config','core_dropin','launcher']
    if profile!='legacy-v1': ids+=['nkn_root','nkn_sdk','nkn_unit','product_tmpfiles']
    if PROFILES[profile]: ids+=['ble_host','ble_unit','ble_policy']
    if profile=='product-recovery-health-v1': ids+=['health_dropin','health_tmpfiles','health_service','health_timer']
    return ids


def elf(raw):
    need(len(raw)>=64 and raw[:7]==b'\x7fELF\x02\x01\x01' and
         int.from_bytes(raw[16:18],'little') in (2,3) and int.from_bytes(raw[18:20],'little')==183 and
         int.from_bytes(raw[52:54],'little')==64,'AArch64 executable required')


def roles_policy(raw):
    policy=parse(raw)
    need(type(policy) is dict and set(policy)=={'format','roles'} and type(policy['format']) is int and policy['format']==1,'role approval format')
    roles=policy['roles']; need(type(roles) is dict and set(roles)=={'nkn','ble'},'finite approved roles')
    ids=[]
    for role in ['nkn','ble']:
        row=roles[role]
        need(type(row) is dict and set(row)=={'uid','gid'} and type(row['uid']) is int and type(row['gid']) is int and
             row['uid']==row['gid'] and 0<row['uid']<2**32-1 and row['uid'] not in (0,29,983,984,985,986,987,65534),'explicit approved role IDs required')
        ids.append(row['uid'])
    need(ids[0]!=ids[1],'aliased approved roles')
    return ids


def create(build_root, app, app_public, policy_path, seed_path, output):
    need(__debug__,'existing application verifier requires assertions enabled')
    build_root=Path(build_root); app=Path(app); output=Path(output)
    need(build_root.is_dir() and not build_root.is_symlink(),'actual build root required')
    report=parse(read(build_root/'profile-build.json',256*1024))
    need(type(report) is dict,'provenance object required')
    profile=report.get('profile'); ids=selected(profile)
    need(report.get('kind')=='unsigned-product-profile' and report.get('format')==1 and type(report.get('format')) is int,
         'PR205 unsigned provenance report required')
    need(report.get('target')=='aarch64-unknown-linux-gnu' and report.get('signed') is False and
         report.get('installable') is False and report.get('epoch_assigned') is False,'not an installation authorization')
    source=report.get('source_archive_sha256'); revision=report.get('revision')
    need(isinstance(source,str) and re.fullmatch('[0-9a-f]{64}',source) and
         isinstance(revision,str) and re.fullmatch('[0-9a-f]{40}',revision),'canonical committed source binding')
    archive=subprocess.run(['/usr/bin/git','-C',str(ROOT),'archive','--format=tar',revision],
                           stdin=subprocess.DEVNULL,capture_output=True,check=True,timeout=10).stdout
    need(len(archive)<=64*1024**2 and digest(archive)==source,'actual committed archive differs from provenance')
    schema=9 if PROFILES[profile] else 6
    # This old verifier remains unchanged: exact domain, external key, finite
    # application payloads and bytes are checked before creating new output.
    checked=application.verify_package(app,app_public,source,schema=schema)
    application_raw=read(app/'manifest.json',8192)
    app_entries={e['name']:e for e in checked['files']}
    programs=report.get('programs'); materials=report.get('materials')
    need(type(programs) is dict and type(materials) is dict,'actual build inventories required')
    for name in ['rustsimhub-core','rustsimhub-edge','rustsimhub-audio-alsa','rustsimhub-forwarder']:
        raw=read(build_root/'programs'/name,32*1024**2)
        entry=programs.get(name,{})
        need(entry.get('sha256')==digest(raw) and type(entry.get('bytes')) is int and entry['bytes']==len(raw) and
             app_entries.get(name,{}).get('sha256')==digest(raw),'application/build bytes differ')
    policy_raw=read(policy_path,8192); nkn,ble=roles_policy(policy_raw)
    bindings={'profile':profile,'source_commit':revision,'source_archive_sha256':source,
              'application_manifest_sha256':digest(application_raw),
              'core_sha256':app_entries['rustsimhub-core']['sha256'],
              'approved_role_policy_sha256':digest(policy_raw)}
    blobs={}; entries=[]; total=0
    for id in ids:
        folder,name=SOURCE_FILES[id]; limit=32*1024**2 if folder=='programs' else 128*1024
        raw=read(build_root/folder/name,limit)
        if folder=='programs':
            elf(raw); entry=programs.get(name,{})
            need(entry.get('sha256')==digest(raw) and type(entry.get('bytes')) is int and entry['bytes']==len(raw),'program byte inventory mismatch')
        else: need(materials.get(name)==digest(raw),'material byte inventory mismatch')
        blobs[id+'.payload']=raw; entries.append({'id':id,'bytes':len(raw),'sha256':digest(raw)}); total+=len(raw)
    need(total<=64*1024**2,'total material budget exceeded')
    config=parse(blobs['profile_config.payload'])
    need(config=={'format':1,'profile':profile,'source_archive_sha256':source,'core_sha256':bindings['core_sha256']} and
         type(config['format']) is int,'installed profile binding differs')
    material_root=ROOT/'firmware/system/product-profiles'
    dropin=(material_root/'50-product-profile.conf').read_text().replace('@CORE_MEMORY@','192M' if profile!='legacy-v1' else '96M').replace('@CORE_TASKS@','64' if profile!='legacy-v1' else '16').encode()
    need(blobs['core_dropin.payload']==dropin,'unreviewed Core drop-in')
    for id,name in [('nkn_unit','rustsimhub-nkn-root.service'),('ble_unit','rustsimhub-ble-recovery.service'),('ble_policy','ble-product.policy')]:
        if id in ids: need(blobs[id+'.payload']==(material_root/name).read_bytes(),'unreviewed fixed service material')
    if profile=='product-recovery-health-v1':
        for id, name in [('health_tmpfiles','tmpfiles.conf'),('health_service','rustsimhub-update-health.service'),('health_timer','rustsimhub-update-health.timer')]:
            need(blobs[id+'.payload']==(ROOT/'firmware/system/update-health'/name).read_bytes(),'unreviewed fixed health material')
        need(blobs['health_dropin.payload']==(material_root/'55-product-health.conf').read_bytes(),'unreviewed fixed health drop-in')
    if profile!='legacy-v1':
        expected=(ROOT/'firmware/system/product-profiles/product.tmpfiles.in').read_text().replace('@NKN_GID@',str(nkn)).replace('@BLE_GID@',str(ble)).encode()
        need(blobs['product_tmpfiles.payload']==expected,'approved role material differs')
    manifest={'format':1,'kind':'product-maintenance','board':'ufi003-v02','arch':'aarch64-linux',
              'bindings':bindings,'roles':BASE_ROLES+(['nkn','ble'] if profile!='legacy-v1' else []),'files':entries}
    raw=encode(manifest); need(len(raw)<=32768,'manifest budget')
    seed=read(seed_path,32); need(len(seed)==32,'external Ed25519 seed length')
    # RFC8410 PKCS8 for a raw Ed25519 seed. Reuse the system OpenSSL already
    # required by the unchanged application verifier, not another dependency.
    with tempfile.TemporaryDirectory(prefix='product-signature-') as temporary:
        private=Path(temporary)
        key=private/'seed.der'; key.write_bytes(bytes.fromhex('302e020100300506032b657004220420')+seed);key.chmod(0o600)
        message=private/'message';message.write_bytes(DOMAIN+raw)
        signature=private/'signature'
        subprocess.run(['openssl','pkeyutl','-sign','-rawin','-keyform','DER','-inkey',str(key),'-in',str(message),'-out',str(signature)],
                       stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True,timeout=5)
        signed=read(signature,64);need(len(signed)==64,'signature length')
    blobs.update({'manifest.json':raw,'manifest.sig':signed})
    need(not output.exists() and not output.is_symlink() and output.parent.is_dir() and not output.parent.is_symlink(),'fresh output required')
    # A complete directory is published at once, on the output filesystem. No
    # partially signed directory is mistaken for a complete finite package.
    with tempfile.TemporaryDirectory(prefix='.product-package-',dir=output.parent) as temporary:
        stage=Path(temporary)/'package'; stage.mkdir(mode=0o700)
        for name,data in blobs.items():
            fd=os.open(stage/name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
            with os.fdopen(fd,'wb') as stream: stream.write(data);stream.flush();os.fsync(stream.fileno())
        fd=os.open(stage,os.O_RDONLY|os.O_DIRECTORY);os.fsync(fd);os.close(fd)
        os.rename(stage,output)
        fd=os.open(output.parent,os.O_RDONLY|os.O_DIRECTORY);os.fsync(fd);os.close(fd)
    return {'format':1,'kind':'product-maintenance-package','profile':profile,'manifest_sha256':digest(raw),
            'files':len(ids),'authorization_verified':False,'installed':False,'starts_services':False}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['build-root','application','app-public','roles-policy','seed','output']:
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    try:
        result=create(args.build_root,args.application,args.app_public,args.roles_policy,args.seed,args.output)
    except (ValueError,OSError,subprocess.SubprocessError,AssertionError):
        parser.exit(1,'Product package refused\n')
    print(json.dumps(result,sort_keys=True))


if __name__=='__main__': main()
