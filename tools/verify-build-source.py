#!/usr/bin/env python3
"""Compare the local NAS source archive with committed Git blobs."""
import pathlib
import subprocess
import tarfile

ROOT=pathlib.Path(__file__).resolve().parents[1]
with tarfile.open(ROOT/'.local/source.tar.gz','r:gz') as archive:
    count=0
    for member in archive.getmembers():
        if not member.isfile(): continue
        source=archive.extractfile(member).read()
        committed=subprocess.check_output(['git','show','HEAD:'+member.name],cwd=ROOT)
        if source != committed:
            raise SystemExit(f'Build archive differs from committed file: {member.name}')
        count+=1
print(f'Build archive matches committed source: {count} files')
