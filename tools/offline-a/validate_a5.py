#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""A5 fixed-source test observations; never a release/build manifest or deployer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import time

SOURCES = {
    'main': 'de663743bd726e77f7c560139d0f91ea78e8fe57',
    'a2': 'fd3358ef1cee1bc2305ca96d13f55ff1ac3fc4ae',
    'a3': 'c2a60903edb30a4bf4f3efb4dffa244da58d1cef',
    'a4': '774d4a05a1ff5191297f5dfa0ca4e0d0a0f89b6b',
}
FEATURES = 'usage-root-transport-candidate,email-production-candidate'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Run:
    def __init__(self, args):
        assert os.name == 'posix' and os.geteuid() != 0, 'use an unprivileged Linux test user'
        assert [n for _, n in socket.if_nameindex()] == ['lo'], 'run in a private network namespace'
        status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
        assert int(status['CapEff'].strip(), 16) == 0 and int(status['CapBnd'].strip(), 16) == 0
        assert status['NoNewPrivs'].strip() == '1'
        os.umask(0o077)
        self.args = args
        self.out = args.output.resolve()
        self.out.mkdir(parents=True, exist_ok=False)
        args.work_root.mkdir(parents=True, exist_ok=True)
        self.native = Path(tempfile.mkdtemp(prefix='a5-' + args.profile + '-', dir=args.work_root))
        self.source = self.native / 'repo'
        head = SOURCES[args.profile]
        self.report = {'kind': 'offline-test-observation', 'profile': args.profile, 'source': {'head': head},
                       'native': str(self.native), 'steps': [], 'binaries': {}, 'passed': False,
                       'excluded': ['hotspot-specific suites', 'BLE/AirSIM/audio', 'B release/CI/deployment', 'devices/public NKN/cellular'],
                       'security': {k: status[k].strip() for k in ['Uid', 'CapEff', 'CapBnd', 'NoNewPrivs']}}
        self.env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
                        CARGO_TARGET_DIR=str(args.cargo_target.resolve()), TZ='UTC',
                        GOPROXY='off', GOSUMDB='off', GOTOOLCHAIN='local', GOWORK='off', GOENV='off')
        for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_MAKEFLAGS', 'MAKEFLAGS',
                    'NODE_OPTIONS', 'NODE_PATH', 'GOFLAGS', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY',
                    'http_proxy', 'https_proxy', 'all_proxy']:
            self.env.pop(key, None)
        self.save()
        self.run('clone', ['git', 'clone', '--shared', '--no-checkout', str(args.repo.resolve()), str(self.source)], cwd=self.native)
        self.run('checkout', ['git', 'checkout', '--detach', head])
        self.report['source']['tree'] = subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=self.source, text=True).strip()
        subprocess.run(['git', 'merge-base', '--is-ancestor', SOURCES['main'], head], cwd=self.source, check=True)
        toolchain = (self.source / 'rust-toolchain.toml').read_text()
        channel = re.search(r'channel\s*=\s*"([^"]+)"', toolchain).group(1)
        self.env['RUSTUP_TOOLCHAIN'] = channel
        self.report['locks'] = {n: sha(self.source / n) for n in ['Cargo.lock', 'rust-toolchain.toml']}
        self.run('rustc', ['rustc', '--version'])
        self.run('node', [args.node, '--version'])
        self.save()

    def save(self):
        (self.out / 'result.json').write_text(json.dumps(self.report, indent=2) + '\n')

    def run(self, name, argv, tests=False, cwd=None, seconds=600, extra=None):
        assert re.fullmatch(r'[A-Za-z0-9_-]+', name)
        path = self.out / (name + '.log')
        assert not path.exists(), name
        print('START ' + name, flush=True)
        at = time.monotonic()
        env = dict(self.env, **(extra or {}))
        with path.open('wb') as log:
            child = subprocess.Popen(argv, cwd=cwd or self.source, env=env, stdin=subprocess.DEVNULL,
                                     stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = child.wait(timeout=seconds)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
                code = 124
        raw = path.read_text(errors='replace')
        counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', raw)
        nonempty = not tests or any(int(row[0]) > 0 for row in counts)
        step = {'name': name, 'command': argv, 'exit_code': code, 'seconds': round(time.monotonic()-at, 3),
                'test_counts': counts, 'nonempty': nonempty, 'log_sha256': sha(path)}
        self.report['steps'].append(step)
        self.save()
        print('END ' + name + ' ' + str(code), flush=True)
        if code != 0 or not nonempty:
            raise RuntimeError(name + ': failed or selected zero tests; original log retained')
        return raw

    def cargo(self, name, args, tests=False):
        return self.run(name, ['cargo', *args, '--locked', '--offline'], tests=tests)

    def test(self, name, packages, features=None, target=None, selection=None, skip=()):
        argv = ['cargo', 'test']
        for package in packages:
            argv += ['-p', 'rustsimhub-' + package]
        if features:
            argv += ['--features', features]
        if target:
            argv += target
        argv += ['--locked', '--offline']
        if selection:
            argv += [selection]
        argv += ['--', '--nocapture']
        for value in skip:
            argv += ['--skip', value]
        self.run(name, argv, tests=True)

    def build_test(self, key, target, name, features=None):
        argv = ['cargo', 'test', '-p', 'rustsimhub-' + name, *target,
                '--locked', '--offline', '--no-run', '--message-format=json']
        if features:
            argv += ['--features', features]
        raw = self.run('build-' + key, argv)
        rows = [json.loads(line) for line in raw.splitlines() if line.startswith('{')]
        wanted = 'rustsimhub_' + name.replace('-', '_') if target == ['--lib'] else 'rustsimhub-' + name
        binaries = [r['executable'] for r in rows if r.get('reason') == 'compiler-artifact'
                    and r.get('target', {}).get('name') == wanted and r.get('profile', {}).get('test') and r.get('executable')]
        assert len(binaries) == 1, key
        original = Path(binaries[0]); dest = self.native / (key + '-test')
        shutil.copyfile(original, dest); dest.chmod(0o700)
        with dest.open('rb') as stream:
            os.fsync(stream.fileno())
        assert dest.stat().st_size == original.stat().st_size and sha(dest) == sha(original)
        self.report['binaries'][key] = {'path': str(dest), 'sha256': sha(dest)}
        self.save()

    def suite(self):
        profile = self.args.profile
        if profile == 'main':
            packages = ['usage-stats', 'nkn-agent', 'forwarder']
            self.test('business-default', packages)
            self.test('business-production', packages, 'rustsimhub-usage-stats/transaction-candidate,rustsimhub-forwarder/smtp-production-candidate')
            self.test('statistics-original-signer', ['core'], FEATURES, ['--lib'], 'nkn_statistics::tests::')
            self.test('root-transport', ['core'], FEATURES, ['--lib'], 'nkn_root::')
            self.test('auth-network', ['auth'], target=['--lib'], selection='network_manager_is_recent_owner_or_live_approved_client_only')
        elif profile == 'a2':
            packages = ['core', 'edge', 'nkn-agent', 'auth']
            self.test('agent-default', ['nkn-agent'])
            self.test('auth-nkn', ['auth'], 'nkn-session-candidate', ['--lib'], 'nkn_session::tests::')
            self.test('core-default', ['core'], target=['--bin', 'rustsimhub-core'], selection='signed_session_continuation_round_trip_rotates_with_existing_overlap')
            self.test('core-production', ['core'], FEATURES, ['--bin', 'rustsimhub-core'], 'nkn_')
            self.test('root-production', ['core'], FEATURES, ['--lib'], 'nkn_root::')
            self.build_test('core', ['--bin', 'rustsimhub-core'], 'core', FEATURES)
        elif profile == 'a3':
            packages = ['core', 'edge', 'storage', 'compat', 'forwarder']
            self.test('storage', ['storage'])
            self.test('observer-default', ['core'], target=['--bin', 'rustsimhub-core'], selection='observer::')
            self.test('calls-default', ['core'], target=['--bin', 'rustsimhub-core'], selection='calls::checks::')
            self.test('observer-production', ['core'], FEATURES, ['--bin', 'rustsimhub-core'], 'observer::')
            self.test('calls-production', ['core'], FEATURES, ['--bin', 'rustsimhub-core'], 'calls::checks::')
            self.test('forwarder-default', ['forwarder'])
            self.test('forwarder-production', ['forwarder'], 'smtp-production-candidate')
            self.build_test('core', ['--bin', 'rustsimhub-core'], 'core', FEATURES)
            self.build_test('edge', ['--bin', 'rustsimhub-edge'], 'edge')
            self.build_test('storage', ['--lib'], 'storage')
        else:
            packages = ['core', 'wire', 'network-agent', 'status-led', 'auth']
            self.test('wire-hidden', ['wire'], target=['--lib'], selection='hidden_wifi_is_explicit_bounded_and_does_not_change_legacy_commands')
            self.test('ordinary-default', ['core'], target=['--lib'], selection='network_trial::provision::tests::', skip=['recovery','a4_usb_nm'])
            self.test('saved-default', ['core'], target=['--lib'], selection='network_trial::tests::')
            self.test('ipc-disabled', ['core'], target=['--bin', 'rustsimhub-core'], selection='hidden_usb_wifi_real_ipc_advertises_disabled_and_never_bypasses_the_mode_gate')
            self.test('catalogue-auth', ['core'], FEATURES, ['--bin', 'rustsimhub-core'], 'catalogue_needs_owner_or_approved_network_client_and_rechecks_after_read')
            self.test('network-led', ['network-agent', 'status-led'])
            self.build_test('default_core', ['--lib'], 'core')
            self.build_test('core', ['--lib'], 'core', FEATURES)
            self.build_test('endpoint', ['--bin', 'rustsimhub-core'], 'core', FEATURES)
        self.run('format', ['cargo', 'fmt', '--all', '--', '--check'])
        argv = ['cargo', 'clippy']
        for package in packages:
            argv += ['-p', 'rustsimhub-' + package]
        argv += ['--all-targets', '--all-features', '--locked', '--offline', '--', '-D', 'warnings']
        self.run('strict', argv)
        nodes = {'main': ['forwarding', 'forwarding-history', 'usage-stats'],
                 'a2': ['nkn-product', 'nkn-status', 'session', 'operation-receipt', 'console-fencing'],
                 'a3': ['daily-sms', 'sms-management', 'daily-phone', 'call-test', 'operation-receipt', 'console'],
                 'a4': ['network', 'network-policy', 'wifi', 'configuration-state', 'event-sync', 'clients', 'session']}[profile]
        for name in nodes:
            self.run('node-' + name, [self.args.node, 'tools/test-web-' + name + '.mjs'])
        argv = ['cargo', 'check', '-p', 'rustsimhub-core', '--lib', '--bin', 'rustsimhub-core',
                '--features', FEATURES, '--target', 'aarch64-unknown-linux-gnu', '--locked', '--offline']
        self.run('arm-compile-only', argv)
        assert all(sha(self.source / n) == v for n, v in self.report['locks'].items())
        self.report['locks_unchanged'] = True
        self.report['passed'] = True
        self.save()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument('--profile', choices=SOURCES, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--work-root', type=Path, required=True)
    p.add_argument('--cargo-target', type=Path, required=True)
    p.add_argument('--node', default='node')
    args = p.parse_args()
    run = None
    try:
        run = Run(args)
        run.suite()
    except Exception as error:
        if run is not None:
            run.report['error'] = type(error).__name__ + ': ' + str(error)
            run.save()
        raise


if __name__ == '__main__':
    main()
