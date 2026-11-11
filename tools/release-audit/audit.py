#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Read-only release preflight. Never executes a candidate, installs or accesses a device."""
import argparse
import base64
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import struct
import zlib

from safeio import Refused, Tree, canonical, hex_id, need, parse, read_file, relative, same, sha
from contracts import APP_FILES, PROFILES, check_report, material_sources
import public
import root_materials

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
import arm_build_manifest as arm
import setup_ap_package as ap

SYS_KINDS = {'boot', 'modules', 'dtb', 'rootfs', 'firmware_policy', 'identity_policy', 'default_off'}
LIMIT = 64 * 1024 * 1024
# Fixed copied overlay inputs, not candidate-selected source paths.
STAGED = ('usr/local/libexec/rustsimhub-firstboot', 'usr/local/libexec/rustsimhub-generic-install',
 'usr/local/libexec/rustsimhub-install-initialize', 'usr/local/libexec/rustsimhub-firmware-extract',
 'etc/systemd/system/rustsimhub-firstboot.service', 'etc/systemd/system/rustsimhub-generic-install.service',
 'etc/systemd/system/rustsimhub-firmware-extract.service', 'etc/systemd/system/rustsimhub-usb.service',
 'etc/systemd/system/rustsimhub-app-select.service', 'etc/systemd/system/rustsimhub-core.service',
 'etc/systemd/system/rustsimhub-ssh-prepare.service')


def kernel_contract(raw, expected):
    # Android boot v0 fixed header; bounded gzip expansion, no candidate execution.
    need(len(raw) >= 1632 and raw[:8] == b'ANDROID!', 'boot_contract')
    size = int.from_bytes(raw[8:12], 'little'); page = int.from_bytes(raw[36:40], 'little')
    need(page in (2048,4096,8192,16384) and 0 < size <= LIMIT and page + size <= len(raw), 'boot_header')
    data = raw[page:page+size]
    if data.startswith(b'\x1f\x8b'):
        stream = zlib.decompressobj(31)
        data = stream.decompress(data, 128*1024*1024 + 1)
        need(len(data) <= 128*1024*1024 and stream.eof, 'kernel_expansion_budget')
    versions = set(re.findall(rb'Linux version (\d+\.\d+[^\s\x00]{0,120}) ',data))
    need(versions == {expected.encode('ascii')}, 'kernel_version')



def git_blob(repo, revision, name):
    relative(name)
    env = dict(os.environ, GIT_NO_REPLACE_OBJECTS='1', GIT_TERMINAL_PROMPT='0')
    result = subprocess.run(['git', '--no-replace-objects', '-C', str(repo), 'show', revision + ':' + name],
                            env=env, stdin=subprocess.DEVNULL, capture_output=True, timeout=15)
    need(result.returncode == 0 and len(result.stdout) <= 8 * 1024 * 1024, 'source_blob_unavailable')
    return result.stdout


def source_archive(repo, archive, revision, expected):
    need(__debug__, 'ordinary_python_required_for_original_verifier')
    need(hex_id(revision, 40) and hex_id(expected, 64), 'source_identifiers')
    raw = read_file(archive, LIMIT)
    need(sha(raw) == expected, 'source_archive_digest')
    # Original verifier checks every committed blob and rejects omitted,
    # extra, linked and unsafe archive entries. A private bounded snapshot
    # prevents its path-based API from reopening a changing candidate path.
    with tempfile.TemporaryDirectory(prefix='rsh-readonly-audit-') as folder:
        snapshot = Path(folder) / 'source.tar'
        snapshot.write_bytes(raw)
        snapshot.chmod(0o400)
        previous = os.environ.get('GIT_NO_REPLACE_OBJECTS')
        os.environ['GIT_NO_REPLACE_OBJECTS'] = '1'
        try:
            count = arm.verify_source(snapshot, repo, revision, expected)
        finally:
            if previous is None:
                os.environ.pop('GIT_NO_REPLACE_OBJECTS', None)
            else:
                os.environ['GIT_NO_REPLACE_OBJECTS'] = previous
    return {'committed_blobs': count, 'archive_sha256': expected}


def program_version(raw):
    # Reuse the existing device glibc check on the already-checked bytes,
    # never execute the candidate or follow its name after validation.
    with tempfile.TemporaryDirectory(prefix='rsh-elf-audit-') as folder:
        path = Path(folder) / 'program'
        path.write_bytes(raw)
        path.chmod(0o400)
        return arm.versions(path)


