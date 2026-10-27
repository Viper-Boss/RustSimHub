#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Isolated check: does the real Core (--serve-device) survive ModemManager
disappearing and coming back? Private dbus-daemon, synthetic ModemManager,
temporary directories only. No hardware, no system bus, no network.

Usage: run_mm_loss.py CORE_BINARY MIGRATE_BINARY FAKE_MM_BINARY REPORT_JSON
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

CORE, MIGRATE, FAKE, REPORT = (Path(p).resolve() for p in sys.argv[1:5])
events = []
started = time.monotonic()


def note(kind, **fields):
    row = {'t': round(time.monotonic() - started, 2), 'event': kind, **fields}
    events.append(row)
    print(json.dumps(row), flush=True)


def main():
    with tempfile.TemporaryDirectory(prefix='rsh-mm-loss-') as raw:
        base = Path(raw)
        base.chmod(0o700)
        dirs = {}
        for name in ('state', 'identity', 'archive', 'access', 'compat', 'journal', 'setup', 'runtime'):
            dirs[name] = base / name
            dirs[name].mkdir(mode=0o700)
        # Same preparation as rustsimhub-core-prepare: initialise the empty history archive.
        subprocess.run([str(MIGRATE), 'init', str(dirs['archive'])], check=True,
                       stdout=subprocess.DEVNULL, timeout=30)
        bus_socket = base / 'bus.sock'
        config = base / 'bus.conf'
        config.write_text(f'''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>system</type><listen>unix:path={bus_socket}</listen><auth>EXTERNAL</auth>
<policy context="default"><allow user="*"/><allow own="*"/><allow send_destination="*"/><allow receive_sender="*"/></policy>
</busconfig>''')
        bus = subprocess.Popen(['dbus-daemon', '--config-file', str(config), '--nofork', '--nopidfile'],
                               stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        for _ in range(100):
            if bus_socket.exists():
                break
            time.sleep(0.05)
        env = {'PATH': '/usr/bin:/bin', 'DBUS_SYSTEM_BUS_ADDRESS': f'unix:path={bus_socket}', 'HOME': str(base)}
        uid = str(os.getuid())
        socket = dirs['runtime'] / 'core.sock'
        core_log = (base / 'core.log').open('w')
        core = subprocess.Popen(
            [str(CORE), '--serve-device', str(socket), uid, str(dirs['state']), str(dirs['identity']),
             str(dirs['runtime'] / 'media.sock'), uid, str(dirs['archive']), str(dirs['access']),
             str(dirs['compat']), str(dirs['journal']), str(dirs['setup'])],
            env=env, stdout=core_log, stderr=subprocess.STDOUT)
        fake_log_path = base / 'fake.log'
        fake = None
        failures = []

        def core_alive(where):
            code = core.poll()
            if code is not None:
                failures.append(f'core exited with {code} during {where}')
                note('core-exited', code=code, during=where)
                return False
            return True

        def probe(where):
            request = json.dumps({'protocol': 2, 'id': 'probe', 'command': {'op': 'identity'}})
            result = subprocess.run([str(CORE), '--request-auth', str(socket)], input=request,
                                    capture_output=True, text=True, timeout=10, env=env)
            try:
                reply = json.loads(result.stdout)
            except ValueError:
                reply = None
            ok = result.returncode == 0 and isinstance(reply, dict) and 'error' not in reply
            note('probe', where=where, ok=ok, keys=sorted(reply)[:8] if isinstance(reply, dict) else None)
            if not ok:
                failures.append(f'identity probe failed during {where}: rc={result.returncode}')
            return ok

        def start_fake():
            nonlocal fake
            fake = subprocess.Popen([str(FAKE)], env=env, stdout=fake_log_path.open('a'), stderr=subprocess.STDOUT)
            note('fake-mm-start', pid=fake.pid)

        def kill_fake(how=signal.SIGKILL):
            fake.send_signal(how)
            fake.wait(timeout=5)
            note('fake-mm-killed', signal=how.name)

        def list_calls_count():
            return fake_log_path.read_text().count('Voice.ListCalls') if fake_log_path.exists() else 0

        def wait_bound(where, timeout=90):
            before = list_calls_count()
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if not core_alive(where):
                    return False
                if list_calls_count() > before:
                    note('line-bound', where=where)
                    return True
                time.sleep(0.25)
            failures.append(f'line never bound during {where}')
            note('bind-timeout', where=where)
            return False

        def hold(seconds, where):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if not core_alive(where):
                    return False
                time.sleep(0.25)
            return True

        try:
            # S0: Core starts while ModemManager is absent.
            if hold(8, 'start-without-mm'):
                probe('start-without-mm')
            # S1..S4: appear, bind, SIGKILL, absence of varying length, reappear.
            for cycle, absence in enumerate((15, 2, 45, 10), 1):
                start_fake()
                if not wait_bound(f'cycle-{cycle}-bind'):
                    break
                kill_fake()
                if not hold(absence, f'cycle-{cycle}-absent-{absence}s'):
                    break
                probe(f'cycle-{cycle}-absent')
            # S5: rapid flapping, killed before or during binding.
            for flap in range(5):
                start_fake()
                time.sleep(0.4)
                kill_fake()
                if not hold(1, f'flap-{flap}'):
                    break
            probe('after-flapping')
            # S6: clean stop while ModemManager is present again.
            start_fake()
            wait_bound('final-bind')
            probe('final')
            if core.poll() is None:
                core.send_signal(signal.SIGTERM)
                try:
                    code = core.wait(timeout=25)
                    note('core-sigterm-exit', code=code)
                    if code != 0:
                        failures.append(f'SIGTERM exit code {code}')
                except subprocess.TimeoutExpired:
                    failures.append('core did not stop within 25 s of SIGTERM')
                    core.kill()
        finally:
            for proc in (fake, core):
                if proc and proc.poll() is None:
                    proc.kill()
                    proc.wait()
            bus.terminate()
            bus.wait()
            core_log.close()
            log_text = (base / 'core.log').read_text(errors='replace')
            fake_text = fake_log_path.read_text(errors='replace') if fake_log_path.exists() else ''
        report = {
            'passed': not failures,
            'failures': failures,
            'core_binary_sha256': subprocess.run(['sha256sum', str(CORE)], capture_output=True, text=True).stdout.split()[0],
            'fake_mm_calls': {k: fake_text.count(k) for k in ('Voice.ListCalls', 'Sim.SimIdentifier', 'Messaging.List', 'CreateCall')},
            'core_log_tail': log_text[-4000:],
            'events': events,
        }
        REPORT.write_text(json.dumps(report, indent=2, ensure_ascii=False))
        print(json.dumps({'passed': report['passed'], 'failures': failures, 'fake_mm_calls': report['fake_mm_calls']}))
        return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
