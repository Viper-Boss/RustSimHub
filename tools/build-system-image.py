#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build/verify an INTERNAL ext4 image in a new directory. No physical-device access."""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import stat
import subprocess
import sys
import tomllib

ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools/rootfs/scripts'))
import rootfs_tools

IMAGE_BYTES=3436183552
GATES=('flash_allowed','release_ready','recovery_verified','board_boot_verified','private_data_included')


def require(condition,reason):
    if not condition: raise ValueError(reason)


def check_source(source,out):
    require(not out.exists() and not out.is_symlink(),'Output must be a new directory; prior evidence retained')
    require(out.parent.resolve(strict=True)==out.absolute().parent,'Output parent must be canonical')
    require(source.resolve(strict=True)==source and source.is_dir(),'Source must be canonical')
    manifest=source/'SYSTEM-CANDIDATE.json'
    require(not manifest.is_symlink() and manifest.is_file(),'Missing candidate manifest')
    candidate=json.loads(manifest.read_text())
    require(candidate.get('kind')=='internal-system-candidate' and all(candidate.get(k) is False for k in GATES),'Not an internal generic candidate')
    require(all(candidate.get(k) is True for k in ('app_service_group_installed','native_media_installed','kernel_audio_modules_installed')),'Complete application/media/kernel layers required')
    tree=source/'rootfs'
    require(not tree.is_symlink() and tree.is_dir(),'Missing regular candidate tree')
    require(rootfs_tools.tree_hash(tree)==(candidate['tree_sha256'],candidate['tree_entries']),'Candidate tree changed')
    state=tree/'var/lib/rustsimhub'
    require(state.is_dir() and not state.is_symlink() and not any(state.iterdir()),'Device state must be absent')
    board=tomllib.loads((ROOT/'firmware/board.toml').read_text())
    lock=json.loads((ROOT/'firmware/kernel/upstream-source.lock.json').read_text())
    require(board['immutable_content']['kernel_source_commit']==lock['commit']==candidate['kernel_audio_layer']['kernel_source_commit'],'Kernel attribution differs')
    require(lock.get('base_kernel_source_attributed') is True,'Base kernel source is not attributed')
    return candidate,tree,board,lock


MOUNT_CHECK='''
import hashlib,json,pathlib,subprocess,sys,tempfile
repo,source,image,out=map(pathlib.Path,sys.argv[1:]);sys.path.insert(0,str(repo/'tools/rootfs/scripts'))
import rootfs_tools
mount=pathlib.Path(tempfile.mkdtemp(prefix='verify-',dir=out))
try:
 subprocess.run(['mount','-t','ext4','-o','loop,ro,noload',str(image),str(mount)],check=True)
 try:
  excluded=[];h=hashlib.sha256();count=0
  for line in rootfs_tools.tree_lines(mount):
   if not (source/'rootfs/lost+found').exists() and line.endswith(' "lost+found"'):
    assert line=='d 0700 0 0 "lost+found"' and not list((mount/'lost+found').iterdir());excluded.append('lost+found');continue
   h.update(line.encode('utf8','surrogateescape')+b'\\n');count+=1
  expected=json.loads((source/'SYSTEM-CANDIDATE.json').read_text())
  assert (h.hexdigest(),count)==(expected['tree_sha256'],expected['tree_entries'])
  assert not any((mount/'var/lib/rustsimhub').iterdir())
  result={'mounted_readonly_no_journal_replay':True,'tree_content_mode_owner_capabilities_verified':True,
   'image_only_empty_directories':excluded,'tree_sha256':h.hexdigest(),'tree_entries':count}
  (out/'mounted-verification.json').write_text(json.dumps(result,indent=2))
 finally:subprocess.run(['umount',str(mount)],check=True)
finally:mount.rmdir()
'''