def materials(tree, report, repo, revision, flags):
    product, _, recovery, health = flags
    expected = material_sources(product, recovery, health)
    need(set(report['materials']) == set(expected) | {'product-profile.json'}, 'material_inventory_finite')
    need(set(tree.names('materials')) == set(report['materials']), 'material_directory_inventory')
    roles = report['plan']['roles']
    substitutions = {'@CORE_MEMORY@': '192M' if product else '96M',
                     '@CORE_TASKS@': '64' if product else '16',
                     '@NKN_GID@': str(roles.get('nkn', {}).get('gid', 0)),
                     '@BLE_GID@': str(roles.get('ble', {}).get('gid', 0))}
    for name, source in expected.items():
        original = git_blob(repo, revision, source).replace(b'\r\n', b'\n')
        for before, after in substitutions.items():
            original = original.replace(before.encode(), after.encode())
        raw = tree.read('materials/' + name, 256 * 1024, mode=0o444)
        public.inspect(name,raw)
        need(raw == original and report['materials'][name] == sha(raw), 'fixed_material_bytes')
    config_raw = tree.read('materials/product-profile.json', 4096, mode=0o444)
    need(same(parse(config_raw), {'format': 1, 'profile': report['profile'],
                                 'source_archive_sha256': report['source_archive_sha256'],
                                 'core_sha256': report['programs']['rustsimhub-core']['sha256']})
         and report['materials']['product-profile.json'] == sha(config_raw), 'installed_profile_binding')
    return {'fixed_materials_checked': len(expected) + 1, 'starts_services': False}


def licenses(tree, report, repo, revision, product):
    entries = report.get('licenses')
    need(type(entries) is dict and 'licenses/PROJECT-LICENSE' in entries
         and 'licenses/Opus-COPYING.txt' in entries and 'licenses/GPL-2.0-only.txt' in entries,
         'license_inventory_missing')
    actual = {}
    for name, expected in entries.items():
        relative(name)
        need(name.startswith(('licenses/', 'sdk-licenses/')) and hex_id(expected, 64), 'license_path')
        # Original producer copies license text, preserving 0644 (and 0600
        # for its freshly copied project LICENSE under a private umask).
        raw = tree.read(name, 2 * 1024 * 1024, allowed_modes=(0o444, 0o644, 0o600))
        public.inspect(name,raw)
        need(sha(raw) == expected, 'license_bytes')
        actual[name] = raw
        if name.startswith('licenses/'):
            source = 'LICENSE' if name == 'licenses/PROJECT-LICENSE' else name
            need(raw == git_blob(repo, revision, source), 'source_license_mismatch')
    need(set(tree.names('licenses')) == {n.split('/', 1)[1] for n in entries if n.startswith('licenses/')},
         'license_directory_inventory')
    if not product:
        need(report.get('sdk_licenses') is None, 'unexpected_sdk')
        return {'source_licenses_checked': len(entries), 'go_modules': 0}
    modules = tree.json('sdk-licenses/modules.json', 256 * 1024)
    need(same(modules, report.get('sdk_licenses')) and modules.get('no_new_external_modules') is True
         and modules.get('dependency_approval') is False and modules.get('go') == 'go1.27.3',
         'sdk_license_contract')
    baseline = parse(git_blob(repo, revision, 'docs/dependencies/nkn-websocket-mask-candidate.json'))
    known = {m['path']: m for m in baseline['compiled_modules']}
    own = 'github.com/Viper-Boss/RustSimHub/tools/nkn-engine'
    rows = modules.get('modules')
    need(type(rows) is list and 0 < len(rows) <= 100, 'sdk_modules')
    selected = {row['path']: row for row in rows}
    need(len(selected) == len(rows) and own in selected and set(selected) <= set(known) | {own},
         'sdk_module_closure')
    expected_sections = {}
    for name, row in selected.items():
        if name == own:
            need(row['version'] == 'v0.0.0', 'sdk_own_version')
            files = [{'path': 'LICENSE', 'sha256': sha(git_blob(repo, revision, 'LICENSE'))}]
        else:
            need(row['version'] == known[name]['version'], 'sdk_module_version')
            files = known[name]['license_files']
        need(same(row.get('license_files'), files) and files, 'sdk_license_manifest')
        for part in files:
            relative(part['path'])
            expected_sections[name + '@' + row['version'] + ': ' + part['path']] = part['sha256']
    # Original notices format, exact individual license bytes including their
    # original newline. Do not merely trust the candidate's bundle digest.
    notice = actual.get('sdk-licenses/THIRD-PARTY-NOTICES.txt')
    need(type(notice) is bytes, 'sdk_notice_missing')
    matches = list(re.finditer(rb'\n===== ([^\r\n]{1,512}) =====\n', notice))
    need(len(matches) == len(expected_sections) + 1 and matches[0].start() == 0, 'sdk_notice_sections')
    found = set()
    for i, match in enumerate(matches):
        title = match[1].decode('utf8')
        content = notice[match.end():matches[i + 1].start() if i + 1 < len(matches) else len(notice)]
        need(title not in found, 'sdk_notice_duplicate')
        found.add(title)
        if title == 'Go runtime go1.27.3':
            need(i == len(matches) - 1 and sha(content) == modules['runtime_license_sha256'],
                 'go_runtime_license')
        else:
            need(title in expected_sections and content.endswith(b'\n')
                 and sha(content[:-1]) == expected_sections[title], 'sdk_notice_license_bytes')
    need(found == set(expected_sections) | {'Go runtime go1.27.3'}, 'sdk_notice_complete')
    provenance = report.get('sdk_provenance', {})
    need(provenance.get('revision') == revision and report.get('sdk', {}).get('source_commit') == revision
         and report['sdk'].get('isolated_fixture_build') is False
         and report['sdk'].get('go') == 'go1.27.3', 'go_source_commit')
    patch = parse(git_blob(repo, revision, 'tools/nkn-build/sdk-managed/patch-manifest.json'))
    need(provenance.get('managed_patch_sha256') == patch['patch_sha256'], 'go_managed_patch')
    return {'source_licenses_checked': len(entries), 'go_modules': len(rows),
            'dependency_approval': False, 'implementation': 'original Go, not Rust'}


