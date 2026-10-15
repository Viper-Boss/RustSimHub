#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stage a verified generic userland for isolated tests, never replacing the system."""
import argparse
import hashlib
import json
import pathlib
import re
import shlex
import sys
import tarfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.local/python-deps'))


def connect(cfg, sock=None):
    import paramiko
    client = paramiko.SSHClient()
    client.load_host_keys(cfg['known_hosts'])
    client.set_missing_host_key_policy(paramiko.RejectPolicy())
    client.connect(cfg['host'], port=cfg.get('port', 22), username=cfg['username'],
                   password=cfg.get('password'), key_filename=cfg.get('key_filename'),
                   sock=sock, timeout=12, look_for_keys=False, allow_agent=False)
    return client


def command(client, body):
    inp, out, err = client.exec_command('sh -s', timeout=240)
    out.channel.set_combine_stderr(True)
    inp.write('set -eu\numask 077\n'+body+'\n')
    inp.channel.shutdown_write()
    raw = out.read().decode(errors='replace')
    if out.channel.recv_exit_status():
        raise RuntimeError('Staging command failed; private diagnostics: '+raw)
    return raw


def check_archive(path):
    with tarfile.open(path) as tar:
        entries = tar.getmembers()
        links = {pathlib.PurePosixPath(m.name) for m in entries if m.issym()}
        seen = set()
        for entry in entries:
            name = pathlib.PurePosixPath(entry.name)
            assert not name.is_absolute() and '..' not in name.parts
            assert name not in seen, 'Duplicate archive entry'
            seen.add(name)
            assert entry.isfile() or entry.isdir() or entry.issym() or entry.islnk()
            assert not any(parent in links for parent in name.parents)
            if entry.islnk():
                target = pathlib.PurePosixPath(entry.linkname)
                assert not target.is_absolute() and '..' not in target.parts
                assert target not in links and not any(parent in links for parent in target.parents)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('device', 'archive', 'metadata', 'report'):
        p.add_argument('--'+name, type=pathlib.Path, required=True)
    args = p.parse_args()
    assert not args.report.exists(), 'Inspect existing attempt instead of restarting'
    meta = json.loads(args.metadata.read_text())
    assert meta['flash_allowed'] is False and re.fullmatch('[a-f0-9]{64}', meta['sha256'])
    assert args.archive.stat().st_size == meta['bytes']
    h = hashlib.sha256()
    with args.archive.open('rb') as f:
        for block in iter(lambda: f.read(4*1024*1024), b''):
            h.update(block)
    assert h.hexdigest() == meta['sha256']
    # No extracted member may traverse another member's symlink. Absolute
    # symlink targets are rootfs-relative at runtime; never extract through them.
    check_archive(args.archive)
    cfg = json.loads(args.device.read_text(encoding='utf-8-sig'))
    jump = client = None
    report = {'passed': False, 'phase': 'preflight', 'partition_writes': False,
              'production_units_installed': False, 'archive_sha256': meta['sha256'],
              'tree_sha256': meta['tree_sha256'], 'bytes': meta['bytes']}
    def save():
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2), encoding='utf-8')
    save()
    try:
        jump = connect(cfg['jump'])
        sock = jump.get_transport().open_channel('direct-tcpip',
                (cfg['host'], cfg.get('port', 22)), ('127.0.0.1', 0), timeout=12)
        client = connect(cfg, sock)
        raw = command(client, 'test "$(id -u)" = 0\ndf -B1 --output=avail /var/tmp | tail -n 1')
        free = int(raw.splitlines()[-1])
        assert free > meta['bytes']+700*1024*1024, 'Keep storage reserve; do not delete old evidence'
        folder = command(client, 'mktemp -d /var/tmp/rustsimhub-system-tree-XXXXXXXX').strip()
        assert re.fullmatch('/var/tmp/rustsimhub-system-tree-[A-Za-z0-9]{8}', folder)
        report.update(directory=folder, phase='uploading', free_before=free)
        save()
        with client.open_sftp() as sf:
            sf.get_channel().settimeout(60)
            with sf.open(folder+'/tree.tar.gz.partial', 'wx') as dst, args.archive.open('rb') as src:
                sf.chmod(folder+'/tree.tar.gz.partial', 0o600)
                dst.set_pipelined(True)
                total = 0
                for block in iter(lambda: src.read(1024*1024), b''):
                    dst.write(block)
                    total += len(block)
                    if total % (32*1024*1024) == 0:
                        report['uploaded_bytes'] = total
                        save()
                        print('Private userland upload MiB:', total//1024//1024, flush=True)
            assert sf.stat(folder+'/tree.tar.gz.partial').st_size == meta['bytes']
        report['uploaded_bytes'] = total
        q = shlex.quote
        command(client, 'echo '+q(meta['sha256']+'  '+folder+'/tree.tar.gz.partial')+
                ' | sha256sum -c -\nmv -nT '+q(folder+'/tree.tar.gz.partial')+' '+q(folder+'/tree.tar.gz')+
                '\ntest ! -e '+q(folder+'/tree.tar.gz.partial'))
        report['phase'] = 'extracting_and_comparing'
        save()
        command(client, 'mkdir -m 0755 '+q(folder+'/rootfs')+'\n'
                'tar --numeric-owner --same-owner --xattrs --xattrs-include=security.capability '
                '-xpf '+q(folder+'/tree.tar.gz')+' -C '+q(folder+'/rootfs')+'\n'
                'tar --numeric-owner --xattrs --xattrs-include=security.capability '
                '-df '+q(folder+'/tree.tar.gz')+' -C '+q(folder+'/rootfs'))
        candidate = json.loads(command(client, 'cat '+q(folder+'/rootfs/usr/share/rustsimhub/system-candidate.json')))
        assert candidate['composition_revision'] == meta['composition_revision']
        assert candidate['flash_allowed'] is False and candidate['private_data_included'] is False
        raw = command(client, 'systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs\n'
                'for file in /proc/asound/card*/pcm*/sub*/status;do test "$(cat "$file")" = closed;done')
        assert raw.splitlines() == ['active']*4
        report.update(passed=True, phase='ready', root_directory=folder+'/rootfs',
                      composition_revision=candidate['composition_revision'],
                      application_revision=candidate['application_revision'],
                      application_source_sha256=candidate['application_source_sha256'],
                      old_services_and_pcm_preserved=True)
        print('Verified userland staged and archive compared; old system unchanged; no flashing', flush=True)
    except Exception as error:
        report['failure'] = str(error)
        raise
    finally:
        save()
        if client:
            client.close()
        if jump:
            jump.close()


if __name__ == '__main__':
    main()
