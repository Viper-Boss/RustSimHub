# SPDX-License-Identifier: AGPL-3.0-only
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

OVERLAY = Path(__file__).resolve().parents[2] / "firmware/system/overlay"
SCRIPT = OVERLAY / "usr/local/libexec/rustsimhub-usb-maintenance"


class UsbMaintenanceTests(unittest.TestCase):
    def test_requires_the_kernel_tty_before_dispatch(self):
        result = subprocess.run(["bash", str(SCRIPT)], input="RSHUSB1 IDENTIFY\n",
                                text=True, capture_output=True, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        unit = (OVERLAY / "etc/systemd/system/rustsimhub-usb-maintenance.service").read_text()
        for entry in ("BindsTo=dev-ttyGS0.device", "TTYPath=/dev/ttyGS0", "StandardInput=tty", "StandardOutput=tty"):
            self.assertIn(entry, unit)

    def test_dispatch_is_bounded_and_has_no_session_requirement(self):
        # The subprocess uses pipes in place of the gadget and a temporary directory
        # in place of persistent state. No device or system service is opened.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            script = SCRIPT.read_text()
            for check in ('test "$(id -u)" = 0', 'test "$(tty <&0)" = /dev/ttyGS0',
                          'test /dev/ttyGS0 -ef /proc/self/fd/1', 'stty raw -echo -hupcl <&0',
                          '/usr/local/libexec/rustsimhub-state-check'):
                script = script.replace(check, "true")
            script = script.replace("/var/lib/rustsimhub/rustsimhub-v1/system", str(root))
            script = script.replace("/usr/local/libexec/rustsimhub-ssh-prepare", "/usr/bin/true")
            def run(request, expected):
                fake_systemctl = '''
systemctl() {
    case "$1" in
        show) echo not-found ;;
        start) touch "$system/ssh-started" ;;
        stop) rm -f "$system/ssh-started" ;;
        is-active) [[ -e "$system/ssh-started" ]] ;;
        *) return 1 ;;
    esac
}
'''
                isolated = script.replace("while IFS=", fake_systemctl + "\nwhile IFS=", 1)
                result = subprocess.run(["bash", "-c", isolated], input=request, text=True,
                                        capture_output=True, check=True, timeout=5)
                self.assertEqual([json.loads(line) for line in result.stdout.splitlines()], expected)

            run("RSHUSB1 IDENTIFY\n", [{"ok": True, "protocol": 1, "product": "RustSimHub",
                                        "role": "local-administrator"}])
            run("RSHUSB1 SSH_ON\n", [{"ok": True, "ssh_enabled": True, "ssh_running": True}])
            run("RSHUSB1 SSH_OFF\n", [{"ok": True, "ssh_enabled": False, "ssh_running": False}])
            self.assertTrue((root / "ssh-disabled").is_file())
            run("X" * 120 + "RSHUSB1 SSH_ON\nRSHUSB1 SSH_STATUS\n", [
                {"ok": False, "error": "invalid_request"},
                {"ok": True, "ssh_enabled": False, "ssh_running": False},
            ])
            run("RSHUSB1 SSH_ON extra\n", [{"ok": False, "error": "invalid_request"}])
            run("RSHUSB1 SSH_ON\n", [{"ok": True, "ssh_enabled": True, "ssh_running": True}])
            self.assertFalse((root / "ssh-disabled").exists())


if __name__ == "__main__":
    unittest.main()