def profile_audit(args):
    with Tree(args.build_root) as tree:
        report = tree.json('profile-build.json', 512 * 1024)
        source, revision = report.get('source_archive_sha256'), report.get('revision')
        need(revision == args.source and hex_id(source, 64), 'expected_source_commit')
        source_result = source_archive(args.repository, args.archive, revision, source)
        flags = check_report(report, args.profile, source, revision, arm, ap.CONTRACT)
        need(set(tree.names('programs')) == set(report['programs']), 'program_directory_inventory')
        checked = {}
        for name, row in report['programs'].items():
            relative(name)
            need('/' not in name and type(row) is dict and type(row.get('bytes')) is int
                 and hex_id(row.get('sha256'), 64), 'program_entry')
            raw = tree.read('programs/' + name, 32 * 1024 * 1024, mode=0o555)
            arm.elf(raw)
            public.inspect(name,raw)
            if name!='rustsimhub-ble-recovery':need(source.encode() in raw,'program_source_marker')
            need(len(raw) == row['bytes'] and sha(raw) == row['sha256'], 'program_bytes')
            need(same(program_version(raw), row.get('glibc')), 'program_glibc_versions')
            checked[name] = {'bytes': len(raw), 'sha256': sha(raw)}
        app = tree.json('application-input.json', 16384)
        need(type(app.get('format')) is int and app['format'] == 1
             and app.get('kind') == 'unsigned-application-input'
             and app.get('source_archive_sha256') == source and app.get('profile') == args.profile
             and app.get('signed') is False and app.get('installable') is False
             and app.get('epoch_assigned') is False and set(app.get('files', {})) == set(APP_FILES),
             'application_input_contract')
        need(app.get('schema_min') == 6 and type(app['schema_min']) is int
             and app.get('schema_max') == (9 if flags[2] else 6) and type(app['schema_max']) is int,
             'application_schema')
        total = 0
        for name in APP_FILES:
            raw = tree.read('application-input/' + name, 24 * 1024 * 1024)
            expected = app['files'][name]
            need(type(expected.get('bytes')) is int and len(raw) == expected['bytes']
                 and sha(raw) == expected.get('sha256'), 'application_payload')
            if name != 'Opus-COPYING.txt':
                need(same(expected, checked[name]), 'application_program_cross_binding')
            else:
                need(raw == git_blob(args.repository, revision, 'licenses/Opus-COPYING.txt'),
                     'application_license')
            total += len(raw)
        need(total <= 24 * 1024 * 1024 and type(app.get('payload_bytes')) is int
             and total == app['payload_bytes'], 'application_budget')
        material_result = materials(tree, report, args.repository, revision, flags)
        license_result = licenses(tree, report, args.repository, revision, flags[0])
        if flags[0]:
            need(args.roles_policy is not None, 'independent_roles_policy_required')
            need(not Path(args.roles_policy).resolve().is_relative_to(Path(args.build_root).resolve()),
                 'package_cannot_approve_roles')
            policy = parse(read_file(args.roles_policy, 8192))
            need(same(policy, {'format': 1, 'roles': {role: report['plan']['roles'][role]
                                                   for role in ('nkn', 'ble')}}), 'independent_roles_policy')
        return {'source_commit': revision, **source_result, 'profile': args.profile,
                'programs': checked, 'application_bytes': total, 'materials': material_result,
                'licenses': license_result, 'default_off_checked': True,
                'program_origin_attested': False, 'installed_nss_verified': False,
                'signature_verified': False,'source_markers_checked':len(checked)-int('rustsimhub-ble-recovery' in checked),
                'public_input_secret_scan':'bounded programs, source-fixed materials and licenses; no secrecy attestation'}


