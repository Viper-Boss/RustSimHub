#!/usr/bin/env python3
"""Audit staged source, report locations/categories without revealing matches."""
import re
import subprocess
import json
import pathlib
import argparse

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--revision', help='Audit a committed tree instead of the index')
args=parser.parse_args()
if args.revision:
    revision=subprocess.check_output(['git','rev-parse','--verify',args.revision+'^{commit}']).decode().strip()
    paths=subprocess.check_output(['git','ls-tree','-r','--name-only','-z',revision]).decode().split('\0')
else:
    revision=None
    paths = subprocess.check_output(['git','diff','--cached','--name-only','-z']).decode().split('\0')
patterns = {
    'credential':r'(?:ghp_|gho_|github_pat_)[A-Za-z0-9_]{20,}|-----BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY-----',
    'personal-host':r'192\.168\.120\.[0-9]+|wzs[0-9]{8,}|C:\\Users\\',
}
private_config=pathlib.Path('.private/nas.json')
if private_config.exists():
    password=json.loads(private_config.read_text(encoding='utf-8-sig')).get('password')
    if password: patterns['private-password']=re.escape(password)
findings=[]
for path in filter(None,paths):
    if path.startswith(('.local/','.private/','target/')):
        findings.append((path,0,'forbidden-directory')); continue
    data=subprocess.check_output(['git','show',(revision or '')+':'+path]).decode('utf-8-sig')
    for category,pattern in patterns.items():
        for number,line in enumerate(data.splitlines(),1):
            if re.search(pattern,line): findings.append((path,number,category))
if findings:
    for path,line,category in findings: print(f'{path}:{line}: {category}')
    raise SystemExit(1)
print(f'Source privacy pattern audit passed: {len(list(filter(None,paths)))} files')
