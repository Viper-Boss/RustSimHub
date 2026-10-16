#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compose an internal system tree from verified base/application inputs. No flashing."""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/rootfs/scripts'))
import rootfs_tools as base_tools
from application_package import verify_package, FILES as APP_FILES

NAMES = ('rustsimhub-core', 'simhubctl', 'rustsimhub-edge', 'rustsimhub-update',
         'rustsimhub-recovery', 'rustsimhub-migrate')


def git(*args):
    return subprocess.check_output(['git', '-c', 'safe.directory='+str(ROOT), '-C', str(ROOT), *args])


def destination(root, relative):
    path = pathlib.PurePosixPath(relative)
    assert not path.is_absolute() and '..' not in path.parts
    # No overlay pathname may traverse a pre-existing symlink in the base.
    current = root
    for part in path.parts[:-1]:
        current /= part
        assert not current.is_symlink(), 'Overlay parent is a symlink'
        current.mkdir(mode=0o755, exist_ok=True)
    target = current / path.name
    assert not target.is_symlink(), 'Overlay target is a symlink'
    return target


def group_member(root, group, member):
    for file, index in [('group', 3), ('gshadow', 3)]:
        path = root / 'etc' / file
        rows = path.read_text().splitlines()
        matches = [i for i, line in enumerate(rows) if line.startswith(group+':')]
        assert len(matches) == 1
        i = matches[0]
        fields = rows[i].split(':')
        fields[index] = ','.join(sorted(set(filter(None, fields[index].split(','))) | {member}))
        rows[i] = ':'.join(fields)
        path.write_text('\n'.join(rows)+'\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('base', 'artifacts', 'out'):
        parser.add_argument('--'+name, type=pathlib.Path, required=True)
    for name in ('signed-application','release-public','updater-artifacts'):
        parser.add_argument('--'+name,type=pathlib.Path)
    parser.add_argument('--native-artifacts', type=pathlib.Path, help='Explicit same-source worker provenance; never inferred from an older signed payload')
    args = parser.parse_args()
    startup = bool(args.signed_application)
    assert not args.native_artifacts or startup
    assert all((args.signed_application,args.release_public,args.updater_artifacts)) or not any((args.signed_application,args.release_public,args.updater_artifacts))
    if sys.platform != 'linux' or os.geteuid() != 0:
        parser.error('Linux root is required to preserve numeric owners')
    if args.out.exists() or args.out.is_symlink():
        parser.error('Output must be a new directory; failed output is retained')
    assert not git('status', '--porcelain', '--', 'firmware/system', 'firmware/base-system.lock.json',
                   'tools/compose-system-tree.py', 'tools/application_package.py', 'tools/rootfs').strip(), 'Commit composition inputs first'
    revision = git('rev-parse', 'HEAD').decode().strip()
    base = args.base.resolve(strict=True)
    manifest = json.loads((base / 'ROOTFS-MANIFEST.json').read_text())
    assert manifest['kind'] == 'rustsimhub-ufi003-rootfs-base' and manifest['release_input'] is True
    # Git's committed bytes are authoritative, including its LF normalization.
    lock_sha = hashlib.sha256(git('show', revision+':firmware/base-system.lock.json')).hexdigest()
    assert manifest['lock']['sha256'] == lock_sha
    assert manifest['tree_sha256'] == base_tools.tree_hash(base / 'rootfs')[0]
    artifacts = args.artifacts.resolve(strict=True)
    build = json.loads((artifacts / 'build.json').read_text())
    assert build['target'] == 'aarch64-unknown-linux-gnu' and build['development_fixtures'] is False
    assert re.fullmatch('[a-f0-9]{40}', build['revision'])
    assert re.fullmatch('[a-f0-9]{64}', build['source_archive_sha256'])
    for name in NAMES:
        path = artifacts / name
        assert path.is_file() and not path.is_symlink()
        assert base_tools.sha256_file(path) == build['artifact_sha256'][name]
        header = subprocess.check_output(['readelf', '-h', str(path)], text=True)
        assert 'AArch64' in header
    if startup:
        signed = args.signed_application.resolve(strict=True)
        release = args.release_public.resolve(strict=True)
        app = verify_package(signed,release,build['source_archive_sha256'])
        for name in ('rustsimhub-core','simhubctl','rustsimhub-edge'):
            assert base_tools.sha256_file(signed/name)==build['artifact_sha256'][name]
        updater = args.updater_artifacts.resolve(strict=True)
        verifier = json.loads((updater/'build.json').read_text())
        assert verifier['target']=='aarch64-unknown-linux-gnu' and verifier['development_fixtures'] is False
        assert verifier.get('boot_runtime_selection_supported') is True,'Updater lacks the boot group selection contract'
        assert verifier.get('local_installation_preparation_supported') is True,'Immutable Edge lacks atomic initialization'
        assert base_tools.sha256_file(updater/'rustsimhub-edge')==verifier['artifact_sha256']['rustsimhub-edge']
        assert base_tools.sha256_file(updater/'rustsimhub-update')==verifier['artifact_sha256']['rustsimhub-update']
        assert 'AArch64' in subprocess.check_output(['readelf','-h',str(updater/'rustsimhub-update')],text=True)
        if args.native_artifacts:
            native = args.native_artifacts.resolve(strict=True)
            media_build = json.loads((native/'build.json').read_text())
            assert media_build['revision'] == build['revision'] and media_build['source_archive_sha256'] == build['source_archive_sha256']
            assert media_build['target'] == 'aarch64-unknown-linux-gnu' and media_build.get('worker_control_supported') is True and media_build['exit_code'] == 0
            assert verifier.get('media_installation_preparation_supported') is True, 'Immutable setup lacks the media installation profile'
            assert base_tools.sha256_file(native/'rustsimhub-audio-alsa') == media_build['binary_sha256'] == base_tools.sha256_file(signed/'rustsimhub-audio-alsa')
    args.out.mkdir(mode=0o700)
    output = args.out.resolve(strict=True)
    try:
        subprocess.run(['cp', '-a', '--reflink=auto', str(base / 'rootfs'), str(output / 'rootfs')], check=True)
        tree = output / 'rootfs'
        for name, number, home in [('rustsimhub-core', 986, '/var/lib/rustsimhub'),
                                   ('rustsimhub-edge', 985, '/nonexistent'),
                                   ('rustsimhub-media', 984, '/nonexistent')]:
            base_tools.ensure_service_user(tree, name, number, number, home)
        groups = (tree / 'etc/group').read_text().splitlines()
        assert not any(line.split(':')[0] == 'rustsimhub-ipc' or line.split(':')[2] == '987' for line in groups)
        base_tools._edit_lines(tree / 'etc/group', 'rustsimhub-ipc', 'rustsimhub-ipc:x:987:', 0o644)
        base_tools._edit_lines(tree / 'etc/gshadow', 'rustsimhub-ipc', 'rustsimhub-ipc:!::', 0o640)
        for member in ('rustsimhub-core', 'rustsimhub-edge', 'rustsimhub-media'):
            group_member(tree, 'rustsimhub-ipc', member)
        group_member(tree, 'audio', 'rustsimhub-media')
        prefix = 'firmware/system/overlay/'
        paths = git('ls-tree', '-r', '--name-only', '-z', revision, '--', prefix).decode().split('\0')
        overlay_hashes = {}
        for name in filter(None, paths):
            row = git('ls-tree', revision, '--', name).decode()
            assert row.startswith('100644 ')
            relative = name[len(prefix):]
            data = git('show', revision+':'+name)
            target = destination(tree, relative)
            target.write_bytes(data)
            os.chown(target, 0, 0)
            mode = 0o755 if relative.startswith('usr/local/libexec/') else 0o644
            if relative.endswith('.nmconnection'):
                mode = 0o600
            target.chmod(mode)
            overlay_hashes[relative] = hashlib.sha256(data).hexdigest()
        slot = 'opt/rustsimhub/factory/'+build['source_archive_sha256']
        for name in NAMES:
            target = destination(tree, slot+'/'+name)
            shutil.copyfile(artifacts / name, target)
            os.chown(target, 0, 0)
            target.chmod(0o555)
        destination(tree, slot+'/build.json').write_text(json.dumps(build, indent=2)+'\n')
        if startup:
            for name in (*APP_FILES,'manifest.json','manifest.sig'):
                target=destination(tree,'usr/share/rustsimhub/factory-app/'+name)
                shutil.copyfile(signed/name,target);os.chown(target,0,0);target.chmod(0o555 if name in APP_FILES[:-1] else 0o444)
            target=destination(tree,'usr/share/rustsimhub/release.public')
            shutil.copyfile(release,target);os.chown(target,0,0);target.chmod(0o444)
            for name,origin in [('rustsimhub-update',updater),('rustsimhub-migrate',artifacts)]:
                target=destination(tree,'usr/libexec/rustsimhub/'+name)
                shutil.copyfile(origin/name,target);os.chown(target,0,0);target.chmod(0o555)
                assert base_tools.sha256_file(target)==(verifier if name=='rustsimhub-update' else build)['artifact_sha256'][name]
            target=destination(tree,'usr/libexec/rustsimhub/rustsimhub-edge-setup')
            shutil.copyfile(updater/'rustsimhub-edge',target);os.chown(target,0,0);target.chmod(0o555)
            assert base_tools.sha256_file(target)==verifier['artifact_sha256']['rustsimhub-edge']
            verify_package(tree/'usr/share/rustsimhub/factory-app',tree/'usr/share/rustsimhub/release.public',build['source_archive_sha256'])
            destination(tree,'opt/rustsimhub/versions').mkdir(mode=0o700)
            destination(tree,'opt/rustsimhub/selected').mkdir(mode=0o755)
            destination(tree,'var/lib/rustsimhub-core').mkdir(mode=0o755)
            destination(tree,'var/lib/rustsimhub-edge').mkdir(mode=0o755)
            destination(tree,'var/lib/rustsimhub-media').mkdir(mode=0o755)
        state_mount = destination(tree, 'var/lib/rustsimhub')
        state_mount.mkdir(mode=0o700, exist_ok=True)
        assert not any(state_mount.iterdir())
        state_mount.chmod(0o700)
        units = ['rustsimhub-firstboot.service', 'rustsimhub-usb.service',
                 'rustsimhub-device-firmware.service', 'rustsimhub-radio-resume.service']
        if startup:units+=['rustsimhub-app.target']
        for unit in units:
            link = destination(tree, 'etc/systemd/system/multi-user.target.wants/'+unit)
            assert not link.exists()
            link.symlink_to('../'+unit)
        mount = destination(tree, 'etc/systemd/system/local-fs.target.requires/var-lib-rustsimhub.mount')
        assert not mount.exists()
        mount.symlink_to('../var-lib-rustsimhub.mount')
        candidate = {'format': 1, 'kind': 'internal-system-candidate', 'composition_revision': revision,
                     'base_tree_sha256': manifest['tree_sha256'], 'base_lock_sha256': lock_sha,
                     'application_revision': build['revision'], 'application_source_sha256': build['source_archive_sha256'],
                     'application_sha256': build['artifact_sha256'], 'overlay_sha256': overlay_hashes,
                     'flash_allowed': False, 'release_ready': False, 'recovery_verified': False,
                     'app_service_group_installed': startup, 'native_media_installed': bool(args.native_artifacts),
                     'media_configuration_required': bool(args.native_artifacts),
                     'board_boot_verified': False, 'private_data_included': False}
        if startup:
            candidate['factory_manifest_sha256']=base_tools.sha256_file(signed/'manifest.json')
            candidate['release_public_sha256']=base_tools.sha256_file(release)
            candidate['immutable_updater_build']=verifier
            candidate['automatic_update_health_confirmation']=False
            if args.native_artifacts: candidate['native_media_build'] = media_build
        destination(tree, 'usr/share/rustsimhub/system-candidate.json').write_text(json.dumps(candidate, indent=2)+'\n')
        spec = importlib.util.spec_from_file_location('base_audit', ROOT / 'tools/rootfs/scripts/audit-rootfs.py')
        audit = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(audit)
        findings = audit.audit(tree)
        if findings:
            raise RuntimeError('Candidate generic audit: '+', '.join(findings))
        base_tools.clamp_mtime(tree, manifest['source_date_epoch'])
        candidate['tree_sha256'], candidate['tree_entries'] = base_tools.tree_hash(tree)
        assert base_tools.tree_hash(base / 'rootfs')[0] == manifest['tree_sha256']
        (output / 'SYSTEM-CANDIDATE.json').write_text(json.dumps(candidate, indent=2)+'\n')
        print('Internal board/application tree composed and audited; flash remains prohibited')
    except Exception:
        (output / 'COMPOSITION.FAILED').write_text('Partial output retained; not a usable candidate\n')
        raise


if __name__ == '__main__':
    main()