def signed_application(args, component):
    need(args.signed_application is not None and args.release_public is not None and
         hex_id(args.manifest_sha256,64),'signed_application_external_inputs_required')
    from application_package import verify_package
    key=read_file(args.release_public,32)
    need(len(key)==32,'release_public_length')
    with Tree(args.signed_application) as tree, tempfile.TemporaryDirectory(prefix='rsh-app-audit-') as scratch:
        target=Path(scratch)
        # Top-level is exactly the original six files plus manifest/signature.
        need(set(os.listdir(tree.fd))==set(APP_FILES)|{'manifest.json','manifest.sig'},'signed_app_inventory')
        for name in (*APP_FILES,'manifest.json','manifest.sig'):
            raw=tree.read(name,24*1024*1024 if name in APP_FILES else 131072)
            if name in component['programs']:
                need(sha(raw)==component['programs'][name]['sha256'],'signed_app_program_mixed')
            if name=='manifest.json':need(sha(raw)==args.manifest_sha256,'external_manifest_digest')
            (target/name).write_bytes(raw)
        public_path=target/'external.public';public_path.write_bytes(key)
        value=verify_package(target,public_path,component['archive_sha256'],schema=6)
        need(type(value['format']) is int and value['format']==2 and value['schema_min']==6 and
             value['schema_max']==(9 if PROFILES[args.profile][2] else 6),'signed_app_profile')
    return {'signature_verified':True,'external_public_sha256':sha(key),
            'manifest_sha256':args.manifest_sha256,'signing_authorization':False}

def _large_file(tree, name, expected, limit, full=False):
    """Large ext4 images are hashed in bounded chunks, without mounting them."""
    relative(name)
    need(type(expected) is dict and type(expected.get('bytes')) is int
         and 0 < expected['bytes'] <= limit and hex_id(expected.get('sha256'), 64), 'system_file_entry')
    # Tree.read remains appropriate for boot/modules/DTB/policy. Rootfs is
    # explicitly bounded by its existing image contract (under four GiB).
    parts = relative(name)
    parent = tree.path.joinpath(*parts[:-1])
    with Tree(parent) as directory:
        fd = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                     dir_fd=directory.fd)
        try:
            before = os.fstat(fd)
            from safeio import stamp
            need(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                 and not before.st_mode & 0o022 and before.st_size == expected['bytes'], 'system_file_size')
            digest = hashlib.sha256()
            head = bytearray()
            count = 0
            deadline = time.monotonic()+120
            while True:
                need(time.monotonic() <= deadline, 'system_read_deadline')
                block = os.read(fd, 1024 * 1024)
                if not block:
                    break
                count += len(block)
                need(count <= limit, 'system_file_budget')
                digest.update(block)
                maximum = LIMIT if full else 65536
                if len(head) < maximum:
                    head.extend(block[:maximum - len(head)])
            need(count == before.st_size and digest.hexdigest() == expected['sha256']
                 and stamp(before) == stamp(os.fstat(fd)), 'system_file_digest_or_change')
            need(stamp(os.stat(parts[-1], dir_fd=directory.fd, follow_symlinks=False)) == stamp(before),
                 'system_file_replaced')
            return bytes(head)
        finally:
            os.close(fd)


