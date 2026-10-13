#!/usr/bin/env python3
"""Package exact committed Git blobs from the explicit build source allowlist."""
import gzip
import io
import pathlib
import subprocess
import tarfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
output=ROOT/'.local/source.tar.gz'
def git(*args): return subprocess.check_output(['git',*args],cwd=ROOT)
revision=git('rev-parse','--verify','HEAD^{commit}').decode().strip()
paths=git('ls-tree','-r','--name-only','-z',revision).decode().split('\0')
def allowed(name):
    path=pathlib.PurePosixPath(name)
    if name in {'Cargo.toml','Cargo.lock','LICENSE','NOTICE.md'}: return True
    if name.startswith('licenses/') and path.suffix=='.txt': return True
    if not (name.startswith('crates/') or name.startswith('native/audio-alsa/')) or 'target' in path.parts: return False
    return path.suffix in {'.rs','.toml','.sql'} or path.name=='Cargo.lock' or (name.startswith('crates/edge/src/web/') and path.suffix in {'.html','.js','.css'})
paths=sorted(filter(allowed,paths))
for required in ['Cargo.toml','Cargo.lock','LICENSE','NOTICE.md']:
    if required not in paths: raise SystemExit(f'Missing committed input: {required}')
# Require all staged/unstaged source changes to be committed first; Git normalizes CRLF.
changed=git('status','--porcelain=v1','--untracked-files=all','--','Cargo.toml','Cargo.lock','LICENSE','NOTICE.md','licenses','crates','native/audio-alsa').decode()
if changed.strip(): raise SystemExit('Commit build-input changes before packaging; no working-tree source is silently omitted')
output.parent.mkdir(parents=True,exist_ok=True)
with output.open('wb') as target, gzip.GzipFile(filename='',mode='wb',fileobj=target,mtime=0) as zipped, tarfile.open(fileobj=zipped,mode='w',format=tarfile.USTAR_FORMAT) as archive:
    for name in paths:
        data=git('show',revision+':'+name)
        entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o644;entry.mtime=0
        archive.addfile(entry,io.BytesIO(data))
print(f'Committed build source archive created: {len(paths)} files, revision {revision[:12]}')
