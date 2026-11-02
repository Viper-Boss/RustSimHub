#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Actual fixed-path Root updater in disposable chroots and private lo.

Only synthetic trust/ledgers are visible. No device, service, production key,
business database or host installation is changed. Reuses the existing Root
fixture's executable/library copier and full file-tree comparison.
"""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('maintenance_fixture', ROOT/'tools/test-maintenance-observation.py')
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
APP = 'var/lib/rustsimhub/rustsimhub-v1/system/app'
TRUST = 'usr/share/rustsimhub/release.public'


def create(root, binary, public):
    root.chmod(0o755)
    fixture.executable(binary, root)
    state = {'format': 1, 'key_sha256': hashlib.sha256(public).hexdigest(),
             'floor': 71, 'confirmed': {'hash': 'a'*64, 'epoch': 71},
             'candidate': None, 'attempted': False}
    fixture.put(root, TRUST, public, 0o444)
    fixture.put(root, APP+'/state.json', json.dumps(state).encode(), 0o600)
    fixture.put(root, APP+'/.update.lock', b'', 0o600)
    # Ensure the non-root --build-info control can execute its ELF loader.
    for path in root.rglob('*'):
        if path.is_dir():
            path.chmod(0o755)
    (root/APP).chmod(0o700)
    return state


def call(root, arguments, uid=0):
    return subprocess.run(
        ['chroot', '--userspec='+str(uid)+':'+str(uid), '--groups='+str(uid),
         str(root), '/updater', *arguments], stdin=subprocess.DEVNULL,
        capture_output=True, timeout=8, env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})


def unchanged(root, arguments, success=False, uid=0):
    before = fixture.tree(root)
    result = call(root, arguments, uid)
    assert fixture.tree(root) == before, 'read/invalid request changed the fixture'
    assert (result.returncode == 0) == success, result.stderr.decode()
    assert len(result.stdout) < 4096 and len(result.stderr) < 4096
    if success:
        value = json.loads(result.stdout)
        assert not result.stderr and value['scope'] == 'application-ledger'
        assert value['read_only'] and value['trust_binding_verified']
        for field in ['authorization_verified', 'health_verified',
                      'application_integrity_verified', 'runtime_selection_verified']:
            assert value[field] is False
        assert value['state']['floor'] == 71 and value['state']['candidate'] is None
    else:
        assert not result.stdout and result.stderr.startswith(b'application update denied: ')
    return result


def pending(root, state):
    state = dict(state, floor=72)
    fixture.put(root, APP+'/state.next', json.dumps(state).encode(), 0o600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--updater', type=Path, required=True)
    binary = parser.parse_args().updater.resolve(strict=True)
    assert os.geteuid() == 0 and socket.if_nameindex() == [(1, 'lo')], 'fresh unshare --net required'
    info = json.loads(subprocess.check_output([str(binary), '--build-info'], timeout=5))
    assert info['read_only_state_inspection_protocol'] == 1
    results = []
    with tempfile.TemporaryDirectory(prefix='rsh-state-inspection-') as temporary:
        base = Path(temporary)
        keys = base/'synthetic-keys'
        keys.mkdir(mode=0o700)
        subprocess.run([str(binary), 'keygen', str(keys)], check=True,
                       capture_output=True, timeout=5)
        public = (keys/'release.public').read_bytes()
        cases = ['ready', 'public-0644', 'pending', 'malformed-next', 'linked-next',
                 'missing-state', 'missing-lock', 'foreign-key', 'malformed-state',
                 'unknown-field', 'writer-busy', 'nonroot', 'invalid-dispatch',
                 'legacy-status-recovery']
        for case in cases:
            root = base/case
            root.mkdir()
            state = create(root, binary, public)
            if case in ['ready', 'public-0644']:
                if case == 'public-0644':
                    (root/TRUST).chmod(0o644)
                unchanged(root, ['inspect-state'], success=True)
                assert not (root/APP/'versions').exists()
            elif case in ['pending', 'malformed-next', 'linked-next']:
                if case == 'pending':
                    pending(root, state)
                elif case == 'malformed-next':
                    fixture.put(root, APP+'/state.next', b'not a ledger', 0o600)
                else:
                    (root/APP/'state.next').symlink_to('absent')
                unchanged(root, ['inspect-state'])
            elif case == 'writer-busy':
                with (root/APP/'.update.lock').open('rb') as writer:
                    fcntl.flock(writer, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    unchanged(root, ['inspect-state'])
                unchanged(root, ['inspect-state'], success=True)
            elif case == 'nonroot':
                control = call(root, ['--build-info'], 65534)
                assert control.returncode == 0, 'non-root ELF execution control failed'
                assert json.loads(control.stdout)['read_only_state_inspection_protocol'] == 1
                refused = unchanged(root, ['inspect-state'], uid=65534)
                assert b'Denied' in refused.stderr
            elif case == 'invalid-dispatch':
                pending(root, state)
                # With the old generic dispatch these valid paths would open
                # a writable Store and promote state.next BEFORE rejection.
                commands = [['unknown', '/'+APP, '/'+TRUST],
                            ['inspect-state', '/'+APP, '/'+TRUST],
                            ['stage', '/'+APP, '/'+TRUST],
                            ['admit-stage', '/'+APP, '/'+TRUST],
                            ['confirm', '/'+APP, '/'+TRUST],
                            ['choose', '/'+APP, '/'+TRUST, 'extra'],
                            ['status', '/'+APP, '/'+TRUST, 'extra'],
                            ['boot-select', '/'+APP, '/'+TRUST, 'incomplete'],
                            ['install-factory', '/'+APP, '/'+TRUST]]
                for arguments in commands:
                    refused = unchanged(root, arguments)
                    assert b'Invalid' in refused.stderr
                assert not (root/APP/'versions').exists()
                assert (root/APP/'state.next').exists()
            elif case == 'legacy-status-recovery':
                pending(root, state)
                result = call(root, ['status', '/'+APP, '/'+TRUST])
                assert result.returncode == 0 and json.loads(result.stdout)['floor_epoch'] == 72
                assert not (root/APP/'state.next').exists()
                assert (root/APP/'versions').is_dir()
                assert json.loads((root/APP/'state.json').read_text())['floor'] == 72
            else:
                if case == 'missing-state':
                    (root/APP/'state.json').unlink()
                elif case == 'missing-lock':
                    (root/APP/'.update.lock').unlink()
                elif case == 'foreign-key':
                    fixture.put(root, TRUST, b'x'*32, 0o444)
                elif case == 'malformed-state':
                    fixture.put(root, APP+'/state.json', b'{}', 0o600)
                else:
                    state['untrusted_health'] = True
                    fixture.put(root, APP+'/state.json', json.dumps(state).encode(), 0o600)
                unchanged(root, ['inspect-state'])
            results.append({'case': case, 'passed': True})
    print(json.dumps({'passed': True, 'actual_root_chroot_cli': True,
                     'read_only_cases_and_legacy_compatibility': results,
                     'invalid_requests_before_store_open': 9,
                     'existing_status_recovery_preserved': True,
                     'device_access': False, 'host_installation_changed': False}))


if __name__ == '__main__':
    main()
