#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Private development-signed production binaries; never a production release key."""
import argparse,hashlib,json,pathlib,re,sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'.local/python-deps'))
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

p=argparse.ArgumentParser(description=__doc__)
for name in ('artifacts','native','native-log','output'):p.add_argument('--'+name,type=pathlib.Path,required=True)
p.add_argument('--require-media-worker',action='store_true')
a=p.parse_args();assert not a.output.exists()
assert a.output.resolve().is_relative_to((ROOT/'.private').resolve()),'Development signing material stays private'
build=json.loads((a.artifacts/'build.json').read_text())
assert build['development_fixtures'] is False and build['target']=='aarch64-unknown-linux-gnu'
source=build['source_archive_sha256'];assert re.fullmatch('[0-9a-f]{64}',source)
log=a.native_log.read_text(encoding='utf-8')
infos=[json.loads(line) for line in log.splitlines() if line.startswith('{')]
assert len(infos)==1 and infos[0]['source_archive_sha256']==source
assert set(infos[0]) <= {'source_archive_sha256','local_media_worker_supported'}
assert not a.require_media_worker or infos[0].get('local_media_worker_supported') is True
names=['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa','Opus-COPYING.txt']
blobs={name:(a.native if name==names[3] else ROOT/'licenses'/name if name==names[4] else a.artifacts/name).read_bytes() for name in names}
for name in names[:3]:assert hashlib.sha256(blobs[name]).hexdigest()==build['artifact_sha256'][name]
native_sha=hashlib.sha256(blobs[names[3]]).hexdigest()
assert native_sha+'  /out/rustsimhub-audio-alsa' in log
assert 'test result: FAILED' not in log and 'Finished `release`' in log
assert sum(map(len,blobs.values()))<=24*1024*1024
key=Ed25519PrivateKey.generate()
a.output.mkdir(mode=0o700)
def write(path,data,mode=0o600):
    with path.open('xb') as file:file.write(data)
    path.chmod(mode)
write(a.output/'release.seed',key.private_bytes(serialization.Encoding.Raw,serialization.PrivateFormat.Raw,serialization.NoEncryption()))
write(a.output/'release.public',key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw))
for epoch in (1,2,3):
    directory=a.output/f'package{epoch}';directory.mkdir(mode=0o700)
    manifest={'format':1,'kind':'application','board':'ufi003-v02','arch':'aarch64-linux','epoch':epoch,'schema_min':5,'schema_max':5,'source_sha256':source,'development_fixtures':False,'files':[{'name':n,'bytes':len(blobs[n]),'sha256':hashlib.sha256(blobs[n]).hexdigest()} for n in names]}
    raw=json.dumps(manifest,separators=(',',':')).encode()
    write(directory/'manifest.json',raw)
    write(directory/'manifest.sig',key.sign(b'RustSimHub.AppManifest.v1\0'+raw))
    for name,data in blobs.items():write(directory/name,data,0o555 if name!=names[4] else 0o444)
write(a.output/'provenance.json',json.dumps({'development_signer':True,'production_release':False,'application_build':build,'native_sha256':native_sha,'native_build_log_sha256':hashlib.sha256(a.native_log.read_bytes()).hexdigest()},indent=2).encode())
print('Three development-signed application epochs prepared privately; no production release or device change')