def system_audit(args, component):
    need(args.system_index is not None, 'system_index_missing')
    index = parse(read_file(args.system_index, 128 * 1024))
    need(type(index) is dict and set(index) == {'format', 'kind', 'source_commit', 'source_archive_sha256',
        'profile', 'artifacts', 'public_files', 'maintenance_trust', 'staged_files'}
         and type(index['format']) is int and index['format'] == 1
         and index['kind'] == 'web5-system-audit-v1', 'system_contract_unknown')
    need(index['source_commit'] == component['source_commit']
         and index['source_archive_sha256'] == component['archive_sha256']
         and index['profile'] == args.profile, 'system_mixed_source')
    need(type(index['artifacts']) is dict and set(index['artifacts']) == SYS_KINDS, 'system_required_materials')
    lock = parse(git_blob(args.repository, args.source, 'firmware/base-system.lock.json'))['kernel']
    with Tree(args.system_root) as tree:
        files = index['artifacts']
        need(files['boot']['sha256'] == lock['boot_img_sha256']
             and files['modules']['sha256'] == lock['modules_sha256'], 'locked_kernel_inputs')
        data = {}
        for role in sorted(SYS_KINDS):
            row = files[role]
            need(type(row) is dict and set(row) == {'path', 'bytes', 'sha256'}, 'system_artifact_fields')
            data[role] = _large_file(tree, row['path'], row, 4 * 1024**3 if role == 'rootfs' else LIMIT, full=role=='boot')
        kernel_contract(data['boot'],lock['release'])
        need(data['dtb'].startswith(bytes.fromhex('d00dfeed'))
             and int.from_bytes(data['dtb'][4:8], 'big') == files['dtb']['bytes'], 'dtb_contract')
        need(len(data['rootfs']) > 1082 and data['rootfs'][1080:1082] == b'\x53\xef', 'rootfs_ext4')
        firmware = parse(tree.read(files['firmware_policy']['path'], 32768))
        need(same(firmware, {'format': 1, 'kind': 'extract-own-firmware-v1', 'extract_at_first_boot': True,
                             'contains_device_firmware': False, 'source_commit': args.source}), 'firmware_policy')
        identity = parse(tree.read(files['identity_policy']['path'], 32768))
        need(same(identity, {'format': 1, 'kind': 'generic-identity-initialization-v1',
                             'generate_on_device': True, 'identity_in_image': False, 'fixed_owner_password': False,
                             'source_commit': args.source}), 'identity_policy')
        need(same(parse(tree.read(files['default_off']['path'], 32768)),
                  {'format': 1, 'forwarding': False, 'nkn': False, 'ble': False,
                   'cellular_data': False, 'source_commit': args.source}), 'system_default_off')
        need(type(index['staged_files']) is dict and set(index['staged_files']) == set(STAGED),
             'staged_source_inventory')
        for name,expected in index['staged_files'].items():
            raw = tree.read('staged/'+name, 256*1024, mode=0o755 if name.startswith('usr/local/libexec/') else 0o644)
            need(hex_id(expected,64) and sha(raw)==expected and
                 raw == git_blob(args.repository,args.source,'firmware/system/overlay/'+name), 'staged_source_bytes')
        trust = index['maintenance_trust']
        need(type(trust) is dict and set(trust) == {'included', 'public_key'}
             and type(trust['included']) is bool, 'maintenance_trust_contract')
        if trust['included']:
            need(args.distribution == 'self-use', 'maintenance_trust_public_distribution')
            key = tree.read(trust['public_key'], 1024)
            need(re.fullmatch(rb'ssh-ed25519 [A-Za-z0-9+/=]{20,}( [\x20-\x7e]{1,128})?\n', key),
                 'maintenance_public_key')
            encoded=key.split()[1]
            blob=base64.b64decode(encoded,validate=True)
            need(len(blob)==51 and blob[:19]==b'\0\0\0\x0bssh-ed25519\0\0\0\x20','maintenance_public_wire')
            public.inspect(trust['public_key'], key)
        else:
            need(trust['public_key'] is None, 'maintenance_trust_contract')
        public_files = index['public_files']
        need(type(public_files) is list and len(public_files) <= 10000
             and len(set(public_files)) == len(public_files), 'public_scan_inventory')
        opaque={files[k]['path'] for k in ('boot','modules','dtb','rootfs')}
        names=set()
        visited=0
        def inventory(directory=''):
            nonlocal visited
            need(len(Path(directory).parts)<=32,'public_depth_budget')
            with Tree(tree.path/directory) as current:
                entries=os.listdir(current.fd)
                need(len(entries)<=10000,'public_inventory_budget')
                for name in entries:
                    visited+=1;need(visited<=10000,'public_inventory_budget')
                    relative(name)
                    need(name not in ('.private','.ssh','.gnupg','.aws'),'private_directory_refused')
                    path=(directory+'/' if directory else '')+name
                    info=os.stat(name,dir_fd=current.fd,follow_symlinks=False)
                    if stat.S_ISDIR(info.st_mode):inventory(path)
                    else:
                        need(stat.S_ISREG(info.st_mode) and info.st_nlink==1,'public_node_type')
                        names.add(path);need(len(names)<=10000,'public_inventory_budget')
        inventory()
        need(set(public_files)==names-opaque,'public_scan_coverage')
        for name in public_files:
            public.inspect(name, tree.read(name, 32 * 1024 * 1024))
    return {'input_set_complete': True, 'kernel_release': lock['release'],
            'public_files_examined': len(public_files), 'distribution': args.distribution,
            'maintenance_public_trust_included': trust['included'], 'kernel_boot_verified': False,
            'identity_ready_verified': False, 'rootfs_mounted': False, 'staged_tree_matches_image':False,
            'public_secrecy_proven':False, 'opaque_inputs_not_secret_scanned':['boot','modules','dtb','rootfs'],
            'staged_source_files_checked':len(STAGED)}


