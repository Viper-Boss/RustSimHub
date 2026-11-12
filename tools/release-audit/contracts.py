# SPDX-License-Identifier: AGPL-3.0-only
"""Finite profile/material policy. Does not extend an installer or a health profile."""
from safeio import need, same

PROFILES = {
    'legacy-v1': (False, False, False, False),
    'product-v1': (True, False, False, False),
    'product-email-v1': (True, True, False, False),
    'product-recovery-v1': (True, False, True, False),
    'product-email-recovery-v1': (True, True, True, False),
    'product-recovery-health-v1': (True, False, True, True),
}
BASE_PROGRAMS = {
    'rustsimhub-core', 'simhubctl', 'rustsimhub-edge', 'rustsimhub-update',
    'rustsimhub-recovery', 'rustsimhub-migrate', 'rustsimhub-status-led',
    'rustsimhub-network-agent', 'rustsimhub-forwarder', 'rustsimhub-setup-ap',
    'rustsimhub-core-launcher', 'rustsimhub-mdns', 'rustsimhub-audio-alsa',
}
APP_FILES = ('rustsimhub-core', 'simhubctl', 'rustsimhub-edge',
             'rustsimhub-audio-alsa', 'rustsimhub-forwarder', 'Opus-COPYING.txt')
BASE_ROLES = {'core': 986, 'edge': 985, 'media': 984, 'forwarder': 983}


def profile(name):
    need(type(name) is str and name in PROFILES, 'profile_unknown')
    return PROFILES[name]


