# SPDX-License-Identifier: AGPL-3.0-only
"""C25: fixed-role clean rootfs material composition, never installation."""
import struct
from .common import ROOT, Directory, MAX_FILE, MAX_TOTAL, canonical, digest, exact, hexadecimal, need, relative, write_bundle, require_completed

# Deliberately finite. Core/edge/audio applications belong to the signed app slot,
# not this base image. No caller-selected destination or credential material.
ROLES = {
    'core-launcher': ('usr/libexec/rustsimhub/rustsimhub-core-launcher', 0o755, 'elf'),
    'updater': ('usr/libexec/rustsimhub/rustsimhub-update', 0o755, 'elf'),
    'setup-ap': ('usr/libexec/rustsimhub/rustsimhub-setup-ap', 0o755, 'elf'),
}
CONFIGS = {
    'rustsimhub-setup-ap.service': 'etc/systemd/system/rustsimhub-setup-ap.service',
    'rustsimhub-setup-edge.service': 'etc/systemd/system/rustsimhub-setup-edge.service',
    'rustsimhub-setup-ap.tmpfiles': 'etc/tmpfiles.d/rustsimhub-setup-ap.conf',
    '50-setup-ap-core.conf': 'etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf',
    '50-setup-ap-target.conf': 'etc/systemd/system/rustsimhub-app.target.d/50-setup-ap.conf',
    'contract.json': 'etc/rustsimhub/setup-ap-contract.json',
}
FORWARDING = {
    'rustsimhub-forwarder.service': ('etc/systemd/system/rustsimhub-forwarder.service', 0o644),
    'rustsimhub-forwarder-reconcile.service': ('etc/systemd/system/rustsimhub-forwarder-reconcile.service', 0o644),
    'rustsimhub-forwarder-status.service': ('etc/systemd/system/rustsimhub-forwarder-status.service', 0o644),
    'rustsimhub-forwarder-boot.service': ('etc/systemd/system/rustsimhub-forwarder-boot.service', 0o644),
    '46-rustsimhub-forwarding.rules': ('etc/polkit-1/rules.d/46-rustsimhub-forwarding.rules', 0o644),
    'forwarder.json': ('etc/rustsimhub/forwarder.json', 0o600),
    'tmpfiles.conf': ('etc/tmpfiles.d/rustsimhub-forwarding.conf', 0o644),
    '20-forwarding.conf': ('etc/systemd/system/rustsimhub-app.target.d/20-forwarding.conf', 0o644),
}
BASE = 'firmware/system/overlay/etc/systemd/system/rustsimhub-core.service'
CONTRACT_PATHS = ('crates/update/src/lib.rs', 'crates/update/src/admission.rs', 'crates/core-launcher/src/lib.rs', BASE) + tuple('firmware/system/setup-ap/' + name for name in CONFIGS) + tuple('firmware/system/forwarding/' + name for name in FORWARDING)


def fixed_files():
    """Exact existing SetupAp profile includes the complete forwarding set."""
    with Directory(ROOT) as repo:
        text = lambda p: repo.read(p, 65536).replace(b'\r\n', b'\n')
        files = {target: (text('firmware/system/setup-ap/' + name), 0o644) for name, target in CONFIGS.items()}
        files.update({target: (text('firmware/system/forwarding/' + name), mode) for name, (target, mode) in FORWARDING.items()})
        base = text(BASE)
        commands = [line[len(b'ExecStart='):] for line in base.splitlines() if line.startswith(b'ExecStart=')]
        need(len(commands) == 1 and b'--serve-device ' in commands[0], 'base_command')
        command = commands[0].replace(b'--serve-device ', b'--serve-device-forwarding-managed ')
        files['etc/systemd/system/rustsimhub-core.service'] = (base, 0o644)
        files['etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf'] = (b'# SPDX-License-Identifier: AGPL-3.0-only\n[Service]\nExecStart=\nExecStart=' + command + b' 983\n', 0o644)
        return files


def contracts():
    """Pin actual UPD-01 implementations; a snapshot JSON is never a lock."""
    with Directory(ROOT) as source:
        return {p: digest(source.read(p, 2 * 1024 * 1024)) for p in CONTRACT_PATHS}