def audit(args):
    report = {'format': 1, 'kind': 'web5-readonly-release-audit-v1', 'scope': args.scope,
              'audit_passed': False, 'release_input_complete': False, 'read_only': True,
              'authorization_verified': False, 'installable': False, 'may_sign': False,
              'health_verified': False, 'may_confirm': False, 'device_access': False,
              'starts_services': False, 'flash_allowed': False, 'findings': []}
    stage = 'profile'
    try:
        need(args.scope in ('component', 'release'), 'audit_scope')
        profile = profile_audit(args)
        report['component'] = profile
        if args.signed_application is not None or args.scope=='release':
            stage='signed_application'
            report['signed_application']=signed_application(args,profile)
        if args.root_materials is not None or args.scope=='release':
            stage='root_materials'
            need('signed_application' in report,'root_requires_signed_application')
            report['root_materials']=root_materials.verify(args,profile,report['signed_application'],git_blob,PROFILES[args.profile])
        if args.scope == 'release':
            stage = 'system'
            report['system'] = system_audit(args, profile)
            report['release_input_complete'] = True
        report['audit_passed'] = True
    except Refused as error:
        report['findings'].append({'check': stage, 'code': str(error)})
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError, AssertionError, tarfile.TarError):
        # Never echo paths, malformed candidate text, key bodies or subprocess stderr.
        report['findings'].append({'check': stage, 'code': 'input_or_tool_refused'})
    if args.scope == 'release' and not report['release_input_complete']:
        report['unverified_required_system_layers'] = sorted(SYS_KINDS)
    return report


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repository', type=Path, required=True)
    p.add_argument('--source', required=True, help='full immutable product commit, not a branch')
    p.add_argument('--archive', type=Path, required=True)
    p.add_argument('--build-root', type=Path, required=True)
    p.add_argument('--profile', choices=tuple(PROFILES), required=True)
    p.add_argument('--roles-policy', type=Path, help='independently held declaration, outside the candidate')
    p.add_argument('--scope', choices=['release', 'component'], default='release')
    p.add_argument('--root-materials',type=Path)
    p.add_argument('--root-manifest-sha256')
    p.add_argument('--signed-application',type=Path)
    p.add_argument('--release-public',type=Path)
    p.add_argument('--manifest-sha256')
    p.add_argument('--system-root', type=Path)
    p.add_argument('--system-index', type=Path)
    p.add_argument('--distribution', choices=['public', 'self-use'], default='public')
    return p


def main():
    result = audit(parser().parse_args())
    print(json.dumps(result, sort_keys=True))
    return 0 if result['audit_passed'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
