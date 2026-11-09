#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Build committed, unsigned ARM product profiles. Never install or enable them."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import io
import signal
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
import arm_build_manifest as arm
import setup_ap_package as ap
sys.path.insert(0, str(Path(__file__).resolve().parent))
import managed_stage

PROFILES = {
    'legacy-v1': (False, False, False),
    'product-v1': (True, False, False),
    'product-email-v1': (True, True, False),
    'product-recovery-v1': (True, False, True),
    'product-email-recovery-v1': (True, True, True),
    'product-recovery-health-v1': (True, False, True),
}
EXTRA = ('nkn-root', 'nkn-sdk-engine', 'rustsimhub-ble-recovery')
BASE_ROLES = {'core': 986, 'edge': 985, 'media': 984, 'forwarder': 983}
require = arm.require
sha = arm.sha


def profile(name):
    require(type(name) is str and name in PROFILES, 'unknown fixed profile')
    return PROFILES[name]


def prefixes(name):
    product, _, recovery = profile(name)
    return (['--enable-update-health-candidate'] if name == 'product-recovery-health-v1' else []) + (['--enable-product-providers-candidate'] if product else []) + (
        ['--enable-secure-recovery-candidate'] if recovery else [])


def contract(core, worker, name, source):
    _, email, _ = profile(name)
    require(core.get('component') == 'core' and core.get('target_arch') == 'aarch64'
            and core.get('source_archive_sha256') == source and core.get('development_fixtures') is False,
            'actual same-source production Core required')
    require(type(core.get('database_schema')) is int and core['database_schema'] == 6,
            'unsupported base schema')
    expected = {'format': 1, 'worker_protocol': 5 if email else 4,
                'outbox': {'application_id': 0x52534657, 'read_format': 4 if email else 3,
                           'write_format': 4 if email else 3, 'migrate_from': [3] if email else [1, 2]}}
    require(core.get('forwarding_runtime_contract') == expected
            and worker.get('forwarding_runtime_contract') == expected
            and worker.get('worker') is True and type(worker.get('protocol')) is int
            and worker['protocol'] == expected['worker_protocol']
            and worker.get('source_archive_sha256') == source
            and worker.get('development_fixtures') is False, 'Core/worker contract or source mismatch')
    # Reject JSON true/1 equivalence inside nested integer contract fields.
    require(json.dumps(core['forwarding_runtime_contract'], sort_keys=True) == json.dumps(expected, sort_keys=True)
            and json.dumps(worker['forwarding_runtime_contract'], sort_keys=True) == json.dumps(expected, sort_keys=True),
            'strict runtime contract types')
    return expected


def role_plan(passwd, group, name):
    """Use the real existing AP role guard first, then check two optional identities."""
    sys.path.insert(0, str(ROOT / 'tools/system'))
    import setup_ap_install
    setup_ap_install.identities(passwd, group)
    def rows(text, width):
        require(type(text) is str and 0 < len(text.encode()) <= 65536, 'bounded NSS text required')
        result = [line.split(':') for line in text.splitlines()]
        require(all(len(r) == width and r[0] and re.fullmatch('0|[1-9][0-9]*', r[2]) for r in result), 'invalid NSS')
        return result
    users, groups = rows(passwd, 7), rows(group, 4)
    output = {r: {'uid': uid, 'gid': uid} for r, uid in BASE_ROLES.items()}
    if not profile(name)[0]:
        return output
    occupied = {uid for uid in BASE_ROLES.values()} | {0, 29, 987, 2**32-1}
    for role in ('ble', 'nkn'):
        full = 'rustsimhub-' + role
        found = [r for r in users if r[0] == full]
        require(len(found) == 1, 'optional installed role required: ' + role)
        row = found[0]
        require(re.fullmatch('[1-9][0-9]*', row[3]) is not None, 'canonical role GID')
        uid, gid = int(row[2]), int(row[3])
        require(uid == gid and 0 < uid < 2**32-1 and uid not in occupied, 'optional role alias')
        require([r for r in users if r[2] == row[2] or r[3] == row[3]] == [row], 'foreign optional role member')
        require([(g[0], g[2]) for g in groups if g[0] == full or g[2] == row[3]] == [(full, row[3])], 'optional group alias')
        require(all(not g[3] or g[3] == full for g in groups if g[2] == row[3]), 'foreign optional supplementary member')
        require(all(g[2] == row[3] for g in groups if full in g[3].split(',')), 'optional role supplementary privilege')
        occupied.add(uid)
        output[role] = {'uid': uid, 'gid': gid}
    return output