def validate(manifest, source):
    exact(manifest, ('schema', 'board', 'arch', 'contracts', 'files'))
    need(type(manifest['schema']) is int and manifest['schema'] == 1, 'schema')
    need(manifest['board'] == 'ufi003-v02' and manifest['arch'] == 'aarch64', 'platform')
    need(manifest['contracts'] == contracts(), 'contract_revision')
    entries = manifest['files']
    need(type(entries) is list and len(entries) == len(ROLES), 'role_count')
    seen, inputs, files, observations, total = set(), set(), [], [], 0
    for entry in entries:
        exact(entry, ('role', 'path', 'sha256'))
        role = entry['role']
        need(isinstance(role, str) and role in ROLES and role not in seen, 'role')
        seen.add(role)
        relative(entry['path'])
        need(entry['path'] not in inputs, 'duplicate_input')
        inputs.add(entry['path'])
        hexadecimal(entry['sha256'])
        data = source.read(entry['path'], MAX_FILE)
        total += len(data)
        need(total <= MAX_TOTAL and digest(data) == entry['sha256'], 'integrity')
        # Header observations only. Full loader/admission checks remain UPD-01.
        need(len(data) >= 64 and data[:6] == b'\x7fELF\x02\x01', 'elf')
        need(data[6] == 1 and struct.unpack_from('<H', data, 52)[0] == 64, 'elf_version')
        need(struct.unpack_from('<H', data, 18)[0] == 183, 'elf_machine')
        need(struct.unpack_from('<H', data, 16)[0] in (2, 3), 'elf_type')
        target, mode, _ = ROLES[role]
        files.append(('rootfs/' + target, data, mode))
        observations.append({'role': role, 'path': target, 'mode': format(mode, '04o'), 'desired_uid': 0, 'desired_gid': 0, 'bytes': len(data), 'sha256': digest(data)})
    observations.sort(key=lambda e: e['role'])
    configs = fixed_files()
    for target, (data, mode) in configs.items():
        files.append(('rootfs/' + target, data, mode))
    report = {'schema': 1, 'kind': 'rootfs-material-candidate', 'files': observations,
              'configs': {target: {'sha256': digest(data), 'mode': format(mode, '04o')} for target, (data, mode) in configs.items()},
              'contracts': manifest['contracts'], 'authorization_verified': False,
              'installed': False, 'complete_bootable_rootfs': False}
    report['content_sha256'] = digest(canonical(report))
    return sorted(files), report


def compose(manifest, source, output):
    files, report = validate(manifest, source)
    return write_bundle(output, files, report)


def compare(left, right):
    """Compare every material byte, not only caller-provided report digests."""
    from .common import parse
    reports = [parse(d.read('report.json', 256 * 1024)) for d in (left, right)]
    for directory in (left, right):
        require_completed(directory)
    need(reports[0] == reports[1], 'reproducibility')
    report = reports[0]
    need(report.get('kind') == 'rootfs-material-candidate', 'report')
    need(report.get('contracts') == contracts(), 'contract_revision')
    unhashed = dict(report)
    expected_content = unhashed.pop('content_sha256', None)
    need(digest(canonical(unhashed)) == expected_content, 'report_integrity')
    need(len(report.get('files', [])) == len(ROLES), 'role_count')
    seen = set()
    for entry in report['files']:
        role = entry['role']
        need(role in ROLES and role not in seen, 'role')
        seen.add(role)
        path, mode, _ = ROLES[role]
        need(entry['path'] == path and entry['mode'] == format(mode, '04o'), 'target')
        for directory in (left, right):
            data = directory.read('rootfs/' + path, MAX_FILE, mode)
            need(len(data) == entry['bytes'] and digest(data) == entry['sha256'], 'integrity')
    configs = fixed_files()
    need(set(report.get('configs', {})) == set(configs), 'configs')
    for path, expected in report['configs'].items():
        fixed_data, mode = configs[path]
        need(expected['mode'] == format(mode, '04o'), 'mode')
        need(expected['sha256'] == digest(fixed_data), 'config_source')
        for directory in (left, right):
            need(digest(directory.read('rootfs/' + path, 65536, mode)) == expected['sha256'], 'integrity')
    return {'reproducible_material_bytes': True, 'content_sha256': report['content_sha256'], 'authorization_verified': False}
