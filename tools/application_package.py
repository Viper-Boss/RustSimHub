# SPDX-License-Identifier: AGPL-3.0-only
"""Offline validation for the fixed signed application format; never executes payloads."""
import hashlib
import json
import os
import pathlib
import re
import stat
import subprocess
import tempfile

FILES = ('rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa','Opus-COPYING.txt')


def checked(path, limit):
    with os.fdopen(os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK),'rb') as file:
        meta=os.fstat(file.fileno())
        assert stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1 and meta.st_size <= limit
        data=file.read(limit+1);assert len(data)<=limit
        return data


def unique_object(pairs):
    result=dict(pairs)
    assert len(result)==len(pairs),'Duplicate manifest fields'
    return result


def verify_package(directory, public_path, source, schema=5):
    directory = pathlib.Path(directory);public_path = pathlib.Path(public_path)
    public = checked(public_path,32);assert len(public)==32
    manifest = checked(directory/'manifest.json',8192)
    signature = checked(directory/'manifest.sig',64);assert len(signature)==64
    assert re.fullmatch('[0-9a-f]{64}',source)
    m=json.loads(manifest,object_pairs_hook=unique_object)
    assert set(m)=={'format','kind','board','arch','epoch','schema_min','schema_max','source_sha256','development_fixtures','files'}
    assert type(m['format']) is int and m['format']==1 and m['kind']=='application' and m['board']=='ufi003-v02' and m['arch']=='aarch64-linux'
    assert type(m['epoch']) is int and 0<m['epoch']<2**64
    assert type(schema) is int and 0<=schema<2**32
    assert type(m['schema_min']) is int and type(m['schema_max']) is int and 0<=m['schema_min']<=schema<=m['schema_max']<2**32
    assert m['development_fixtures'] is False and m['source_sha256']==source
    assert len(m['files'])==len(FILES)
    total=0
    for entry,name in zip(m['files'],FILES):
        assert set(entry)=={'name','bytes','sha256'} and entry['name']==name
        data=checked(directory/name,24*1024*1024)
        assert type(entry['bytes']) is int and 0<len(data)==entry['bytes']
        assert hashlib.sha256(data).hexdigest()==entry['sha256']
        total+=len(data)
        if name!=FILES[-1]:
            assert len(data)>=64 and data[:7]==b'\x7fELF\x02\x01\x01'
            assert int.from_bytes(data[16:18],'little') in (2,3) and int.from_bytes(data[18:20],'little')==183
            assert int.from_bytes(data[52:54],'little')==64
    assert total<=24*1024*1024
    # RFC8410 SubjectPublicKeyInfo for a raw Ed25519 public key. OpenSSL verifies
    # the exact domain-prefixed bytes, matching the Rust runtime verifier.
    with tempfile.TemporaryDirectory() as temp:
        temp=pathlib.Path(temp)
        (temp/'public.der').write_bytes(bytes.fromhex('302a300506032b6570032100')+public)
        (temp/'signed').write_bytes(b'RustSimHub.AppManifest.v1\0'+manifest)
        subprocess.run(['openssl','pkeyutl','-verify','-pubin','-keyform','DER','-inkey',str(temp/'public.der'),'-rawin','-in',str(temp/'signed'),'-sigfile',str(directory/'manifest.sig')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    return m
