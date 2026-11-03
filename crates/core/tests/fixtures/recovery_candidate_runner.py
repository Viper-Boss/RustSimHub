#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""One private broker and three actual UID roles; disposable container only."""
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path('/fixture')
assert os.getuid() == 0 and os.environ.get('RUSTSIMHUB_RECOVERY_CANDIDATE_FIXTURE') == '1'
case = os.environ['RUSTSIMHUB_RECOVERY_CASE']
# The cap-limited controller reads only role-group result files. Children get
# their explicit groups below; BLE never inherits these supplementary groups.
os.setgroups([1002, 1003])
ROOT.chmod(0o1777)
for name, uid in [('core', 1001), ('ble', 1002), ('backend', 1003)]:
    directory = ROOT / name
    directory.mkdir(mode=0o750)
    os.chown(directory, uid, 1002 if name == 'core' else uid)
    # Backend metadata contains only public readiness/name/result values.
    directory.chmod(0o2755 if name == 'backend' else 0o2750)
(ROOT / 'control').mkdir(mode=0o777)
(ROOT / 'control').chmod(0o1777)
recovery = Path('/run/rustsimhub/core-recovery')
recovery.mkdir(parents=True, mode=0o750)
os.chown(recovery, 1001, 1002)
recovery.chmod(0o2750)
address = 'unix:path=/fixture/bus.sock'
config = ROOT / 'bus.conf'
config.write_text('''<busconfig><type>session</type><listen>unix:path=/fixture/bus.sock</listen>
<auth>EXTERNAL</auth><policy context="default"><allow user="*"/><allow own="*"/>
<allow send_destination="*"/><allow receive_sender="*"/></policy></busconfig>''')
config.chmod(0o644)
children = []


def launch(name, uid, command):
    log = (ROOT / (name + '.log')).open('w')
    env = dict(os.environ, DBUS_SYSTEM_BUS_ADDRESS=address, TMPDIR=str(ROOT / ('core' if name == 'bus' else name)))
    process = subprocess.Popen(command, user=uid, group=uid, extra_groups=[1002] if uid == 1003 else [],
                               env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
    children.append((process, uid, log))
    return process


def wait(path, seconds=18):
    end = time.monotonic() + seconds
    while not path.exists():
        assert all(p.poll() in (None, 0) for p, _, _ in children), 'role failed before ' + str(path)
        assert time.monotonic() < end, 'timeout: ' + str(path)
        time.sleep(0.01)


try:
    launch('bus', 1001, ['/input/dbus-daemon', '--nofork', '--nopidfile', '--config-file=/fixture/bus.conf'])
    wait(ROOT / 'bus.sock')
    for name, uid in [('core', 1001), ('backend', 1003), ('ble', 1002)]:
        launch(name, uid, ['/input/recovery-test', '--ignored', '--exact', name + '_role', '--nocapture', '--test-threads=1'])
    proofs = {}
    for name in ('core', 'ble', 'backend'):
        wait(ROOT / name / 'done.json', 25)
        proofs[name] = json.loads((ROOT / name / 'done.json').read_text())
    for process, _, _ in children[1:]:
        assert process.wait(timeout=3) == 0
    assert proofs['core']['actual_auth_and_secure_session'] and proofs['core']['original_network_lock_released']
    assert proofs['ble']['actual_non_test_service'] and proofs['ble']['actual_fixed_core_port']
    assert proofs['backend']['gatt_and_advertisement_unregistered'] and not proofs['backend']['wwan_or_gsm_mutation']
    print(json.dumps({'fixture': 'actual-d-gatt-a-candidate-core-wifi', 'case': case, 'proofs': proofs,
                      'actors_capabilities': [], 'production_roles_installed': False,
                      'external_backends': ['private BlueZ/HCI peer', 'NetworkManager', 'ModemManager'],
                      'core_auth_wifi_actor_or_crypto_mocked': False}, sort_keys=True), flush=True)
except BaseException:
    for path in sorted(ROOT.glob('*.log')):
        print(path.name, path.read_text()[-12000:], flush=True)
    raise
finally:
    for process, uid, log in children:
        if process.poll() is None:
            subprocess.run(['python3', '-c', 'import os,signal,sys;os.kill(int(sys.argv[1]),signal.SIGTERM)', str(process.pid)],
                           user=uid, group=uid, extra_groups=[], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        log.close()
