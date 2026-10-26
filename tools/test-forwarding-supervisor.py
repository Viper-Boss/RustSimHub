#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Real Root/systemd/cgroup forwarding test, with synthetic data and lo only.

Require Root, running systemd/cgroup v2 and a fresh `unshare --net`. Only
unique runtime fixture units are created, stopped and removed. No account,
production service, host network, modem or external destination is modified.
With --managed, a temporary polkit rule permits only the existing test UID to
start two unique fixture oneshots, then is removed. Shared polkit infrastructure
may be activated; cleanup does not stop that shared service.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import grp
import pwd
import importlib.util
import json
import multiprocessing
import os
from pathlib import Path
import shutil
import signal
import select
import socket
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('forwarding_process_fixture', ROOT/'tools/test-forwarding-worker-process.py')
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
CORE, EDGE = fixture.CORE, fixture.EDGE
# PID1 credential setup resolves NSS users. Reuse an existing fixture identity;
# production uses its own locked service account, never the nobody account.
WORKER = pwd.getpwnam('nobody').pw_uid
IPC = grp.getgrnam('nogroup').gr_gid


def identity(uid):
    def apply():
        os.setgroups([IPC])
        os.setgid(IPC)
        os.setuid(uid)
    return apply


def credential_guards(base):
    """Valid settings + a positive IPC control: bad metadata must stop before
    any Core connection, not merely fail later on a malformed JSON object.
    The probe socket is NOT the actual Core and never returns a task.
    """
    directory = base/'credential-probe'
    directory.mkdir(mode=0o700)
    os.chown(directory, WORKER, IPC)
    credential = directory/'forwarding.json'
    settings = json.loads((base/'runtime/settings.json').read_text())
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sink:
        path = str(base/'credential-probe.sock')
        sink.bind(path)
        os.chmod(path, 0o666)
        sink.listen(2)
        sink.settimeout(0.15)
        settings['core_socket'] = path
        # No real capability is needed even in this private synthetic probe.
        settings['lease'] = 'a'*64
        encoded = json.dumps(settings)

        def write(mode):
            credential.write_text(encoded)
            os.chown(credential, WORKER, IPC)
            credential.chmod(mode)

        for case in ['writable', 'hardlinked', 'symlink', 'readonly_control']:
            write(0o600 if case == 'writable' else 0o400)
            extra = directory/'extra'
            if case == 'hardlinked':
                os.link(credential, extra)
            elif case == 'symlink':
                credential.rename(extra)
                credential.symlink_to(extra)
            result = subprocess.run([str(base/'forwarder'), '--service-worker'],
                env={'LC_ALL': 'C', 'CREDENTIALS_DIRECTORY': str(directory)},
                preexec_fn=identity(WORKER), stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=8)
            connected = False
            try:
                stream, _ = sink.accept()
                stream.close()
                connected = True
            except socket.timeout:
                pass
            if case == 'readonly_control':
                assert connected, 'valid credential never reached its positive IPC control'
            else:
                assert result.returncode != 0 and not connected, 'unsafe credential reached Core'
            credential.unlink()
            if extra.exists():
                extra.unlink()
    Path(path).unlink()


