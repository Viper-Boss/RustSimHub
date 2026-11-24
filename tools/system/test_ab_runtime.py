# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
OVERLAY = ROOT / "firmware/system/overlay"
COMPOSE = importlib.util.spec_from_file_location("compose_system_tree", ROOT/"tools/compose-system-tree.py")
compose = importlib.util.module_from_spec(COMPOSE)
sys.path.insert(0, str(ROOT/'tools'))
COMPOSE.loader.exec_module(compose)


class CompositionTests(unittest.TestCase):
    def test_ab_root_uses_the_exact_same_selector_and_enables_verified_mounts(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)
            system = path/"rootfs"
            targets = ("etc/systemd/system/var-lib-rustsimhub.mount", "etc/ssh/sshd_config.d/20-rustsimhub.conf")
            for relative in targets:
                target = system/relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((OVERLAY/relative).read_bytes())
            # The separately exercised ELF verifier is independent of the composition wiring.
            selector = path/"selector"
            selector.write_bytes(b"expected ARM64 selector")
            boot = path/"boot.ab-candidate.img"
            boot.write_bytes(b"verified boot image")
            report = {
                "kind": "internal-ab-boot-candidate", "board_boot_verified": False,
                "flash_allowed": False, "release_ready": False, "initramfs_readback": True,
                "boot_bytes": boot.stat().st_size, "boot_sha256": hashlib.sha256(boot.read_bytes()).hexdigest(),
                "selector_sha256": hashlib.sha256(selector.read_bytes()).hexdigest(),
            }
            (path/"AB-BOOT-CANDIDATE.json").write_text(json.dumps(report))
            with patch.object(compose, "BOOT_BYTES", boot.stat().st_size), patch.object(compose, "check_arm64_selector"), patch.object(compose.os, "chown"):
                selector.write_bytes(b"changed selector")
                with self.assertRaises(AssertionError):
                    compose.install_ab(system, path, selector, {})
                selector.write_bytes(b"expected ARM64 selector")
                metadata = {}
                self.assertEqual(compose.install_ab(system, path, selector, metadata), report)
            self.assertIn("RSH_CONFIG", (system/targets[0]).read_text())
            self.assertIn("/keys/ssh-host/key", (system/targets[1]).read_text())
            self.assertEqual((system/"usr/libexec/rustsimhub/rustsimhub-ab-select").read_bytes(), selector.read_bytes())
            service = system/"etc/systemd/system/rustsimhub-firstboot.service.d/60-ab-storage.conf"
            self.assertIn("Requires=rustsimhub-ab-storage.service", service.read_text())
            timer = system/"etc/systemd/system/timers.target.wants/rustsimhub-ab-health.timer"
            self.assertEqual(os.readlink(timer), "../rustsimhub-ab-health.timer")


class HealthScriptTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.marker = self.root/"rustsimhub-ab-boot"
        self.marker.write_text("B 11 trial\n")
        self.marker.chmod(0o600)
        fixture = self.root/"paths"
        fixture.mkdir()
        (fixture/"dev").write_text("179:17\n")
        source = (OVERLAY/"usr/local/libexec/rustsimhub-firmware-slot").read_text()
        source = source.replace('[ "$(id -u)" = 0 ]', "true")
        source = source.replace('/usr/share/rustsimhub/ab-system.json', str(self.marker))
        source = source.replace('/run/initramfs', str(self.root))
        source = source.replace('= 0:0:600:1', f'= {os.geteuid()}:{os.getegid()}:600:1')
        source = source.replace('/usr/local/libexec/rustsimhub-state-check --read-only', 'true')
        source = source.replace('timeout 4s /opt/rustsimhub/selected/simhubctl identity /run/rustsimhub/core/core.sock 986', 'check_core')
        source = source.replace('sleep 5', 'true').replace('remaining=24', 'remaining=2')
        helpers = f'''
ab_partition() {{ echo '{fixture}'; }}
findmnt() {{ printf '%s\\n' "$DEVICE"; }}
check_core() {{ [ "$HEALTHY" = yes ]; }}
systemctl() {{
    case "$1" in
        is-active) return 0 ;;
        --no-block) echo "$2" >> '{self.root}/events' ;;
        *) return 1 ;;
    esac
}}
'''
        source = source.replace('. /usr/local/libexec/rustsimhub-ab-partitions', helpers)
        selector = self.root/"selector"
        selector.write_text(f'''#!/bin/sh
printf '%s\\n' "$1 $3 $4" >> '{self.root}/events'
case "$1" in
    confirm) echo 'B 12' ;;
    rollback) echo 'B 13' ;;
    check-trial) [ "$PENDING" = yes ] ;;
    *) exit 1 ;;
esac
''')
        selector.chmod(0o755)
        self.script = source.replace('/usr/libexec/rustsimhub/rustsimhub-ab-select', str(selector))

    def run_mode(self, mode, *, healthy="yes", device="179:17", pending="yes"):
        return subprocess.run(["sh", "-c", self.script, "firmware-slot", mode], capture_output=True,
                              text=True, timeout=5, env=dict(os.environ, HEALTHY=healthy, DEVICE=device, PENDING=pending))

    def events(self):
        path = self.root/"events"
        return path.read_text().splitlines() if path.exists() else []

    def test_healthy_slot_confirms_and_can_choose_backup_from_the_same_boot(self):
        result = self.run_mode("health")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.marker.read_text(), "B 12 stable\n")
        self.assertEqual(self.run_mode("rollback").returncode, 0)
        self.assertEqual(self.events(), ["confirm B 11", "rollback B 12"])

    def test_mismatched_running_root_is_never_confirmed(self):
        self.assertNotEqual(self.run_mode("health", device="179:99").returncode, 0)
        self.assertEqual(self.events(), [])

    def test_failed_health_restarts_only_a_trial_still_in_ledger(self):
        self.assertNotEqual(self.run_mode("health", healthy="no", pending="no").returncode, 0)
        self.assertNotEqual(self.run_mode("health", healthy="no").returncode, 0)
        self.assertEqual(self.events(), ["check-trial B 11", "check-trial B 11", "reboot"])


if __name__ == "__main__":
    unittest.main()
