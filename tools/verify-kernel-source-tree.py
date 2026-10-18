#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Verify the entire pinned kernel source tar against its Git tree. Never extracts it."""
import argparse
import hashlib
import json
import os
import pathlib
import stat
import tarfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
MAX_ARCHIVE = 512*1024*1024
MAX_FILE = 128*1024*1024
MAX_TOTAL = 3*1024*1024*1024
MAX_ENTRIES = 200000


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def encoded(text):
    return text.encode('utf8','surrogateescape')


def blob(data):
    return hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()


def source_tree(stream):
    leaves = {}; paths = {}; required_dirs = set(); prefix = None; total = 0
    with tarfile.open(fileobj=stream, mode='r|gz') as archive:
        for count, member in enumerate(archive,1):
            require(count<=MAX_ENTRIES, 'Too many archive entries')
            raw = member.name.rstrip('/') if member.isdir() else member.name
            parts = raw.split('/')
            require(parts and all(p not in ('','..','.') for p in parts)
                    and not raw.startswith('/') and '\0' not in raw, 'Unsafe source pathname')
            require(len(encoded(raw))<=4096, 'Source pathname too long')
            if prefix is None: prefix = parts[0]
            require(parts[0]==prefix, 'Multiple archive roots')
            require(raw not in paths, 'Duplicate archive pathname')
            for i in range(1,len(parts)):
                parent='/'.join(parts[:i])
                require(paths.get(parent,'dir')=='dir', 'A file or symlink shadows a source directory')
                required_dirs.add(parent)
            kind = 'dir' if member.isdir() else 'leaf'
            if kind=='leaf':
                require(raw not in required_dirs, 'File shadows existing source children')
            paths[raw]=kind
            if member.isdir(): continue
            require(len(parts)>1, 'Source root is not a directory')
            relative='/'.join(parts[1:])
            if member.issym():
                data=encoded(member.linkname)
                require(len(data)<=16384, 'Oversize Git symlink')
                mode='120000';identifier=blob(data)
            else:
                require(member.isfile() and 0<=member.size<=MAX_FILE, 'Link, device or oversize source entry')
                total+=member.size
                require(total<=MAX_TOTAL, 'Source data exceeds bound')
                mode='100755' if member.mode&0o111 else '100644'
                digest=hashlib.sha1(b'blob '+str(member.size).encode()+b'\0');size=0
                with archive.extractfile(member) as file:
                    for data in iter(lambda:file.read(1024*1024),b''):
                        size+=len(data);require(size<=member.size, 'Source length mismatch');digest.update(data)
                require(size==member.size, 'Truncated source entry')
                identifier=digest.hexdigest()
            leaves[relative]=(mode,identifier)
    require(leaves, 'Empty source archive')
    directories={'':{}}
    for path, facts in leaves.items():
        parts=path.split('/')
        for i in range(len(parts)):
            parent='/'.join(parts[:i]);directories.setdefault(parent,{})
            if i<len(parts)-1:
                directories[parent][parts[i]]=('40000','/'.join(parts[:i+1]))
            else: directories[parent][parts[i]]=facts
    hashes={}
    for name in sorted(directories,key=lambda n:(-n.count('/'),-len(n))):
        rows=[]
        for child,(mode,identifier) in sorted(directories[name].items(),key=lambda row:encoded(row[0]+('/' if row[1][0]=='40000' else ''))):
            sha=hashes[identifier] if mode=='40000' else identifier
            rows.append(mode.encode()+b' '+encoded(child)+b'\0'+bytes.fromhex(sha))
        data=b''.join(rows)
        hashes[name]=hashlib.sha1(b'tree '+str(len(data)).encode()+b'\0'+data).hexdigest()
    return {'archive_prefix':prefix,'file_count':len(leaves),'file_bytes':total,
            'calculated_git_tree':hashes[''],'qcom_tree':hashes.get('sound/soc/qcom')}


def verify(archive_path, report_path):
    require(not report_path.exists() and not report_path.is_symlink(), 'Previous report is retained')
    lock=json.loads((ROOT/'firmware/kernel/upstream-source.lock.json').read_text())
    expected=next(row for row in lock['assets'] if row['name'].endswith('.orig.tar.gz'))
    require(not archive_path.is_symlink(), 'Source archive symlink refused')
    with archive_path.open('rb') as stream:
        facts=os.fstat(stream.fileno())
        require(stat.S_ISREG(facts.st_mode) and facts.st_size==expected['bytes'] and facts.st_size<=MAX_ARCHIVE,
                'Expected a bounded regular pinned archive')
        digest=hashlib.sha256()
        for data in iter(lambda:stream.read(1024*1024),b''):digest.update(data)
        require(digest.hexdigest()==expected['sha256'], 'Source archive SHA256 mismatch')
        stream.seek(0)
        result=source_tree(stream)
    recorded=lock['source_tree']
    passed=(result['calculated_git_tree']==lock['commit_tree']==recorded['git_tree']
            and result['archive_prefix']==recorded['archive_prefix']
            and result['file_count']==recorded['file_count'] and result['file_bytes']==recorded['file_bytes']
            and result['qcom_tree']==recorded['qcom_git_tree'])
    report={'passed':passed,**result,'archive_sha256':expected['sha256'],
            'kernel_source_commit':lock['commit'],'pinned_git_tree':lock['commit_tree'],
            'files_extracted':False,'source_executed':False,'device_access':False,
            'complete_kernel_rebuilt':False,'boot_source_gate_resolved':False,
            'flash_allowed':False,'release_ready':False}
    with report_path.open('x',encoding='utf8') as handle:
        handle.write(json.dumps(report,indent=2)+'\n')
    require(passed,'Complete source Git tree differs; failed report retained')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive',type=pathlib.Path,required=True)
    parser.add_argument('--report',type=pathlib.Path,required=True)
    args=parser.parse_args();result=verify(args.archive,args.report)
    print(str(result['file_count'])+' source files/links exactly match the pinned Git tree; nothing extracted or executed')


if __name__=='__main__':main()