def test(core_binary, worker_binary, managed=False):
    global CORE
    if managed:
        # Reuse a real, existing non-root NSS identity. Polkit resolves the
        # caller, while the worker still uses the distinct existing nobody.
        account = pwd.getpwuid(core_binary.stat().st_uid)
        assert account.pw_uid not in [0, EDGE, WORKER]
        CORE = account.pw_uid
    assert os.geteuid() == 0 and socket.if_nameindex() == [(1, 'lo')]
    assert Path('/proc/1/comm').read_text().strip() == 'systemd'
    assert Path('/sys/fs/cgroup/cgroup.controllers').exists()
    subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
    name = 'rustsimhub-forwarder-fixture-'+str(uuid.uuid4())+'.service'
    unit = Path('/run/systemd/system')/name
    assert not unit.exists()
    helpers = {action:Path('/run/systemd/system')/(name.removesuffix('.service')+'-'+action+'.service') for action in ['reconcile','status','boot']}
    assert all(not p.exists() for p in helpers.values())
    rule = Path('/etc/polkit-1/rules.d')/('49-rsh-fwd-fixture-'+name.split('fixture-',1)[1]+'.rules') if managed else None
    if rule:
        assert rule.parent.is_dir() and rule.parent.stat().st_uid == 0 and not rule.exists()
    with tempfile.TemporaryDirectory(prefix='rsh-fwd-root-', dir='/run') as temporary:
        base = Path(temporary)
        assert base.parent == Path('/run')
        base.chmod(0o711)
        for filename, origin in [('core', core_binary), ('forwarder', worker_binary),
                                  ('client.py', ROOT/'tools/test-forwarding-worker-ipc.py')]:
            shutil.copyfile(origin, base/filename)
            (base/filename).chmod(0o755)
        if managed:
            client = base/'client.py'
            source = client.read_text()
            old = 'EDGE, CORE, WORKER, OTHER = 65529, 65530, 65531, 65532'
            assert old in source
            client.write_text(source.replace(old, f'EDGE, CORE, WORKER, OTHER = {EDGE}, {CORE}, {WORKER}, 65532').replace('stream.settimeout(3)', 'stream.settimeout(12)'))
        for folder, uid, mode in [('state', CORE, 0o700), ('identity', CORE, 0o700),
                                  ('ipc', CORE, 0o2750), ('runtime', 0, 0o2750), ('persistent', 0, 0o700)]:
            path = base/folder
            path.mkdir()
            os.chown(path, uid, IPC if folder != 'persistent' else 0)
            path.chmod(mode)

        def openssl(*args):
            subprocess.run(['openssl', *args], cwd=base, check=True, capture_output=True, timeout=10)
        openssl('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1', '-subj', '/CN=Disposable root',
                '-keyout', 'ca.key', '-out', 'ca.pem', '-addext', 'basicConstraints=critical,CA:TRUE')
        openssl('req', '-new', '-newkey', 'rsa:2048', '-nodes', '-subj', '/CN=fixture.test',
                '-keyout', 'server.key', '-out', 'server.csr')
        (base/'extensions').write_text('basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:fixture.test\nextendedKeyUsage=serverAuth\n')
        openssl('x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca.key', '-CAcreateserial',
                '-days', '1', '-extfile', 'extensions', '-out', 'server.pem')
        for file in ['ca.key', 'server.key']:
            (base/file).chmod(0o600)
        (base/'ca.pem').chmod(0o644)
        dns = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        dns.bind(('127.0.0.1', 0))
        tcp = socket.socket()
        tcp.bind(('127.0.0.1', 0))
        tcp.listen(4)
        dns_port, tls_port = dns.getsockname()[1], tcp.getsockname()[1]
        mp = multiprocessing.get_context('fork')
        dc, dchild = mp.Pipe()
        tc, tchild = mp.Pipe()
        servers = [mp.Process(target=fixture.dns_server, args=(dns, dchild)),
                   mp.Process(target=fixture.tls_server, args=(tcp, tchild, str(base/'server.pem'), str(base/'server.key'), False, 1 if managed else 0))]
        sock = str(base/'ipc/core.sock')
        context = {'core_socket': sock, 'core_uid': CORE, 'worker_uid': WORKER,
            'worker_gid': IPC, 'worker_group': IPC, 'program': str(base/'forwarder'),
            'persistent': str(base/'persistent'), 'runtime': str(base/'runtime'),
            'network_status': str(base/'network.json'), 'interface': 'lo', 'ca_bundle': str(base/'ca.pem'), 'unit': name}
        (base/'context.json').write_text(json.dumps(context))
        (base/'context.json').chmod(0o600)
        core = stubborn = action = None
        started = []
        events = []
        registered = False
        rule_written = False
        if managed:
            choice = {'reconcile': helpers['reconcile'].name, 'status': helpers['status'].name,
                      'report': str(base/'runtime/status.json')}
            (base/'management.json').write_text(json.dumps(choice))
            os.chown(base/'management.json', 0, IPC)
            (base/'management.json').chmod(0o640)

        def ctl(*args, check=True):
            return subprocess.run(['systemctl', '--no-ask-password', *args], check=check,
                                  capture_output=True, text=True, timeout=22)

        def rpc(uid, command, protocol=4, allow_denied=False):
            request = {'protocol': protocol, 'id': str(uuid.uuid4()), 'command': command}
            result = subprocess.run([sys.executable, str(base/'client.py'), '--client', sock], input=json.dumps(request),
                text=True, capture_output=True, timeout=13 if managed else 6,
                user=uid, group=IPC, extra_groups=[IPC], cwd=base)
            assert result.returncode == 0, 'Core transport failed'
            response = json.loads(result.stdout)
            assert 'error' not in response or (allow_denied and response.get('error') == 'denied'), 'Core refused request'
            return response

        def root(action, check=True):
            result = ctl('start', helpers[action].name, check=False)
            if check:
                assert result.returncode == 0, 'finite Root action failed: '+action
            return result

        def network():
            value = {'schema': 1, 'available': True, 'boot_id': Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
                'sample_unix_seconds': int(time.time()), 'devices': [{'interface': 'lo', 'kind': 'wifi',
                'state': '100 (connected)', 'ipv4': ['127.0.0.1/8'], 'dns_ipv4': [f'127.0.0.1:{dns_port}']}]}
            (base/'network.json').write_text(json.dumps(value))
            (base/'network.json').chmod(0o640)

        def report():
            return json.loads((base/'runtime/status.json').read_text())

        def event_count():
            while tc.poll():
                try:
                    events.append(tc.recv())
                except (EOFError, ConnectionResetError):
                    break
            return max([v.get('count', 0) for v in events] or [0])

        try:
            for server in servers:
                server.start()
                started.append(server)
            dns.close()
            tcp.close()
            dchild.close()
            tchild.close()
            with (base/'core.out').open('wb') as out, (base/'core.err').open('wb') as err:
                (base/'core.out').chmod(0o600)
                (base/'core.err').chmod(0o600)
                arguments = [str(base/'core'), '--forwarding-service-fixture' if managed else '--forwarding-ipc-fixture', sock, str(EDGE), str(WORKER),
                    str(base/'state'), str(base/'identity'), f'https://fixture.test:{tls_port}/synthetic-only']
                if managed:
                    arguments.append(str(base/'management.json'))
                core = subprocess.Popen(arguments,
                    preexec_fn=identity(CORE), stdout=out, stderr=err, cwd=base)

            def ready():
                assert core.poll() is None
                lines = (base/'core.out').read_text().splitlines()
                return json.loads(lines[0]) if lines else None
            owner = fixture.wait_for(ready, 8)['owner_token']
            template = (ROOT/'firmware/system/forwarding/rustsimhub-forwarder.service').read_text()
            lines = [line for line in template.splitlines() if not line.startswith(('Requires=', 'After=', 'BindsTo=', 'PartOf=', 'BindReadOnlyPaths='))]
            template = '\n'.join(lines).replace('/var/lib/rustsimhub/rustsimhub-v1/system/forwarding/enabled.json', str(base/'persistent/enabled.json'))
            template = template.replace('/opt/rustsimhub/selected/rustsimhub-forwarder --service-worker', f'{base}/forwarder --service-worker')
            template = template.replace('User=rustsimhub-forwarder', f'User={WORKER}').replace('Group=rustsimhub-forwarder', 'Group=nogroup')
            template = template.replace('/run/rustsimhub-forwarding', str(base/'runtime'))
            template = template.replace('SupplementaryGroups=rustsimhub-ipc', 'SupplementaryGroups=nogroup')
            template = template.replace('TimeoutStopSec=15', 'TimeoutStopSec=2')
            template += f'\nNetworkNamespacePath=/proc/{os.getpid()}/ns/net\n'
            unit.write_text(template)
            unit.chmod(0o644)
            registered = True
            for kind, helper in helpers.items():
                source = (ROOT/f'firmware/system/forwarding/rustsimhub-forwarder-{kind}.service').read_text()
                source = '\n'.join(line for line in source.splitlines() if not line.startswith(('Requires=', 'After=', 'BindReadOnlyPaths=')))
                action_kind = 'reconcile' if kind == 'boot' else kind
                source = source.replace(f'/opt/rustsimhub/selected/rustsimhub-forwarder --root-{action_kind}', f'{base}/forwarder --root-fixture {action_kind} {base}/context.json')
                source = source.replace('/run/rustsimhub-forwarding', str(base/'runtime')).replace('/var/lib/rustsimhub/rustsimhub-v1/system/forwarding', str(base/'persistent'))
                source = source.replace('SupplementaryGroups=rustsimhub-ipc', 'SupplementaryGroups=nogroup')
                helper.write_text(source)
                helper.chmod(0o644)
            ctl('daemon-reload')
            if managed:
                # Only this UID can start these TWO unique, Root-owned fixture
                # oneshots. No production rule/account/unit is changed.
                grant = 'polkit.addRule(function(action, subject) { if (action.id === "org.freedesktop.systemd1.manage-units" && subject.user === '+json.dumps(account.pw_name)+' && action.lookup("verb") === "start" && '+json.dumps([helpers['reconcile'].name, helpers['status'].name])+'.indexOf(action.lookup("unit")) >= 0) { return polkit.Result.YES; } });\n'
                with rule.open('x') as policy:
                    rule_written = True
                    policy.write(grant)
                rule.chmod(0o644)
                ctl('start', 'polkit.service')  # Shared system infrastructure; never stop it in cleanup.
                time.sleep(1)  # Permit its rule monitor to load only this temporary rule.
            network()
            disabled = rpc(EDGE, {'op': 'set_forwarding_enabled', 'token': owner, 'expected_generation': 1,
                       'request_id': str(uuid.uuid4()), 'enabled': False}, 2)
            if managed:
                assert disabled['process_state_verified'] and disabled['state'] == 'disabled', disabled
            root('reconcile')
            assert report()['state'] == 'disabled' and report()['process_state_verified']
            ctl('start', name)
            assert ctl('show', name, '--property=MainPID', '--value').stdout.strip() == '0'
            assert not (base/'persistent/enabled.json').exists()
            assert rpc(0, {'op': 'state'})['lease_active'] is False
            ctl('start', helpers['boot'].name)
            assert ctl('show', helpers['boot'].name, '--property=ConditionResult', '--value').stdout.strip() == 'no'
            assert ctl('show', helpers['boot'].name, '--property=ExecMainPID', '--value').stdout.strip() == '0'
            enable_id = str(uuid.uuid4())
            enable_command = {'op': 'set_forwarding_enabled', 'token': owner, 'expected_generation': 2,
                              'request_id': enable_id, 'enabled': True}
            if managed:
                # The actual browser editor keeps secrets inside Core. Its
                # typed input must still reach the real worker/TLS endpoint.
                view = rpc(EDGE, {'op': 'forwarding_rules', 'token': owner}, 2)
                assert view['generation'] == 2
                enable_command = {'op': 'forwarding_configure', 'token': owner, 'expected_generation': 2,
                    'request_id': enable_id, 'policy': {'enabled': True,
                        'targets': [{'id': t['id'], 'destination': {'kind': 'keep'}} for t in view['targets']],
                        'rules': view['rules']}}
            enabled = rpc(EDGE, enable_command, 2)
            if managed:
                assert enabled['process_state_verified'] and enabled['state'] == 'running', enabled
                saved = rpc(EDGE, {'op': 'forwarding_request', 'token': owner, 'request_id': enable_id}, 2)
                assert saved['known'] and saved['applied_generation'] == 3 and saved['not_found_is_not_proof']
            root('reconcile')
            running = report()
            assert running['state'] == 'running' and running['process_state_verified']
            ctl('start', helpers['boot'].name)
            assert report()['worker_pid'] == running['worker_pid'], 'startup reconcile restarted healthy worker'
            root('reconcile')
            assert report()['worker_pid'] == running['worker_pid'], 'exact retry restarted healthy worker'
            core.send_signal(signal.SIGUSR1)
            fixture.wait_for(lambda: event_count() == 2, 8)
            fixture.wait_for(lambda: sum(v['state'] == 'confirmed' for v in rpc(EDGE,
                {'op': 'forwarding_receipts', 'token': owner, 'after': '0', 'limit': 32}, 2)['receipts']) == 2, 5)
            if managed:
                test_id = str(uuid.uuid4())
                test_command = {'op': 'forwarding_test', 'token': owner, 'expected_generation': 3,
                    'request_id': test_id, 'target_id': view['targets'][0]['id']}
                test_result = rpc(EDGE, test_command, 2)
                assert test_result['accepted'] and test_result['receipt']['synthetic']
                fixture.wait_for(lambda: event_count() == 3, 8)
                def test_confirmed():
                    receipt = rpc(EDGE, {'op': 'forwarding_test_request', 'token': owner, 'request_id': test_id}, 2)
                    return receipt['known'] and receipt['receipt']['synthetic'] and receipt['receipt']['state'] == 'confirmed'
                fixture.wait_for(test_confirmed, 5)
                assert rpc(EDGE, test_command, 2)['receipt']['state'] == 'confirmed'
            credential_guards(base)
            # A process which ignores SIGTERM is deliberately moved ONLY into
            # this unique owned cgroup. systemd must reap the full group, not
            # merely signal its main PID. Its Popen handle is always reaped.
            stubborn = subprocess.Popen([sys.executable, '-c',
                'import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);print("ready",flush=True);time.sleep(90)'],
                preexec_fn=identity(WORKER), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
            assert select.select([stubborn.stdout], [], [], 3)[0]
            assert stubborn.stdout.readline().strip() == 'ready'
            cgroup = Path('/sys/fs/cgroup/system.slice')/name/'cgroup.procs'
            cgroup.write_text(str(stubborn.pid))
            rejected = root('status', check=False)
            assert rejected.returncode != 0, 'unexpected extra worker was called verified'
            off = {'op': 'set_forwarding_enabled', 'token': owner, 'expected_generation': 3,
                   'request_id': str(uuid.uuid4()), 'enabled': False}
            if managed:
                # The fixture parent must reap its own artificial child while
                # Core waits for the real cgroup proof, like a real child reaper.
                with ThreadPoolExecutor(max_workers=1) as executor:
                    pending = executor.submit(rpc, EDGE, off, 2)
                    assert stubborn.wait(timeout=12) == -signal.SIGKILL
                    stopped = pending.result(timeout=13)
                assert stopped['process_state_verified'] and stopped['state'] == 'disabled', stopped
            else:
                rpc(EDGE, off, 2)
                action = subprocess.Popen(['systemctl', '--no-ask-password', 'start', helpers['reconcile'].name],
                    stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
                assert stubborn.wait(timeout=12) == -signal.SIGKILL
                assert action.wait(timeout=20) == 0
            assert report()['state'] == 'disabled' and report()['process_state_verified']
            assert not Path('/proc/'+str(running['worker_pid'])).exists()
            assert not (base/'persistent/enabled.json').exists()
            assert not (base/'runtime/settings.json').exists()
            assert not Path('/run/credentials', name).exists()
            if managed:
                # The real Owner API must run an OFF-state probe through the
                # actual fixed PID1 jobs/credentials/worker and stop itself.
                # No ordinary switch/rule mutation is used to make this pass.
                network()
                view = rpc(EDGE, {'op': 'forwarding_rules', 'token': owner}, 2)
                assert view['enabled'] is False and view['generation'] == 4
                probe_id = str(uuid.uuid4())
                probe = {'op': 'forwarding_quick_test', 'token': owner, 'expected_generation': 4,
                    'request_id': probe_id, 'target_id': view['targets'][0]['id']}
                admitted = rpc(EDGE, probe, 2)
                assert admitted['accepted'] and admitted['quick_test']
                probe_observation = {}
                def probe_closed():
                    receipt = rpc(EDGE, {'op': 'forwarding_test_request', 'token': owner, 'request_id': probe_id}, 2,
                                  allow_denied=True)
                    # This read shares Core's nonblocking Auth/queue mutexes
                    # with the worker. A refused read is not a final receipt.
                    # Retry only observation within the existing 20s bound;
                    # never retry the start/send/configuration commands.
                    if 'error' in receipt:
                        probe_observation['refused_reads'] = probe_observation.get('refused_reads', 0) + 1
                        return False
                    probe_observation.update(known=receipt['known'], state=(receipt.get('receipt') or {}).get('state'),
                        quick_active=receipt.get('quick_active'), automatic_stop_verified=receipt.get('automatic_stop_verified'))
                    return receipt['known'] and receipt['receipt']['state'] == 'confirmed' and receipt['quick_active'] is False and receipt['automatic_stop_verified'] is True
                fixture.wait_for(probe_closed, 20)
                assert event_count() == 4
                assert rpc(EDGE, probe, 2)['receipt']['state'] == 'confirmed'
                time.sleep(.2)
                assert event_count() == 4
                assert report()['state'] == 'disabled' and report()['process_state_verified']
                assert ctl('show', name, '--property=MainPID', '--value').stdout.strip() == '0'
                assert not (base/'persistent/enabled.json').exists()
                assert not (base/'runtime/settings.json').exists()
                assert not Path('/run/credentials', name).exists()
                assert rpc(EDGE, {'op': 'forwarding_rules', 'token': owner}, 2) == view
            for helper in helpers.values():
                assert ctl('show', helper.name, '--property=MainPID', '--value').stdout.strip() == '0'
            ctl('reset-failed', name, check=False)
            ctl('start', name)  # Re-enqueue startup with OFF persisted: zero worker PID.
            assert ctl('show', name, '--property=MainPID', '--value').stdout.strip() == '0'
            assert rpc(0, {'op': 'state'})['lease_active'] is False
            root('status')
            assert report()['state'] == 'disabled'
            dc.send('stop')
            tc.send('stop')
            assert dc.poll(2) and dc.recv()['dns_queries'] == (4 if managed else 2)
            fixture.wait_for(lambda: fixture.drain_result(tc, events), 3)
            assert next(v for v in events if 'http_requests' in v)['http_requests'] == (4 if managed else 2)
            for server in started:
                server.join(timeout=3)
                assert server.exitcode == 0
        except BaseException:
            if managed and 'probe_observation' in locals():
                # State labels only: no IDs, credentials, message or target.
                print(json.dumps({'last_probe':probe_observation,'synthetic_http_events':event_count()}),file=sys.stderr)
            # Only properties of this disposable unit; no bearer/config/body.
            print(ctl('show', name, '--property=LoadState,ActiveState,Result,MainPID,ControlPID,ControlGroup,UnitFileState,Job', check=False).stdout, file=sys.stderr)
            print(ctl('status', name, '--no-pager', check=False).stdout[-3000:], file=sys.stderr)
            for helper in helpers.values():
                print(ctl('status', helper.name, '--no-pager', check=False).stdout[-2500:], file=sys.stderr)
            raise
        finally:
            # Revoke the temporary privilege first, even if a later stop or
            # daemon-reload fails. Root cleanup does not need this grant.
            if rule_written:
                assert rule.parent == Path('/etc/polkit-1/rules.d') and rule.name.startswith('49-rsh-fwd-fixture-')
                rule.unlink()
            if registered:
                for helper in helpers.values():
                    ctl('stop', helper.name, check=False)
                    ctl('reset-failed', helper.name, check=False)
                ctl('stop', name, check=False)
                ctl('reset-failed', name, check=False)
            if action is not None:
                fixture.stop_process(action)
            fixture.stop_process(stubborn)
            fixture.stop_process(core)
            for server in started:
                if server.is_alive():
                    server.terminate()
                server.join(timeout=3)
                if server.is_alive():
                    server.kill()
                    server.join(timeout=3)
            if registered:
                assert unit.name == name and name.startswith('rustsimhub-forwarder-fixture-')
                unit.unlink()
                for helper in helpers.values():
                    if helper.exists():
                        helper.unlink()
                ctl('daemon-reload')
            for pipe in [dc, tc, dchild, tchild]:
                pipe.close()
            dns.close()
            tcp.close()
    print(json.dumps({'passed': True, 'actual_systemd': True, 'actual_cgroup': True,
        'actual_core_dbus_polkit_coordination': managed,
        'retained_credentials_and_exact_configuration_receipt': managed,
        'temporary_polkit_rule_removed': managed,
        'synthetic_test_once_and_exact_receipt': managed,
        'http_requests': 4 if managed else 2, 'extra_child_forcibly_reaped': True, 'off_startup_job_created_no_worker': True,
        'off_state_quick_probe_and_automatic_stop': managed,
        'credential_metadata_negative_cases': 3, 'credential_ipc_positive_control': True,
        'root_and_service_credentials_removed_on_off': True,
        'boot_oneshot_skipped_when_off': True, 'boot_reconcile_idempotent_when_on': True,
        'physical_coldboot_verified': False, 'production_units_installed': False, 'device_access': False}))


if __name__ == '__main__':
    def deadline(_number, _frame):
        raise RuntimeError('bounded fixture deadline')
    signal.signal(signal.SIGTERM, deadline)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core', required=True, type=Path)
    parser.add_argument('--worker', required=True, type=Path)
    parser.add_argument('--managed', action='store_true', help='Also test actual Core -> fixed PID1 jobs with a temporary, unique-unit polkit rule; existing non-root artifact owner required')
    args = parser.parse_args()
    test(args.core.resolve(strict=True), args.worker.resolve(strict=True), args.managed)
