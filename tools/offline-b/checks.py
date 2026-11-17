# SPDX-License-Identifier: AGPL-3.0-only
"""Reviewed local checks. No device commands, downloads, AP or BLE test discovery."""
from dataclasses import dataclass
import sys


@dataclass(frozen=True)
class Check:
    name: str
    group: str
    argv: tuple
    kind: str
    actions: tuple
    requires: tuple = ()
    seconds: int = 120
    layer: str = 'source-and-synthetic'


CHECKS = []


def python(name, directory, actions, *, linux=False, requires=(), pattern='test_*.py'):
    CHECKS.append(Check(name, 'materials', (sys.executable, '-m', 'unittest', 'discover',
        '-s', directory, '-p', pattern, '-v'), 'unittest', tuple(actions.split()),
        (('linux',) if linux else ()) + requires))


python('profiles', 'tools/product-release', 'UPD-02 UPD-03 UPD-10 REL-01')
python('source-inputs', 'tools/product-integration', 'UPD-03 REL-04 REL-06', requires=('git',))
python('mode-matrix', 'tools/product-integration/matrix', 'UPD-03 UPD-06 UPD-10', linux=True)
python('release-audit', 'tools/release-audit', 'SEC-01 SEC-03 SEC-04 SEC-05 SEC-07 REL-01 REL-05 REL-06',
       linux=True, requires=('git', 'openssl', 'readelf', 'go'))
python('public-scan', 'tools/security-inventory', 'SEC-04 SEC-07', linux=True)
python('dependencies', 'tools/dependencies', 'SEC-05', requires=('git', 'symlink'))
python('kernel-inputs', 'tools/kernel-artifact-audit', 'UPD-07 UPD-08', linux=True)
python('second-board', 'tools/second-board', 'UPD-09 REL-02 REL-03', linux=True)
python('rootfs', 'tools/rootfs/scripts', 'UPD-07 UPD-08 UPD-09 REC-03', linux=True, pattern='test_rootfs.py')
python('recovery-tools', 'tools/recovery', 'REC-03 REC-04 REC-05', linux=True)
CHECKS.append(Check('offline-materials', 'materials', (sys.executable, '-m', 'unittest',
    'tools.offline_delivery.test_delivery', '-v'), 'unittest', ('UPD-02', 'UPD-08', 'UPD-10', 'REL-05'), ('linux',)))
python('performance-tools', 'tools/performance', 'STAB-01 STAB-02 STAB-04 STAB-05 STAB-06', linux=True)

# These are existing synthetic tests of actual web modules, never API stubs
# substituted for production modules. Dedicated AP, media and BLE are excluded.
for name, actions in (
    ('daily-sms', 'REC-04 SEC-02'), ('sms-management', 'REC-04 SEC-02'),
    ('daily-phone', 'SEC-02 SEC-06'), ('call-test', 'SEC-02'),
    ('forwarding', 'SEC-02 SEC-06'), ('forwarding-history', 'REC-04'),
    ('network-policy', 'SEC-02'), ('network', 'SEC-02'),
    ('nas-bridge', 'SEC-02 REC-04'), ('indicator', 'SEC-06'),
    ('usage-stats', 'SEC-02'), ('usage-participation', 'SEC-02'),
    ('nkn-product', 'SEC-02'), ('event-sync', 'SEC-06'),
    ('app-imports', 'REL-01'), ('app-imports-negative', 'SEC-06'),
    ('app-assembly', 'REL-01'),
):
    CHECKS.append(Check('web-' + name, 'business', ('node', 'tools/test-web-' + name + '.mjs'),
                        'node', tuple(actions.split()),
                        ('node', 'browser') if name == 'app-assembly' else ('node',), 60,
                        'real-browser' if name == 'app-assembly' else 'source-and-synthetic'))

for name, manifest, actions in (
    ('install-harness', 'tools/product-install/harness/Cargo.toml', 'UPD-02 UPD-05 SEC-03'),
    ('upgrade-harness', 'tools/product-upgrade/harness/Cargo.toml', 'UPD-06 REC-04 SEC-03'),
):
    CHECKS.append(Check(name, 'process', ('cargo', 'test', '--manifest-path', manifest,
        '--locked', '--offline'), 'rust', tuple(actions.split()), ('linux', 'cargo'), 600))

CHECKS.append(Check('web-app-assembly-node', 'browser',
    ('node', 'tools/offline-b/browser.mjs'), 'node', ('SEC-02', 'SEC-06', 'REL-01'),
    ('node', 'node-playwright', 'edge'), 120, 'real-browser-subset'))

EXCLUDED = {
    'ap': 'Dedicated hotspot/firstboot-ap work deferred by 2026-11-16 instruction; USB/shared Wi-Fi only.',
    'ble': 'BLE/AirSIM/VoBLE/configurator/native audio belong to other windows.',
    'device': 'No device, NAS, carrier, public RPC/chain, production signing or service access authorized.',
    'build': 'Actual ARM profiles need pinned sysroots, SDK stage, approved roles and QEMU; see handoff.',
    'browser': 'B Edge subset is separate from the original Python browser matrix and device/mobile acceptance.',
    'pid1': 'Unit/harness results do not prove PID1/cgroup/nft or physical power-failure behavior.',
    'stability': 'Short local runs do not prove 72-hour device stability or power consumption.',
}

if len({c.name for c in CHECKS}) != len(CHECKS):
    raise ValueError('duplicate reviewed check')
