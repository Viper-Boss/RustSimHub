#!/usr/bin/env python3
"""Compare the local NAS source archive with committed Git blobs."""
import pathlib
import subprocess
import tarfile
import argparse

ROOT=pathlib.Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--archive',type=pathlib.Path,default=ROOT/'.local/source.tar.gz')
parser.add_argument('--revision',default='HEAD')
args=parser.parse_args()
revision=subprocess.check_output(['git','rev-parse','--verify',args.revision+'^{commit}'],cwd=ROOT).decode().strip()
with tarfile.open(args.archive,'r:gz') as archive:
    count=0
    for member in archive.getmembers():
        if not member.isfile(): continue
        source=archive.extractfile(member).read()
        committed=subprocess.check_output(['git','show',revision+':'+member.name],cwd=ROOT)
        if source != committed:
            raise SystemExit(f'Build archive differs from committed file: {member.name}')
        count+=1
print(f'Build archive matches committed source: {count} files')