def build(source,out):
    candidate,tree,board,lock=check_source(source,out)
    spec=importlib.util.spec_from_file_location('generic_image_audit',ROOT/'tools/rootfs/scripts/audit-rootfs.py')
    audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)
    findings=audit.audit(tree)
    require(not findings,'Candidate generic audit failed; no image created')
    git=['git','-c','safe.directory='+str(ROOT),'-C',str(ROOT)]
    dirty=subprocess.check_output([*git,'status','--porcelain','--','tools/build-system-image.py',
                                   'tools/rootfs','firmware/board.toml','firmware/kernel/upstream-source.lock.json'])
    require(not dirty.strip(),'Commit image construction inputs first')
    revision=subprocess.check_output([*git,'rev-parse','HEAD'],text=True).strip()
    epoch=json.loads((ROOT/'firmware/base-system.lock.json').read_text())['source_date_epoch']
    out.mkdir(mode=0o700)
    report={'format':1,'kind':'internal-unflashable-system-image','passed':False,
            **{key:False for key in GATES},'composition_revision':candidate['composition_revision'],
            'application_revision':candidate['application_revision'],
            'candidate_manifest_sha256':rootfs_tools.sha256_file(source/'SYSTEM-CANDIDATE.json'),
            'tree_sha256':candidate['tree_sha256'],'tree_entries':candidate['tree_entries'],
            'builder_sha256':rootfs_tools.sha256_file(pathlib.Path(__file__)),
            'builder_revision':revision,'generic_audit_passed':True,
            'bytes':IMAGE_BYTES,'source_date_epoch':epoch,'base_kernel_source_commit':lock['commit'],
            'base_kernel_source_tree':lock['commit_tree'],'base_kernel_source_attribution_recorded':True,
            'independent_kernel_rebuild_verified':False,'private_firmware_included':False,
            'device_state_initialized':False,'partition_writes':0}
    image=out/'rootfs.img'
    def run(args,log,**kwargs):
        with (out/log).open('xb') as handle:
            subprocess.run(args,stdout=handle,stderr=subprocess.STDOUT,check=True,timeout=240,**kwargs)
    try:
        with image.open('xb') as handle:
            image.chmod(0o600);handle.truncate(IMAGE_BYTES)
        info=image.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_nlink==1,'Image output is not a new regular file')
        env=dict(os.environ,E2FSPROGS_FAKE_TIME=str(epoch))
        run(['mke2fs','-t','ext4','-F','-L','rootfs','-U','6e736866-7566-6930-3033-726f6f746673',
             '-E','lazy_itable_init=0,lazy_journal_init=0,hash_seed=6e736866-7566-6930-3033-686173687364',
             '-d',str(tree),str(image)],'mke2fs.log',env=env)
        commands=out/'inode-times.txt'
        commands.write_text('\n'.join(rootfs_tools.image_time_commands(tree,epoch))+'\n')
        run(['debugfs','-w','-f',str(commands),str(image)],'inode-times.log',env=env)
        run(['e2fsck','-f','-n',str(image)],'e2fsck.log',env=env)
        run(['dumpe2fs','-h',str(image)],'superblock.log')
        run(['unshare','--mount','--fork','--propagation','private',sys.executable,'-c',MOUNT_CHECK,
             str(ROOT),str(source),str(image),str(out)],'mounted-verification.log')
        require(rootfs_tools.tree_hash(tree)==(candidate['tree_sha256'],candidate['tree_entries']),'Source tree changed during image construction')
        report.update(source_tree_unchanged=True,image_sha256=rootfs_tools.sha256_file(image),
                      filesystem_clean_readonly_check=True,passed=True)
        return report
    finally:
        (out/'SYSTEM-IMAGE.json').write_text(json.dumps(report,indent=2)+'\n')
        for path in out.iterdir():
            if path.is_file() and not path.is_symlink():path.chmod(0o600)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source',type=pathlib.Path,required=True)
    parser.add_argument('--out',type=pathlib.Path,required=True)
    args=parser.parse_args()
    if sys.platform!='linux' or os.geteuid()!=0:parser.error('Requires Linux root for numeric owners and isolated loop verification')
    build(args.source.absolute(),args.out.absolute())
    print('Internal ext4 image matches every candidate entry; state/firmware absent and flash gates remain false')


if __name__=='__main__':main()