def strict_json(raw, limit=128*1024):
    require(isinstance(raw, bytes) and 0 < len(raw) <= limit, 'bounded JSON required')
    def invalid(_):
        raise ValueError('nonfinite JSON')
    return json.loads(raw, object_pairs_hook=arm.unique, parse_constant=invalid)


def write_new(path, data, mode=0o444):
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    path.chmod(mode)


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + '\n').encode()


def inspect_program(path, runner, args=('--build-info',)):
    return strict_json(arm.bounded_run([*runner, str(path), *args]))


def launcher_check(launcher, runner, core, installed, digest):
    request = {'metadata': json.dumps(core), 'ap_contract': json.dumps(ap.CONTRACT),
               'profile': json.dumps(installed), 'core_sha256': digest}
    report = strict_json(arm.bounded_run([*runner, str(launcher), '--check-product-profile'], stdin_data=encoded(request)))
    require(report.get('authorization_verified') is False and report.get('starts_services') is False,
            'launcher comparison is not authorization')
    return report


def render(output, name, source, hashes, roles):
    product, email, recovery = profile(name)
    installed = {'format': 1, 'profile': name, 'source_archive_sha256': source,
                 'core_sha256': hashes['rustsimhub-core']}
    write_new(output / 'product-profile.json', encoded(installed))
    material = ROOT / 'firmware/system/product-profiles'
    files = ['50-product-profile.conf']
    if product:
        files += ['rustsimhub-nkn-root.service', 'product.tmpfiles.in']
    if recovery:
        files += ['rustsimhub-ble-recovery.service', 'ble-product.policy']
    if name == 'product-recovery-health-v1':
        files += ['55-product-health.conf']
        health = ROOT / 'firmware/system/update-health'
        for source_name, destination in [('tmpfiles.conf', 'health.tmpfiles'),
                ('rustsimhub-update-health.service', 'rustsimhub-update-health.service'),
                ('rustsimhub-update-health.timer', 'rustsimhub-update-health.timer')]:
            write_new(output / destination, (health / source_name).read_bytes())
    values = {'@NKN_GID@': str(roles.get('nkn', {}).get('gid', 0)),
              '@BLE_GID@': str(roles.get('ble', {}).get('gid', 0)),
              '@CORE_MEMORY@': '192M' if product else '96M', '@CORE_TASKS@': '64' if product else '16'}
    for filename in files:
        raw = (material / filename).read_bytes().replace(b'\r\n', b'\n')
        for key, value in values.items():
            raw = raw.replace(key.encode(), value.encode())
        require(b'@' not in raw, 'unresolved fixed template')
        write_new(output / filename.replace('.in', ''), raw)
    return installed, {'profile': name, 'compiled_provider': product, 'email_contract': email,
        'recovery_schema': 9 if recovery else None, 'automatic_migration': False,
        'forwarding_enabled': False, 'nkn_references': {'manual': False, 'remote': False, 'statistics': False},
        'ble_enabled': False, 'health_confirmation': 'fixed-running-candidate-v1' if name == 'product-recovery-health-v1' else 'unsupported_for_this_profile',
        'core_prefixes': prefixes(name), 'roles': roles, 'starts_services': False,
        'installation_permitted': False, 'requires': ['fresh device evidence', 'reviewed signed Root write set',
        'actual business/outbox compatibility and rollback', 'private NKN policy/seed/member material']}


def run_build(command, cwd, env, log, *, timeout=1200, max_bytes=8*1024**2):
    # Tools and switches are defined by this source, not by profile JSON.
    with log.open('ab') as stream:
        stream.write((json.dumps(command) + '\n').encode())
        stream.flush()
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        reaped = False
        try:
            deadline = time.monotonic() + timeout
            while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
                require(time.monotonic() < deadline, 'fixed build deadline exceeded')
                require(os.fstat(stream.fileno()).st_size <= max_bytes, 'fixed build log exceeded budget')
                time.sleep(.05)
            require(os.fstat(stream.fileno()).st_size <= max_bytes, 'fixed build log exceeded budget')
            # Keep the leader unreaped until cleanup, so a reused PID can never
            # turn this signal into cleanup of another task's process group.
            try: os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError: pass
            code = process.wait(timeout=3)
            reaped = True
            require(code == 0, 'fixed build stage failed; preserve output and log')
        finally:
            if not reaped:
                try: os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError: pass
                process.wait(timeout=3)


