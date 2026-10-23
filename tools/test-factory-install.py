#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Private signed factory/rollback acceptance in an existing candidate userland."""
import argparse
import hashlib
import io
import json
import pathlib
import re
import shlex
import sys
import tarfile
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.local/python-deps'))


def main():
    import paramiko
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('device', 'artifacts', 'updater-artifacts', 'native', 'report'):
        p.add_argument('--'+name, type=pathlib.Path, required=True)
    p.add_argument('--root-directory', required=True)
    a = p.parse_args()
    assert not a.report.exists(), 'Use a new report, never overwrite evidence'
    assert re.fullmatch(r'/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}/rootfs', a.root_directory)
    app = json.loads((a.artifacts/'build.json').read_text())
    tool = json.loads((a.updater_artifacts/'build.json').read_text())
    assert app['development_fixtures'] is False and tool['development_fixtures'] is False
    names = ['rustsimhub-core', 'simhubctl', 'rustsimhub-edge', 'rustsimhub-audio-alsa', 'Opus-COPYING.txt']
    blobs = {name: (a.native if name == names[3] else ROOT/'licenses'/name if name == names[4] else a.artifacts/name).read_bytes() for name in names}
    for name in names[:3]:
        assert hashlib.sha256(blobs[name]).hexdigest() == app['artifact_sha256'][name]
    assert sum(map(len, blobs.values())) < 24*1024*1024
    updater = a.updater_artifacts/'rustsimhub-update'
    assert hashlib.sha256(updater.read_bytes()).hexdigest() == tool['artifact_sha256'][updater.name]
    config = json.loads(a.device.read_text(encoding='utf-8-sig'))
    def connect(c, sock=None):
        s = paramiko.SSHClient(); s.load_host_keys(c['known_hosts']); s.set_missing_host_key_policy(paramiko.RejectPolicy())
        s.connect(c['host'], port=c.get('port', 22), username=c['username'], password=c.get('password'), key_filename=c.get('key_filename'), sock=sock, timeout=12, look_for_keys=False, allow_agent=False)
        return s
    def run(client, body):
        inp, out, err = client.exec_command('sh -s', timeout=180)
        out.channel.set_combine_stderr(True); inp.write('set -eu\numask 077\n'+body+'\n'); inp.channel.shutdown_write()
        raw = out.read().decode(errors='replace')
        if out.channel.recv_exit_status():
            report['failed_output'] = raw
            raise RuntimeError('Acceptance failed; inspect private report, no automatic rerun')
        return raw
    report = {'passed': False, 'new_calls': 0, 'new_sms': 0, 'cellular_tests': False, 'partition_writes': False, 'production_startup_installed': False, 'development_signer': True, 'application_source': app['source_archive_sha256'], 'updater_source': tool['source_archive_sha256']}
    nas = dev = None
    try:
        nas = connect(config['jump'])
        dev = connect(config, nas.get_transport().open_channel('direct-tcpip', (config['host'], config.get('port', 22)), ('127.0.0.1', 0), timeout=12))
        base = run(dev, 'test "$(df -B1 --output=avail /var/tmp | tail -1)" -gt 200000000\nd=$(mktemp -d /var/tmp/rustsimhub-factory-test-XXXXXXXX);chmod 0700 "$d";echo "$d"').strip()
        assert re.fullmatch('/var/tmp/rustsimhub-factory-test-[A-Za-z0-9]{8}', base)
        report['directory'] = base
        # A disposable development signer stays on the workstation, never the device.
        with tempfile.TemporaryDirectory(prefix='factory-sign-', dir=ROOT/'.private') as directory:
            directory = pathlib.Path(directory)
            key = Ed25519PrivateKey.generate()
            public = key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
            with dev.open_sftp() as sftp:
                for name, data in [('release.public', public), ('updater', updater.read_bytes())]:
                    with sftp.open(base+'/'+name, 'wx') as f: f.write(data)
                    sftp.chmod(base+'/'+name, 0o555 if name == 'updater' else 0o600)
                for epoch in (1, 2, 3):
                    m = {'format': 1, 'kind': 'application', 'board': 'ufi003-v02', 'arch': 'aarch64-linux', 'epoch': epoch, 'schema_min': app.get('database_schema',5), 'schema_max': app.get('database_schema',5), 'source_sha256': app['source_archive_sha256'], 'development_fixtures': False, 'files': [{'name': name, 'bytes': len(blobs[name]), 'sha256': hashlib.sha256(blobs[name]).hexdigest()} for name in names]}
                    manifest = json.dumps(m, separators=(',', ':')).encode()
                    archive = directory/f'package{epoch}.tar.gz'
                    with tarfile.open(archive, 'w:gz') as tar:
                        for name, data in {**blobs, 'manifest.json': manifest, 'manifest.sig': key.sign(b'RustSimHub.AppManifest.v1\0'+manifest)}.items():
                            info = tarfile.TarInfo(name); info.size = len(data); info.mode = 0o555 if name in names[:4] else 0o600
                            tar.addfile(info, io.BytesIO(data))
                    sftp.put(str(archive), base+'/'+archive.name)
                    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                    run(dev, 'echo '+shlex.quote(digest+'  '+base+'/'+archive.name)+' | sha256sum -c -')
        run(dev, 'echo '+shlex.quote(tool['artifact_sha256']['rustsimhub-update']+'  '+base+'/updater')+' | sha256sum -c -')
        script = '''set -eu
umask 077
cd /mnt/trial
for e in 1 2 3;do mkdir "package$e";tar -xzf "package$e.tar.gz" -C "package$e";done
./updater --build-info
./package1/rustsimhub-audio-alsa --build-info
mkdir store
u(){ action=$1;shift;./updater "$action" /mnt/trial/store /mnt/trial/release.public "$@"; }
'''
        script += '''
factory=$(u install-factory /mnt/trial/package1)
test "$(u choose)" = "$factory"
u stage /mnt/trial/package2
cp store/state.json before.json
test "$(u install-factory /mnt/trial/package1)" = "$factory"
cmp store/state.json before.json
candidate=$(u choose)
test "$candidate" != "$factory"
cp store/state.json attempted.json
u install-factory /mnt/trial/package1 > /dev/null
cmp store/state.json attempted.json
test "$(u choose)" = "$factory"
u status
if u stage /mnt/trial/package1;then exit 12;fi
u stage /mnt/trial/package3
third=$(u choose)
u confirm "${third##*/}"
test "$(u install-factory /mnt/trial/package1)" = "$third"
cp store/state.json saved.json
mv store/state.json store/state.lost
if u status;then exit 12;fi
cp saved.json store/state.next
u status
cmp store/state.json saved.json
test "$(u choose)" = "$third"
echo FACTORY_INSTALL_UPDATE_FALLBACK_AND_LEDGER_RECOVERY_PASSED
'''
        command = ['systemd-run', '--quiet', '--wait', '--pipe', '--collect', '--expand-environment=no', '--unit=rustsimhub-factory-'+base[-8:]]
        for prop in ['RootDirectory='+a.root_directory, 'MountAPIVFS=yes', 'PrivateDevices=yes', 'PrivateNetwork=yes', 'ProtectSystem=strict', 'PrivateTmp=yes', 'NoNewPrivileges=yes', 'CapabilityBoundingSet=', 'AmbientCapabilities=', 'TemporaryFileSystem=/mnt:mode=0755 /run:mode=0755', 'BindPaths='+base+':/mnt/trial', 'RuntimeMaxSec=150', 'MemoryMax=128M']:
            command += ['-p', prop]
        raw = run(dev, shlex.join(command+['/bin/sh', '-ec', script])); report['private_output'] = raw
        values = [json.loads(line) for line in raw.splitlines() if line.startswith('{')]
        assert values[0]['source_archive_sha256'] == tool['source_archive_sha256']
        assert values[1]['source_archive_sha256'] == app['source_archive_sha256']
        statuses = [v for v in values if 'floor_epoch' in v]
        assert [s['floor_epoch'] for s in statuses] == [2, 3]
        assert statuses[0]['candidate'] is None and statuses[1]['candidate'] is None
        assert 'FACTORY_INSTALL_UPDATE_FALLBACK_AND_LEDGER_RECOVERY_PASSED' in raw
        old = run(dev, 'systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\nfor p in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$p")" = closed;done')
        assert old.splitlines() == ['active']*4
        report['passed'] = True; report['cleanup_verified'] = True
        print('Signed factory install, pending attempt preservation, rollback floor and missing-ledger denial/recovery passed in candidate userland')
    finally:
        a.report.write_text(json.dumps(report, indent=2), encoding='utf-8')
        if dev: dev.close()
        if nas: nas.close()


if __name__ == '__main__':
    main()
