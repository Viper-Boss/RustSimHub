#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit local USB/NAS/16MiB-ENOSPC runs from A5's verified test binaries."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from validate_a5 import Run, SOURCES, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--observation', type=Path, required=True)
    p.add_argument('--mode', choices=['usb', 'nas', 'enospc', 'sms'], required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--go', default='go')
    p.add_argument('--go-cache', type=Path)
    a = p.parse_args()
    assert os.geteuid() != 0 and [n for _, n in socket.if_nameindex()] == ['lo']
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    assert int(status['CapEff'], 16) == 0 and int(status['CapBnd'], 16) == 0 and status['NoNewPrivs'].strip() == '1'
    os.umask(0o077)
    b = json.loads(a.observation.read_text()); assert b['passed']
    expected = 'a4' if a.mode == 'usb' else 'a3'
    assert b['profile'] == expected and b['source']['head'] == SOURCES[expected]
    for row in b['binaries'].values():
        assert sha(Path(row['path'])) == row['sha256'], 'binary changed; rebuild instead of trusting old copy'
    r = Run.__new__(Run); r.out = a.output.resolve(); r.out.mkdir(parents=True, exist_ok=False)
    r.native = Path(b['native']); r.source = r.native / 'repo'
    assert subprocess.check_output(['git','rev-parse','HEAD'], cwd=r.source, text=True).strip() == b['source']['head']
    r.env = dict(os.environ, TZ='UTC', GOPROXY='off', GOSUMDB='off', GOENV='off', GOWORK='off', GOTOOLCHAIN='local')
    for key in ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy','XDG_RUNTIME_DIR']:
        r.env.pop(key, None)
    r.report = {'kind': 'offline-test-observation', 'mode': a.mode, 'source': b['source'], 'steps': [],
                'security': {k: status[k].strip() for k in ['Uid','CapEff','CapBnd','NoNewPrivs']}, 'passed': False}
    r.save()
    children = []; logs = []
    root = Path(tempfile.mkdtemp(prefix='a5-runtime-', dir=r.native)); root.chmod(0o700)
    def start(name, argv):
        f = (r.out/(name+'.log')).open('wb'); logs.append(f)
        child = subprocess.Popen(argv, cwd=r.source, env=r.env, stdin=subprocess.DEVNULL,
                                 stdout=f, stderr=subprocess.STDOUT, start_new_session=True)
        children.append(child); return child
    def wait(path):
        end = time.monotonic()+20
        while not path.exists():
            assert time.monotonic()<end and all(c.poll() is None for c in children), 'fixture startup failed'
            time.sleep(.02)
    try:
        if a.mode == 'usb':
            address = 'unix:path='+str(root/'bus')
            r.env.update(DBUS_SYSTEM_BUS_ADDRESS=address, RSH_A4_USB_NM='1', RSH_A4_BUS_ROOT=str(root))
            start('private-dbus', ['/usr/bin/dbus-daemon','--session','--nofork','--address='+address]); wait(root/'bus')
            cases = ['save','cancel','renamed','lost-save','revoked','no-usb','no-ip','lost-activation']
            cases += ['hidden-'+x for x in cases] + ['hidden-profile-lost-flag']
            for key in ['default_core','core']:
                raw = r.run('usb-'+key,[b['binaries'][key]['path'],'--ignored','--exact',
                    'network_trial::provision::tests::a4_usb_nm::actual_usb_api_and_nm_adapter_preserve_original_principal_and_owned_rollback','--nocapture'],tests=True)
                assert all('case='+case+',' in raw for case in cases)
            r.report['case_count_per_feature_combination'] = len(cases)
        elif a.mode == 'sms':
            r.run('two-clients-restart-revoke',[b['binaries']['core']['path'],
                'authenticated::tests::a3_sms_views::','--nocapture'], tests=True)
        elif a.mode == 'enospc':
            # The original test also verifies the mount namespace, filesystem,
            # exact 16MiB size and fixed temporary prefix before writing.
            assert r.env.get('RSH_A3_ENOSPC') == '1'
            r.run('five-real-enospc-boundaries',[b['binaries']['storage']['path'],'--ignored','--exact',
                'outgoing_recovery_tests::enospc::actual_enospc_at_five_send_boundaries_preserves_original_id_and_permits','--nocapture'],tests=True)
        else:
            r.env.update(GO111MODULE='off', CGO_ENABLED='1', RSH_A3_NAS='1', RSH_A3_NAS_CORE=str(root))
            if a.go_cache:
                r.env['GOCACHE'] = str(a.go_cache.resolve())
            core = start('actual-core',[b['binaries']['core']['path'],'--ignored','--exact','calls::checks::nas_a3::original_core_nas_fixture','--nocapture'])
            wait(root/'credentials.json'); wait(root/'core-request-count')
            edge = start('actual-edge',[b['binaries']['edge']['path'],'--ignored','--exact','linux::tests::a3_nas::original_edge_fixture','--nocapture'])
            wait(root/'edge.json')
            raw = r.run('nas-go-client',[a.go,'test','-race','-count=1','-timeout=45s','-v','-run',
                '^TestA3ActualEdgeCoreAuthControlRevocationAndOriginalOperation$','./tools/nas-adapter/control'])
            assert '--- PASS: TestA3ActualEdgeCoreAuthControlRevocationAndOriginalOperation' in raw
            assert core.wait(timeout=15) == 0 and edge.wait(timeout=15) == 0
            r.report['actual'] = json.loads((root/'core-result.json').read_text())
            r.report['core_forward_count'] = int((root/'core-request-count').read_text())
        r.report['passed'] = True
    except Exception as error:
        r.report['error'] = type(error).__name__+': '+str(error)
        raise
    finally:
        for child in children:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL); child.wait()
        for f in logs:
            f.close()
        r.report['owned_children_stopped'] = all(c.poll() is not None for c in children)
        r.save()


if __name__ == '__main__':
    main()