def build(args):
    require(__debug__, 'existing SDK verifier must not run with Python optimization')
    require(sys.platform == 'linux' and os.geteuid() != 0, 'ordinary Linux workstation required')
    product, email, recovery = profile(args.profile)
    require(args.output.is_absolute() and not args.output.exists() and not args.output.is_symlink(), 'fresh absolute output')
    require(args.output.parent.is_dir() and not args.output.parent.is_symlink(), 'output parent required')
    # Validate complete committed archive BEFORE creating the output tree.
    arm.verify_source(args.archive, args.repository, args.revision, args.source)
    archive_bytes = arm.regular(args.archive, 64*1024**2)
    require(sha(archive_bytes) == args.source, 'source archive changed')
    roles_input = strict_json(arm.regular(args.roles, 256*1024), 256*1024)
    require(type(roles_input) is dict, 'NSS object required')
    require(set(roles_input) == {'passwd', 'group', 'nss_passwd', 'nss_group'}, 'exact NSS snapshot fields')
    roles = role_plan(roles_input['passwd'], roles_input['group'], args.profile)
    require(role_plan(roles_input['nss_passwd'], roles_input['nss_group'], args.profile) == roles,
            'local/NSS role disagreement')
    args.output.mkdir(mode=0o700)
    write_new(args.output / 'INCOMPLETE', b'Unsigned build in progress; never install.\n')
    source = args.output / 'source'
    arm.verify_source(args.archive, args.repository, args.revision, args.source, source)
    # The running builder and all reused policy code/templates must themselves
    # be the frozen source, not a newer dirty checkout claiming an older stamp.
    for item in ['tools/product-release/release.py', 'tools/product-release/managed_stage.py', 'tools/arm_build_manifest.py',
                 'tools/setup_ap_package.py', 'tools/system/setup_ap_install.py',
                 'firmware/system/setup-ap/contract.json',
                 *['firmware/system/update-health/' + p.name for p in (ROOT/'firmware/system/update-health').iterdir() if p.is_file()],
                 *['firmware/system/product-profiles/' + p.name for p in (ROOT/'firmware/system/product-profiles').iterdir()]]:
        require((ROOT/item).read_bytes() == (source/item).read_bytes(), 'running tool differs from committed source')
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.output / 'target'),
               RUSTSIMHUB_BUILD_SOURCE_SHA256=args.source, CARGO_NET_OFFLINE='true',
               CARGO_PROFILE_RELEASE_OPT_LEVEL='z')
    # Explicit optional cache reuse is forbidden here: per-profile target prevents
    # a later feature build from silently overwriting an earlier exported program.
    log = args.output / 'build.log'
    sdk_provenance = None
    if product:
        require(args.verified_sdk_stage is not None and 'GOMODCACHE' in env, 'verified SDK stage and explicit GOMODCACHE required')
        go_env = dict(env, CGO_ENABLED='0', GOOS='linux', GOARCH='arm64', GOTOOLCHAIN='local',
                      GOPROXY='off', GOSUMDB='off', GOWORK='off', GOENV='off', GOFLAGS='')
        sdk_stage, sdk_provenance = managed_stage.prepare(source, args.verified_sdk_stage,
            args.output/'sdk-source', go_env, args.revision)
    common = ['cargo', '+1.98.0', 'build', '--release', '--locked', '--offline', '--no-default-features',
              '--target', 'aarch64-unknown-linux-gnu', '-j', '2']
    run_build([*common, '--workspace'], source, env, log)
    if product:
        features = 'rustsimhub-core/product-providers-candidate'
        if email:
            features += ',rustsimhub-core/email-production-candidate,rustsimhub-forwarder/smtp-production-candidate'
        run_build([*common, '-p', 'rustsimhub-core', '-p', 'rustsimhub-forwarder', '--features', features], source, env, log)
        run_build([*common, '-p', 'rustsimhub-ble', '--bin', 'rustsimhub-ble-recovery', '--features', 'product-host-candidate'], source, env, log)
    audio_lib = args.audio_sysroot/'usr/lib/aarch64-linux-gnu'
    audio_env = dict(env, PKG_CONFIG_ALLOW_CROSS='1', PKG_CONFIG_PATH='',
        PKG_CONFIG_SYSROOT_DIR=str(args.audio_sysroot), PKG_CONFIG_LIBDIR=str(audio_lib/'pkgconfig'))
    run_build(['cargo', '+1.98.0', 'build', '--release', '--manifest-path', 'native/audio-alsa/Cargo.toml',
               '--locked', '--offline', '--target', 'aarch64-unknown-linux-gnu', '-j', '2'], source, audio_env, log)
    binary_dir = args.output / 'target/aarch64-unknown-linux-gnu/release'
    programs = args.output / 'programs'
    programs.mkdir(mode=0o700)
    names = list(arm.PROGRAMS) + ['rustsimhub-audio-alsa'] + (['rustsimhub-ble-recovery'] if product else [])
    for name in names:
        data = arm.regular(binary_dir / name, single_link=False)
        arm.elf(data)
        write_new(programs / name, data, 0o555)
    if product:
        for name in ['nkn-root', 'nkn-sdk-engine']:
            destination = programs / name
            flags = '-s -w -buildid=rsh-' + args.source
            if name == 'nkn-sdk-engine':
                flags += ' -X main.sourceCommit=' + args.revision
            run_build(['go', 'build', '-trimpath', '-buildvcs=false', '-mod=readonly', '-tags=nkn_sdk_candidate',
                       '-ldflags=' + flags, '-o', str(destination),
                       'github.com/Viper-Boss/RustSimHub/tools/nkn-engine/cmd/' + name], sdk_stage, go_env, log)
            arm.elf(arm.regular(destination, 32*1024**2))
            require(arm.bounded_run(['go', 'tool', 'buildid', str(destination)]).decode().strip() == 'rsh-' + args.source,
                    'Go source build ID mismatch')
            destination.chmod(0o555)
            names.append(name)
    runner = ['qemu-aarch64', '-L', str(args.sysroot), '-E', 'LD_LIBRARY_PATH='+str(audio_lib)]
    infos = {n: inspect_program(programs / ('rustsimhub-' + n), runner) for n in arm.COMPONENTS}
    worker = inspect_program(programs / 'rustsimhub-forwarder', runner)
    audio = inspect_program(programs / 'rustsimhub-audio-alsa', runner)
    require(audio.get('source_archive_sha256') == args.source and audio.get('local_media_worker_supported') is True,
            'same-source production audio required')
    arm.contracts(infos, args.source)
    runtime = contract(infos['core'], worker, args.profile, args.source)
    mode_info = inspect_program(programs / 'rustsimhub-core', runner,
        ('--product-runtime-compatibility', args.profile))
    require(mode_info.get('mode') == args.profile and mode_info.get('source_archive_sha256') == args.source
        and mode_info.get('target_arch') == 'aarch64' and mode_info.get('prefixes') == prefixes(args.profile)
        and mode_info.get('opens_database') is False and mode_info.get('starts_services') is False,
        'actual fixed-mode production Core metadata required')
    require(mode_info.get('business') == {'application_id': 1381189698, 'read': [9 if recovery else 6],
        'write': 9 if recovery else 6, 'maintenance_from': [6, 7] if recovery else [], 'automatic_migration': False},
        'actual fixed-mode database contract required')
    # Actual prefix parsing must succeed in the exact compiled Core.
    require(inspect_program(programs / 'rustsimhub-core', runner, (*prefixes(args.profile), '--build-info')) == infos['core'],
            'Core does not implement selected profile prefixes')
    sdk = None
    license_report = None
    if product:
        sdk = inspect_program(programs / 'nkn-sdk-engine', runner, ('--version',))
        require(sdk.get('source_commit') == args.revision and sdk.get('nkn_sdk_linked') is True
                and sdk.get('isolated_fixture_build') is False and sdk.get('root_launch_required') is True,
                'actual nonfixture SDK required')
        license_report = managed_stage.notices(source, sdk_stage, programs, args.output/'sdk-licenses', go_env)
    hashes = {n: sha(arm.regular(programs / n, 32*1024**2)) for n in names}
    materials = args.output / 'materials'
    materials.mkdir(mode=0o700)
    installed, plan = render(materials, args.profile, args.source, hashes, roles)
    checked = launcher_check(programs / 'rustsimhub-core-launcher', runner, infos['core'], installed, hashes['rustsimhub-core'])
    # Publish the exact original application payload set without inventing a
    # release epoch/signature. A later authorized signer supplies those inputs.
    application = args.output / 'application-input'
    application.mkdir(mode=0o700)
    for name in ['rustsimhub-core','simhubctl','rustsimhub-edge','rustsimhub-audio-alsa','rustsimhub-forwarder']:
        write_new(application / name, arm.regular(programs / name, 24*1024**2), 0o555)
    opus = next(p for p in (source/'licenses').rglob('*') if p.is_file() and 'opus' in p.name.lower() and 'copying' in p.name.lower())
    write_new(application / 'Opus-COPYING.txt', opus.read_bytes())
    total = sum(p.stat().st_size for p in application.iterdir())
    require(total <= 24*1024**2, 'original total application payload budget exceeded')
    write_new(args.output/'application-input.json', encoded({'format':1,'kind':'unsigned-application-input',
        'source_archive_sha256':args.source,'profile':args.profile,'schema_min':6,'schema_max':9 if recovery else 6,
        'files':{p.name:{'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in application.iterdir()},
        'payload_bytes':total,'epoch_assigned':False,'signed':False,'installable':False}))
    # Origin/ALSA and the five existing payloads are unchanged by this tool;
    # explicitly not a bootable image or a signed application group.
    manifest = {'format': 1, 'kind': 'unsigned-product-profile', 'revision': args.revision,
        'source_archive_sha256': args.source, 'profile': args.profile, 'target': 'aarch64-unknown-linux-gnu',
        'programs': {n: {'sha256': hashes[n], 'bytes': (programs/n).stat().st_size,
                       'glibc': arm.versions(programs/n)} for n in names}, 'build_info': infos,
        'worker': worker, 'audio': audio, 'mode_metadata': mode_info,
        'rust_profile': {'name': 'release', 'opt_level': 'z', 'lto': 'thin', 'strip': 'symbols'}, 'runtime_contract': runtime, 'sdk': sdk, 'sdk_provenance': sdk_provenance,
        'sdk_licenses': license_report, 'launch_check': checked, 'plan': plan,
        'materials': {p.name: sha(p.read_bytes()) for p in materials.iterdir()},
        'signed': False, 'epoch_assigned': False, 'installable': False, 'bootable_rootfs': False,
        'hardware_tested': False, 'health_authorized': False}
    licenses = source / 'licenses'
    require(licenses.is_dir(), 'source licenses required')
    shutil.copytree(licenses, args.output / 'licenses')
    shutil.copyfile(source/'LICENSE', args.output/'licenses/PROJECT-LICENSE')
    manifest['licenses'] = {str(p.relative_to(args.output)): sha(p.read_bytes())
        for folder in ['licenses', 'sdk-licenses'] for p in (args.output/folder).rglob('*') if p.is_file()}
    # Source and lock bytes must still equal the frozen archive after all builds.
    with tarfile.open(fileobj=io.BytesIO(archive_bytes)) as archive:
        for member in archive:
            if member.isfile():
                require((source/member.name).read_bytes() == archive.extractfile(member).read(), 'build modified committed source')
    write_new(args.output / 'profile-build.json', encoded(manifest))
    write_new(args.output / 'COMPLETED', sha(encoded(manifest)).encode() + b'\n')
    (args.output / 'INCOMPLETE').unlink()
    return manifest


def assemble_unsigned(argv):
    """Reuse the existing verifier/packager over independently signed inputs."""
    import importlib.util
    parser=argparse.ArgumentParser(description='Assemble unsigned finite Root inputs from an actual same-source build and signed application.')
    parser.add_argument('--assemble-unsigned',action='store_true',required=True)
    for name in ['build-root','application','app-public','roles-policy','output']:
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args(argv)
    spec=importlib.util.spec_from_file_location('original_product_packager',ROOT/'tools/product-install/package.py')
    package=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(package)
    try:
        result=package.create(args.build_root,args.application,args.app_public,args.roles_policy,None,args.output)
    except (ValueError,OSError,subprocess.SubprocessError,AssertionError) as error:
        parser.exit(1,'Unsigned assembly refused: '+str(error)+'\n')
    print(json.dumps(result,sort_keys=True))


def main():
    if '--assemble-unsigned' in sys.argv[1:]:
        return assemble_unsigned(sys.argv[1:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=tuple(PROFILES), required=True)
    for name in ('archive', 'repository', 'output', 'roles', 'sysroot', 'audio-sysroot'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--verified-sdk-stage', type=Path)
    args = parser.parse_args()
    try:
        result = build(args)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, 'Product build refused: ' + str(error) + '\n')
    print(json.dumps({'profile': result['profile'], 'programs': len(result['programs']),
                      'signed': False, 'installable': False, 'device_access': False}))


if __name__ == '__main__':
    main()
