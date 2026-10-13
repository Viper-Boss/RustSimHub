#!/usr/bin/env python3
"""Explicit source allowlist; .private/.local/target never enter NAS build inputs."""
import pathlib
import tarfile
ROOT = pathlib.Path(__file__).resolve().parents[1]
output = ROOT / '.local' / 'source.tar.gz'
with tarfile.open(output,'w:gz') as archive:
    for name in ['Cargo.toml','Cargo.lock','LICENSE','NOTICE.md']:
        path = ROOT / name
        if not path.exists(): raise SystemExit(f'Missing required file: {name}')
        archive.add(path,arcname=name)
    for source in [ROOT / 'crates', ROOT / 'native' / 'audio-alsa']:
        for path in sorted(source.rglob('*')):
            if path.is_file() and (path.suffix in {'.rs','.toml','.sql'} or path.name == 'Cargo.lock'):
                if 'target' in path.relative_to(source).parts:
                    continue
                archive.add(path,arcname=path.relative_to(ROOT).as_posix())
print('Build source archive created (explicit source allowlist)')