def check_report(report, expected_profile, source, revision, arm, ap_contract):
    product, email, recovery, health = profile(expected_profile)
    need(type(report) is dict and type(report.get('format')) is int
         and report['format'] == 1 and report.get('kind') == 'unsigned-product-profile',
         'profile_format')
    need(report.get('profile') == expected_profile and report.get('target') == 'aarch64-unknown-linux-gnu',
         'profile_or_architecture')
    need(report.get('revision') == revision and report.get('source_archive_sha256') == source,
         'profile_source')
    for key in ('signed', 'epoch_assigned', 'installable', 'health_authorized', 'bootable_rootfs'):
        need(report.get(key) is False, 'metadata_cannot_authorize')
    info = report.get('build_info')
    need(type(info) is dict, 'component_metadata')
    # Original shared verifier: sources, architecture, fixed Core/Edge/AP/
    # launcher contracts and existing updater capabilities.
    arm.contracts(info, source)
    need(same(info['core'].get('setup_ap_installation_contract'), ap_contract), 'setup_contract_unknown')
    core, worker = info['core'], report.get('worker')
    need(type(worker) is dict and worker.get('source_archive_sha256') == source
         and worker.get('development_fixtures') is False and worker.get('worker') is True,
         'worker_identity')
    runtime = {'format': 1, 'worker_protocol': 5 if email else 4,
               'outbox': {'application_id': 0x52534657, 'read_format': 4 if email else 3,
                          'write_format': 4 if email else 3, 'migrate_from': [3] if email else [1, 2]}}
    need(same(report.get('runtime_contract'), runtime)
         and same(core.get('forwarding_runtime_contract'), runtime)
         and same(worker.get('forwarding_runtime_contract'), runtime)
         and type(worker.get('protocol')) is int and worker['protocol'] == runtime['worker_protocol'],
         'runtime_version_contract')
    mode = report.get('mode_metadata')
    need(type(mode) is dict and mode.get('format') == 1 and type(mode['format']) is int
         and mode.get('kind') == 'product-runtime-compatibility'
         and mode.get('component') == 'core' and mode.get('target_arch') == 'aarch64'
         and mode.get('source_archive_sha256') == source and mode.get('mode') == expected_profile
         and mode.get('development_fixtures') is False, 'mode_identity')
    for key in ('authorization_verified', 'opens_database', 'starts_services'):
        need(mode.get(key) is False, 'mode_cannot_authorize')
    need(same(mode.get('features'), {'email': email, 'product_providers': product,
                                   'recovery_execution': recovery}), 'compiled_features')
    prefixes = (['--enable-update-health-candidate'] if health else []) + (
        ['--enable-product-providers-candidate'] if product else []) + (
        ['--enable-secure-recovery-candidate'] if recovery else [])
    need(same(mode.get('prefixes'), prefixes), 'mode_prefixes')
    business = mode.get('business', {})
    need(type(business) is dict and type(business.get('write')) is int
         and business['write'] == (9 if recovery else 6)
         and business.get('automatic_migration') is False, 'business_schema')
    need(same(mode.get('forwarding_runtime_contract'), runtime), 'mode_runtime_contract')
    plan = report.get('plan', {})
    need(type(plan) is dict and plan.get('profile') == expected_profile, 'plan_profile')
    for key in ('forwarding_enabled', 'ble_enabled', 'automatic_migration', 'starts_services', 'installation_permitted'):
        need(plan.get(key) is False, 'default_off')
    need(same(plan.get('nkn_references'), {'manual': False, 'remote': False, 'statistics': False}),
         'default_nkn_off')
    need(same(plan.get('core_prefixes'), prefixes), 'plan_prefixes')
    if health:
        need(plan.get('health_confirmation') == 'fixed-running-candidate-v1', 'health_contract')
    else:
        need(plan.get('health_confirmation') in ('unsupported_for_this_profile', 'unsupported_until_window3_admission'),
             'unsupported_health_profile')
    roles = plan.get('roles', {})
    need(type(roles) is dict and set(roles) == set(BASE_ROLES) | ({'nkn', 'ble'} if product else set()),
         'roles_finite')
    used = {0, 29, 987, 65534}
    for role, row in roles.items():
        need(type(row) is dict and set(row) == {'uid', 'gid'}
             and type(row['uid']) is int and type(row['gid']) is int
             and row['uid'] == row['gid'] and 0 < row['uid'] < 2**32 - 1
             and row['uid'] not in used, 'role_identity')
        if role in BASE_ROLES:
            need(row['uid'] == BASE_ROLES[role], 'base_role_identity')
        else:
            need(row['uid'] not in set(BASE_ROLES.values()), 'optional_role_alias')
        used.add(row['uid'])
    expected_programs = BASE_PROGRAMS | ({'nkn-root', 'nkn-sdk-engine', 'rustsimhub-ble-recovery'} if product else set())
    need(type(report.get('programs')) is dict and set(report['programs']) == expected_programs,
         'program_inventory_finite')
    launch = report.get('launch_check', {})
    need(type(launch) is dict and launch.get('authorization_verified') is False
         and launch.get('starts_services') is False, 'launcher_cannot_authorize')
    need(launch.get('arguments', [])[:len(prefixes)] == prefixes, 'launcher_prefixes')
    arguments = launch.get('arguments')
    need(type(arguments) is list and all(type(v) is str and len(v) < 1024 for v in arguments)
         and '--serve-device-forwarding-managed-setup-ap' in arguments
         and not any(v in ('/bin/sh', '/bin/bash', '-c') for v in arguments), 'launcher_arguments')
    rust = report.get('rust_profile', {})
    need(type(rust) is dict and rust.get('name') == 'release' and rust.get('opt_level') == 'z'
         and rust.get('lto') == 'thin' and rust.get('strip') == 'symbols', 'release_profile')
    return product, email, recovery, health


def material_sources(product, recovery, health):
    out = {'50-product-profile.conf': 'firmware/system/product-profiles/50-product-profile.conf'}
    if product:
        out.update({'rustsimhub-nkn-root.service': 'firmware/system/product-profiles/rustsimhub-nkn-root.service',
                    'product.tmpfiles': 'firmware/system/product-profiles/product.tmpfiles.in'})
    if recovery:
        out.update({'rustsimhub-ble-recovery.service': 'firmware/system/product-profiles/rustsimhub-ble-recovery.service',
                    'ble-product.policy': 'firmware/system/product-profiles/ble-product.policy'})
    if health:
        out.update({'55-product-health.conf': 'firmware/system/product-profiles/55-product-health.conf',
                    'health.tmpfiles': 'firmware/system/update-health/tmpfiles.conf',
                    'rustsimhub-update-health.service': 'firmware/system/update-health/rustsimhub-update-health.service',
                    'rustsimhub-update-health.timer': 'firmware/system/update-health/rustsimhub-update-health.timer'})
    return out
