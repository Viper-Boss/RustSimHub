#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run verified ARM64 programs in an immutable base, without network or hardware."""
import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/rootfs/scripts'))
import rootfs_tools

NAMES = ('rustsimhub-core', 'simhubctl', 'rustsimhub-edge', 'rustsimhub-update',
         'rustsimhub-recovery', 'rustsimhub-migrate')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--system-tree', action='store_true', help='Verify installed programs in a composed internal tree')
    for name in ('base', 'artifacts', 'report'):
        parser.add_argument('--' + name, type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'linux' or os.geteuid() != 0:
        parser.error('Requires Linux root with qemu ARM64 binfmt and mount namespaces')
    if args.report.exists():
        parser.error('Report exists; inspect the previous attempt')
    base = args.base.resolve(strict=True)
    artifacts = args.artifacts.resolve(strict=True)
    manifest = json.loads((base.parent / ('SYSTEM-CANDIDATE.json' if args.system_tree else 'ROOTFS-MANIFEST.json')).read_text())
    build = json.loads((artifacts / 'build.json').read_text())
    if args.system_tree:
        assert manifest['kind'] == 'internal-system-candidate' and manifest['flash_allowed'] is False
        assert manifest['application_revision'] == build['revision']
        assert manifest['application_source_sha256'] == build['source_archive_sha256']
        program_directory = '/opt/rustsimhub/factory/'+build['source_archive_sha256']
    else:
        assert manifest['kind'] == 'rustsimhub-ufi003-rootfs-base' and manifest['release_input'] is True
        program_directory = '/mnt'
    assert build['target'] == 'aarch64-unknown-linux-gnu'
    assert build['development_fixtures'] is False
    assert re.fullmatch('[a-f0-9]{64}', build['source_archive_sha256'])
    assert base.stat().st_uid == 0
    before = rootfs_tools.tree_hash(base)[0]
    assert before == manifest['tree_sha256']
    for name in NAMES:
        assert not (artifacts / name).is_symlink()
        assert rootfs_tools.sha256_file(artifacts / name) == build['artifact_sha256'][name]
        if args.system_tree:
            installed = base / program_directory.lstrip('/') / name
            assert installed.is_file() and not installed.is_symlink()
            assert installed.stat().st_uid == 0 and installed.stat().st_mode & 0o777 == 0o555
            assert rootfs_tools.sha256_file(installed) == build['artifact_sha256'][name]
    native = args.system_tree and manifest.get('native_media_installed') is True
    if native:
        media = manifest['native_media_build']
        assert media['revision'] == build['revision'] and media['source_archive_sha256'] == build['source_archive_sha256']
        installed = base / 'usr/share/rustsimhub/factory-app/rustsimhub-audio-alsa'
        assert installed.is_file() and not installed.is_symlink()
        assert installed.stat().st_uid == 0 and installed.stat().st_mode & 0o777 == 0o555
        assert rootfs_tools.sha256_file(installed) == media['binary_sha256']
    report = {'passed': False, 'hardware_access': False, 'network_access': False,
              'application_revision': build['revision'],
              'source_archive_sha256': build['source_archive_sha256'],
              'base_tree_sha256': before, 'composed_system': args.system_tree,
              'program_directory': program_directory, 'native_media_checked': native}
    try:
        with tempfile.TemporaryDirectory(prefix='rustsimhub-base-runtime-') as temporary:
            work = pathlib.Path(temporary)
            payload = work / 'payload'
            mounted = work / 'mounted'
            payload.mkdir(mode=0o755)
            mounted.mkdir()
            for name in NAMES:
                shutil.copyfile(artifacts / name, payload / name)
                (payload / name).chmod(0o555)
            # Each bind is confined to the child mount namespace. The built
            # tree never receives a payload, device, procfs or a writable bind.
            script = '''
set -eu
mount --bind "$1" "$2"
mount -o remount,bind,ro "$2"
mount --bind "$3" "$2/mnt"
mount -o remount,bind,ro "$2/mnt"
for name in rustsimhub-core simhubctl rustsimhub-edge rustsimhub-update rustsimhub-recovery rustsimhub-migrate;do
 setpriv --no-new-privs chroot --userspec=986:986 "$2" "$4/$name" --build-info
done
if test "$5" = yes;then
 setpriv --no-new-privs chroot --userspec=984:984 "$2" /usr/share/rustsimhub/factory-app/rustsimhub-audio-alsa --build-info
fi
setpriv --no-new-privs chroot --userspec=986:986 "$2" "$4/rustsimhub-core" --self-check
setpriv --no-new-privs chroot --userspec=986:986 "$2" "$4/simhubctl" --help >/dev/null
'''
            result = subprocess.run(
                ['timeout', '--kill-after=2', '35', 'unshare', '--mount', '--net', '--fork', '--propagation', 'private',
                 'sh', '-ec', script, 'runtime-probe', str(base), str(mounted), str(payload), program_directory, 'yes' if native else 'no'],
                env={'PATH': '/usr/sbin:/usr/bin:/sbin:/bin', 'LC_ALL': 'C'},
                capture_output=True, text=True, timeout=45)
            report['exit_code'] = result.returncode
            report['output'] = result.stdout
            report['diagnostics'] = result.stderr
            assert result.returncode == 0
            values = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
            assert len(values) == 8 if native else len(values) == 7
            assert all(info['source_archive_sha256'] == build['source_archive_sha256'] for info in values[:6])
            assert values[0]['development_fixtures'] is False
            if native:
                assert values[6]['source_archive_sha256'] == media['source_archive_sha256']
                assert values[6]['local_media_worker_supported'] is True
            assert values[-1]['ok'] is True and values[-1]['hardware_mutations'] is False
            report['program_count'] = 7 if native else 6
            report['passed'] = True
    finally:
        report['base_tree_unchanged'] = rootfs_tools.tree_hash(base)[0] == before
        report['passed'] = report['passed'] and report['base_tree_unchanged']
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2), encoding='utf-8')
    assert report['passed']
    print(f"{report['program_count']} production ARM64 programs run in the unchanged base; network and hardware absent")


if __name__ == '__main__':
    main()
