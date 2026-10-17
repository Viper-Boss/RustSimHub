#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the candidate's ARM64 modprobe in a read-only tree without CAP_SYS_MODULE."""
import argparse
import json
import os
import pathlib
import stat
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tools/rootfs/scripts'))
import audio_modules as audio
import rootfs_tools


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--system-tree', type=pathlib.Path, required=True)
    parser.add_argument('--report', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'linux' or os.geteuid() != 0:
        parser.error('Requires Linux root, namespaces and ARM64 binfmt')
    if args.report.exists() or args.report.is_symlink():
        parser.error('Report exists; previous evidence is retained')
    tree = args.system_tree.resolve(strict=True)
    candidate = json.loads((tree.parent/'SYSTEM-CANDIDATE.json').read_text())
    audio.require(candidate['kind']=='internal-system-candidate' and candidate['kernel_audio_modules_installed'], 'Not an audio candidate')
    audio.require(all(candidate[n] is False for n in ('flash_allowed','release_ready','board_boot_verified','recovery_verified')), 'Unexpected candidate gates')
    value, source_sha = audio.profile()
    layer = candidate['kernel_audio_layer']
    audio.require(layer['source_lock_sha256']==source_sha, 'Candidate uses another audio source lock')
    installed = tree/'usr/lib/modules'/audio.RELEASE/'updates/rustsimhub-audio'
    audio.verify_artifacts(installed, value)
    for file in installed.iterdir():
        facts = file.stat()
        audio.require((facts.st_uid,facts.st_gid,stat.S_IMODE(facts.st_mode))==(0,0,0o444), 'Module ownership/mode differs')
    audio.require(json.loads((tree/'usr/share/rustsimhub/audio-modules.json').read_text())==layer, 'Installed layer manifest differs')
    profile = tree/'usr/share/rustsimhub/audio-driver-profile'
    audio.require(audio.digest(audio.regular_bytes(profile, 8192))==layer['driver_profile_sha256'], 'Runtime driver profile differs')
    before = rootfs_tools.tree_hash(tree)
    audio.require(before==(candidate['tree_sha256'],candidate['tree_entries']), 'Candidate tree changed')
    report = {'passed':False,'composition_revision':candidate['composition_revision'],
              'tree_sha256':before[0],'module_count':22,'loaded_profile_count':16,
              'target_modprobe_emulated':True,'cap_sys_module_removed':True,
              'hardware_access':False,'network_access':False,'modules_loaded':False,
              'audio_driver_service_started':False,'board_boot_verified':False,'flash_allowed':False}
    try:
        with tempfile.TemporaryDirectory(prefix='rustsimhub-module-index-') as temporary:
            mounted = pathlib.Path(temporary)/'root'
            mounted.mkdir()
            script = '''
set -eu
mount --bind "$1" "$2"
mount -o remount,bind,ro "$2"
tree=$2
release=$3
shift 3
for module in "$@"; do
 printf 'MODULE=%s\\n' "$module"
 setpriv --no-new-privs --bounding-set=-sys_module chroot "$tree" /usr/sbin/modprobe -C /dev/null -S "$release" --show-depends --ignore-install "$module"
done
for alias in 'of:N*T*Cqcom,q6afe-dais' 'of:N*T*Cqcom,q6asm' 'of:N*T*Cqcom,q6voice-dais' 'of:N*T*Cqcom,apq8016-sbc-sndcard'; do
 printf 'ALIAS=%s\\n' "$alias"
 setpriv --no-new-privs --bounding-set=-sys_module chroot "$tree" /usr/sbin/modprobe -C /dev/null -S "$release" --resolve-alias "$alias"
done
'''
            result = subprocess.run(['timeout','--kill-after=2','45','unshare','--mount','--net','--fork',
                '--propagation','private','sh','-ec',script,'target-module-probe',str(tree),str(mounted),audio.RELEASE,
                *(r['name'][:-3] for r in value['expected_modules'])], env=audio.ENV,
                capture_output=True,text=True,timeout=55)
            report.update(exit_code=result.returncode,diagnostics=result.stderr,output=result.stdout)
            audio.require(result.returncode==0, 'Target modprobe execution failed')
            groups = {}; name = None
            for line in result.stdout.splitlines():
                if line.startswith(('MODULE=','ALIAS=')):
                    audio.require(line not in groups, 'Duplicate probe result')
                    name=line;groups[name]=[]
                else:
                    audio.require(name is not None, 'Unexpected probe output')
                    groups[name].append(line)
            expected_count = len(value['expected_modules'])+len(audio.ALIASES)
            audio.require(len(groups)==expected_count, 'Incomplete target probe')
            for row in value['expected_modules']:
                paths=[]
                for line in groups['MODULE='+row['name'][:-3]]:
                    fields=line.split()
                    audio.require(len(fields)==2 and fields[0]=='insmod', 'Target proposed unexpected action/options')
                    paths.append(pathlib.PurePosixPath(fields[1]).relative_to('/lib/modules/'+audio.RELEASE).as_posix())
                audio.require(paths==layer['dependencies'][row['name']], 'Target and host module resolution differ')
            for alias, expected in audio.ALIASES.items():
                audio.require(groups['ALIAS='+alias]==[expected], 'Target alias resolution differs')
            report.update(passed=True,all_22_target_dependency_graphs_match=True,all_4_target_aliases_match=True)
    finally:
        report['tree_unchanged']=rootfs_tools.tree_hash(tree)==before
        report['passed']=report['passed'] and report['tree_unchanged']
        args.report.write_text(json.dumps(report,indent=2)+'\n')
    audio.require(report['passed'], 'Audio candidate validation failed')
    print('Target ARM64 modprobe: all 22 driver graphs and four aliases match; no modules loaded; tree unchanged')


if __name__=='__main__':
    main()
