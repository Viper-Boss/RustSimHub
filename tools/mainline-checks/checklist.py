# SPDX-License-Identifier: AGPL-3.0-only
"""Reviewed fixed argv inventory. No glob-based command discovery or shell."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Check:
    name: str
    group: str
    argv: tuple
    timeout: int = 180
    requires: tuple = ()
    files: tuple = ()
    kind: str = 'command'
    cwd: str = '.'
    depends: tuple = ()
    artifact: str = ''
    committed_source: bool = False


CHECKS = []


def add(name, group, argv, **kw):
    CHECKS.append(Check(name, group, tuple(argv), **kw))


add('fmt', 'ordinary', ['cargo', 'fmt', '--all', '--', '--check'], requires=('rustfmt',))
add('clippy', 'ordinary', ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'], timeout=1800, requires=('go', 'clippy'))
for suffix, features in [('default', []), ('all-features', ['--all-features'])]:
    add('workspace-' + suffix, 'ordinary', ['cargo', 'test', '--workspace', *features, '--locked'], timeout=1800, requires=('go',), kind='rust')
add('production-check', 'ordinary', ['cargo', 'check', '-p', 'rustsimhub-core', '--no-default-features', '--locked'], timeout=1800)
for name, files in [
    ('rootfs-syntax', ['tools/rootfs/scripts/build-rootfs.sh']),
    ('recovery-probe-syntax', ['tools/recovery-fastboot-probe.sh']),
    ('committed-build-syntax', ['tools/wsl-build-committed.sh']),
    ('recovery-syntax', ['tools/recovery/watch-edl-capture.sh']),
    ('field-syntax', ['tools/field/rsh-readonly-snapshot.sh']),
]:
    add(name, 'ordinary', ['sh', '-n', *files], files=tuple(files))
for name, directory, pattern in [('rootfs', 'tools/rootfs/scripts', 'test_rootfs.py'),
                                  ('recovery', 'tools/recovery', 'test_*.py'),
                                  ('field', 'tools/field', 'test_*.py')]:
    add(name, 'ordinary', ['python3', '-m', 'unittest', 'discover', '-s', directory, '-p', pattern], kind='unittest', files=(directory,))
for name, script, kind in [
    ('arm-manifest', 'tools/test-arm-build-manifest.py', 'unittest'),
    ('pon-review', 'tools/test-pon-dump-review.py', 'command'),
    ('development-packager', 'tools/test-development-application-packager.py', 'command'),
    ('ap-package', 'tools/test-setup-ap-package.py', 'unittest'),
    ('ap-install', 'tools/test-setup-ap-install.py', 'unittest'),
]:
    requires = ('pycrypto', 'openssl') if name in ('development-packager', 'ap-package', 'ap-install') else ()
    add(name, 'ordinary', ['python3', script], files=(script,), kind=kind, requires=requires)
add('arm-manifest-optimized', 'ordinary', ['python3', '-O', 'tools/test-arm-build-manifest.py'], files=('tools/test-arm-build-manifest.py',), kind='unittest')
# 27 explicit synthetic Node scripts; browser-only recovery interop is deferred below.
WEB = (
    'ap-wifi', 'call-test', 'clients', 'configuration-state', 'daily-phone',
    'daily-sms', 'deadlines', 'diagnostics', 'event-sync', 'forwarding-history',
    'forwarding', 'incoming-call-test', 'indicator', 'media', 'nas-bridge',
    'network', 'nkn-status', 'operation-receipt', 'password', 'proof',
    'session', 'setup', 'sms-management',
    'test-center-component', 'test-center', 'usage-stats', 'wifi',
)
for name in WEB:
    script = 'tools/test-web-' + name + '.mjs'
    add('web-' + name, 'ordinary', ['node', script], files=(script,))
for name in ('cancel', 'controller', 'gatt-framing', 'static-update'):
    script = 'configurator/tests/' + name + '.mjs'
    add('configurator-' + name, 'ordinary', ['node', script], files=(script,))
add('go-offline-tests', 'ordinary', ['go', 'test', '-mod=readonly', '-count=1', '-timeout=120s', '-v', './...'], cwd='tools/nkn-engine', kind='go', timeout=240)
for name, action in [('native-test', 'test'), ('native-clippy', 'clippy')]:
    argv = ['cargo', action, '--manifest-path', 'native/audio-alsa/Cargo.toml']
    if action == 'clippy':
        argv += ['--all-targets']
    argv += ['--locked']
    if action == 'clippy':
        argv += ['--', '-D', 'warnings']
    add(name, 'native-audio', argv, requires=('audio',), kind='rust' if action == 'test' else 'command', timeout=1800)
add('compatibility-build', 'ordinary', ['cargo', 'build', '-p', 'rustsimhub-core', '-p', 'rustsimhub-forwarder', '-p', 'rustsimhub-core-launcher', '--no-default-features', '--locked'], committed_source=True, timeout=1800)
add('upgrade-compatibility', 'ordinary', ['python3', 'tools/test-upgrade-compatibility.py', '--launcher', 'target/debug/rustsimhub-core-launcher', '--core', 'target/debug/rustsimhub-core', '--worker', 'target/debug/rustsimhub-forwarder'], depends=('compatibility-build',))
add('worker-inspection', 'ordinary', ['python3', 'tools/test-forwarding-inspection.py', '--worker', 'target/debug/rustsimhub-forwarder'], depends=('compatibility-build',))
add('runner-unit', 'runner', ['python3', '-m', 'unittest', 'discover', '-s', 'tools/mainline-checks', '-p', 'test_*.py'], kind='unittest')
# Specialized groups are opt-in. Use the original fixtures and privilege prefixes.
add('board-fixtures', 'root', ['sudo', '-n', 'python3', '-m', 'unittest', 'discover', '-s', 'tools/system', '-p', 'test_*.py'], requires=('root', 'mount'), kind='unittest', timeout=600)
add('ap-units', 'root', ['sudo', '-n', 'python3', 'tools/test-setup-ap-units.py'], requires=('root',), kind='command')
add('ap-build', 'root', ['cargo', 'build', '-p', 'rustsimhub-core', '-p', 'rustsimhub-edge', '-p', 'rustsimhub-setup-ap', '-p', 'rustsimhub-core-launcher', '--locked'], requires=('root',), timeout=1800)
AP = ('--core', 'target/debug/rustsimhub-core', '--edge', 'target/debug/rustsimhub-edge', '--ap', 'target/debug/rustsimhub-setup-ap')
add('ap-material', 'root', ['sudo', '-n', 'python3', 'tools/test-setup-ap-material.py', *AP, '--edge-user', '{user}'], requires=('root',), depends=('ap-build',))
add('core-launcher', 'root', ['sudo', '-n', 'python3', 'tools/test-core-launcher.py', '--launcher', 'target/debug/rustsimhub-core-launcher'], requires=('root',), depends=('ap-build',))
add('ap-chroot', 'root', ['sudo', '-n', 'python3', 'tools/test-setup-ap-install-chroot.py', *AP, '--launcher', 'target/debug/rustsimhub-core-launcher'], requires=('root',), depends=('ap-build',))
add('forwarder-artifacts', 'root-netns', ['cargo', 'test', '-p', 'rustsimhub-forwarder', '--lib', '--all-features', '--locked', '--no-run', '--message-format=json'], requires=('root', 'netns', 'mount'), artifact='forwarder-artifacts.json', timeout=1800)
add('storage-full', 'root-netns', ['sudo', '-n', 'python3', 'tools/test-forwarding-storage-full.py', '--cargo-artifacts', '{run}/forwarder-artifacts.json'], requires=('root', 'netns', 'mount'), depends=('forwarder-artifacts',))
add('update-build', 'root-netns', ['cargo', 'build', '-p', 'rustsimhub-update', '--locked'], requires=('root', 'netns'), timeout=1800)
add('update-artifacts', 'root-netns', ['cargo', 'test', '-p', 'rustsimhub-update', '--lib', '--all-features', '--locked', '--no-run', '--message-format=json'], requires=('root', 'netns'), artifact='update-artifacts.json', timeout=1800)
for name, script, args, deps in [
    ('update-state', 'test-update-state-inspection.py', ['--updater', 'target/debug/rustsimhub-update'], ['update-build']),
    ('metadata', 'test-metadata-process.py', ['--cargo-artifacts', '{run}/update-artifacts.json'], ['update-artifacts']),
    ('metadata-no-kill', 'test-metadata-process.py', ['--cargo-artifacts', '{run}/update-artifacts.json', '--missing-kill'], ['update-artifacts']),
    ('maintenance', 'test-maintenance-observation.py', ['--updater', 'target/debug/rustsimhub-update'], ['update-build']),
    ('signed-formats', 'test-signed-application-formats.py', ['--updater', 'target/debug/rustsimhub-update'], ['update-build']),
]:
    prefix = ['sudo', '-n', 'unshare', '--net']
    if name.startswith('metadata'):
        prefix += ['setpriv'] + (['--bounding-set=-kill'] if name == 'metadata-no-kill' else []) + ['--no-new-privs']
    add(name, 'root-netns', [*prefix, 'python3', 'tools/' + script, *args], requires=('root', 'netns'), depends=tuple(deps))
add('ipc-core-build', 'root-netns', ['cargo', 'build', '-p', 'rustsimhub-core', '--features', 'dev-fixtures', '--locked'], requires=('root', 'netns'), timeout=1800)
add('worker-build', 'root-netns', ['cargo', 'build', '-p', 'rustsimhub-forwarder', '--locked'], requires=('root', 'netns'), timeout=1800)
WORKER = ('--core', 'target/debug/rustsimhub-core', '--worker', 'target/debug/rustsimhub-forwarder')
add('worker-ipc', 'root-netns', ['sudo', '-n', 'python3', 'tools/test-forwarding-worker-ipc.py', '--core', 'target/debug/rustsimhub-core'], requires=('root', 'netns'), depends=('ipc-core-build',))
add('worker-process', 'root-netns', ['sudo', '-n', 'unshare', '--net', 'python3', 'tools/test-forwarding-worker-process.py', *WORKER], requires=('root', 'netns'), depends=('ipc-core-build', 'worker-build'))
add('indicator-build', 'root-systemd', ['cargo', 'build', '-p', 'rustsimhub-update', '--example', 'indicator-resource', '--locked'], requires=('root', 'systemd', 'mount'), timeout=1800)
add('indicator-resource', 'root-systemd', ['sudo', '-n', 'python3', 'tools/test-update-indicator-resource.py', '--fixture', 'target/debug/examples/indicator-resource'], requires=('root', 'systemd', 'mount'), depends=('indicator-build',))
add('supervisor-core-build', 'root-systemd', ['cargo', 'build', '-p', 'rustsimhub-core', '--features', 'dev-fixtures', '--locked'], requires=('root', 'systemd', 'netns'), timeout=1800)
add('supervisor-worker-build', 'root-systemd', ['cargo', 'build', '-p', 'rustsimhub-forwarder', '--features', 'dev-fixtures', '--locked'], requires=('root', 'systemd', 'netns'), timeout=1800)
for name, args in [('unmanaged', []), ('managed', ['--managed']), *[(p, ['--managed', '--close-at', p]) for p in ('dns', 'tls', 'http', 'late_ack')]]:
    add('supervisor-' + name, 'root-systemd', ['sudo', '-n', 'unshare', '--net', 'python3', 'tools/test-forwarding-supervisor.py', *WORKER, *args], requires=('root', 'systemd', 'netns', 'polkit') if name != 'unmanaged' else ('root', 'systemd', 'netns'), depends=('supervisor-core-build', 'supervisor-worker-build'), timeout=300)

GROUPS = ('ordinary', 'native-audio', 'root', 'root-netns', 'root-systemd', 'runner')

# This is an inventory boundary, never an executable substitute for real browser evidence.
EXCLUSIONS = ({'name': 'web-recovery-session', 'status': 'not_run',
               'entrypoint': 'tools/run-web-recovery-session.py',
               'reason': 'browser-only WASM/WebCrypto/IndexedDB interop; needs prepared native peer/WASM and actual Chromium; not a Node script or real GATT acceptance'},)
