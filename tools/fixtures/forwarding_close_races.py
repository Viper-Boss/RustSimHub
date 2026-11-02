# SPDX-License-Identifier: AGPL-3.0-only
"""Actual Core/worker/PID1 close checks, imported by the supervisor fixture.

Uses its existing disposable identities, units, private loopback and real
production handlers. Only DNS/TLS/HTTP peers are synthetic. No second queue,
controller or supervisor implementation lives here.
"""
from pathlib import Path
import signal
import time
import uuid


def verify(phase, base, unit, helpers, core, running, owner, edge, worker,
           dns, tls, servers, rpc, ctl, report, network, wait_for):
    events = []

    def receipts():
        response = rpc(edge, {'op': 'forwarding_receipts', 'token': owner,
                             'after': '0', 'limit': 32}, 2, allow_denied=True)
        return None if 'error' in response else response['receipts']

    def snapshot():
        rows = receipts()
        return {'rows': rows} if rows is not None else None

    before = wait_for(snapshot, 3)['rows']
    before = {row['id']: row for row in before}
    core.send_signal(signal.SIGUSR1)
    control = dns if phase == 'dns' else tls
    entered = {'dns': 'dns_waiting', 'tls': 'tls_waiting',
               'http': 'posted', 'late_ack': 'posted'}[phase]

    def event(kind):
        while control.poll():
            events.append(control.recv())
        return any(row.get('event') == kind for row in events)

    wait_for(lambda: event(entered), 8)

    def claimed_pair():
        rows = receipts()
        if rows is None:
            return None
        new = [row for row in rows if row['id'] not in before]
        return new if len(new) == 2 and sorted(row['state'] for row in new) == ['in_flight', 'queued'] else None

    pair = wait_for(claimed_pair, 3)
    states = {row['id']: row['state'] for row in pair}
    stop_id = str(uuid.uuid4())
    off = {'op': 'set_forwarding_enabled', 'token': owner,
           'expected_generation': 3, 'request_id': stop_id, 'enabled': False}
    started = time.monotonic()
    stopped = rpc(edge, off, 2)
    stop_seconds = time.monotonic() - started
    assert stopped['state'] == 'disabled' and stopped['process_state_verified']
    assert stop_seconds < 8, 'Owner OFF exceeded its existing budget'
    assert report()['state'] == 'disabled' and report()['process_state_verified']
    assert not Path('/proc', str(running['worker_pid'])).exists()
    assert ctl('show', unit, '--property=MainPID', '--value').stdout.strip() == '0'
    group = Path('/sys/fs/cgroup/system.slice')/unit/'cgroup.procs'
    assert not group.exists() or not group.read_text().strip()
    for path in [base/'persistent/enabled.json', base/'runtime/settings.json',
                 base/'runtime/worker.json', Path('/run/credentials')/unit]:
        assert not path.exists(), 'OFF retained a worker marker or credential'
    assert rpc(0, {'op': 'state'})['lease_active'] is False

    def after_close():
        rows = receipts()
        if rows is None:
            return None
        by_id = {row['id']: row for row in rows}
        assert set(by_id) == set(before) | set(states), 'close created or lost an unrelated receipt'
        assert all(by_id[key] == value for key, value in before.items())
        for key, state in states.items():
            assert by_id[key]['state'] == ('unknown' if state == 'in_flight' else 'cancelled')
        return by_id

    settled = wait_for(after_close, 3)
    control.send('release')
    if phase in ('http', 'late_ack'):
        wait_for(lambda: event('late_ack_attempted' if phase == 'late_ack' else 'peer_closed'), 3)
    # A late DNS answer/TLS attempt/2xx must not resurrect the worker or its
    # durable result. Replay is ONLY the identical OFF configuration request.
    time.sleep(.2)
    assert wait_for(after_close, 3) == settled
    repeated = rpc(edge, off, 2)
    assert repeated['state'] == 'disabled' and repeated['process_state_verified']
    receipt = rpc(edge, {'op': 'forwarding_request', 'token': owner,
                         'request_id': stop_id}, 2)
    assert receipt['known'] and receipt['applied_generation'] == 4
    network()
    ctl('start', helpers['boot'].name)
    assert ctl('show', helpers['boot'].name, '--property=ConditionResult', '--value').stdout.strip() == 'no'
    ctl('start', unit)
    assert ctl('show', unit, '--property=MainPID', '--value').stdout.strip() == '0'
    assert rpc(0, {'op': 'state'})['lease_active'] is False
    assert wait_for(after_close, 3) == settled

    # Only these disposable listeners are stopped. The caller's finally also
    # removes its unique units/rule and reaps all children on every failure.
    dns.send('stop')
    tls.send('stop')
    assert dns.poll(3)
    assert dns.recv()['dns_queries'] == 1
    tls_events = events if control is tls else []

    def final_count():
        while tls.poll():
            tls_events.append(tls.recv())
        return next((row for row in tls_events if 'http_requests' in row), None)

    result = wait_for(final_count, 3)
    assert result['http_requests'] == (1 if phase in ('http', 'late_ack') else 0)
    for server in servers:
        server.join(timeout=3)
        assert server.exitcode == 0
    return {'passed': True, 'close_at': phase, 'actual_systemd': True,
            'actual_cgroup': True, 'actual_owner_core_dbus_polkit_stop': True,
            'two_real_queued_jobs': True, 'claimed_became_unknown': True,
            'pending_became_cancelled': True, 'prior_receipts_unchanged': True,
            'http_requests': result['http_requests'], 'dns_queries': 1,
            'off_request_replayed_without_resend': True,
            'late_response_did_not_confirm': phase == 'late_ack',
            'worker_pid_zero_and_credentials_removed': True,
            'off_startup_created_no_worker': True,
            'stop_seconds': round(stop_seconds, 3),
            'device_access': False, 'physical_coldboot_verified': False}
