#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Read-only classic Bluetooth checks for RustSimHub and NasSimHub hosts."""

import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess


def command(name, *args):
    binary = shutil.which(name)
    if binary is None and name == "modinfo" and Path("/usr/sbin/modinfo").is_file():
        binary = "/usr/sbin/modinfo"
    if binary is None:
        return None
    try:
        return subprocess.run(
            [binary, *args],
            capture_output=True,
            text=True,
            errors="replace",
            env={**os.environ, "LC_ALL": "C"},
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None


def settings(text, name):
    for line in text.splitlines():
        key, separator, value = line.strip().partition(":")
        if separator and key == name:
            return value.lower().split()
    return None


def check_controller(name):
    result = command("btmgmt", "--index", name, "info")
    supported = current = None
    if result is not None and result.returncode == 0:
        supported = settings(result.stdout, "supported settings")
        current = settings(result.stdout, "current settings")
    return {
        "controller": name,
        "classic_supported": None if supported is None else "br/edr" in supported,
        "classic_enabled": None if current is None else "br/edr" in current,
        "powered": None if current is None else "powered" in current,
    }


def check_rfcomm():
    if Path("/sys/module/rfcomm").is_dir():
        return "present"
    result = command("modinfo", "-F", "filename", "rfcomm")
    if result is None:
        return "unknown"
    if result.returncode == 0 and result.stdout.strip():
        return "available"
    return "not_found"


def device_model():
    try:
        with Path("/proc/device-tree/model").open("rb") as model:
            return model.read(256).partition(b"\0")[0].decode("utf-8", "replace")
    except OSError:
        return None


def main():
    controllers = sorted(
        path.name
        for path in Path("/sys/class/bluetooth").glob("hci*")
        if re.fullmatch(r"hci[0-9]+", path.name)
    )
    profile = command(
        "busctl", "--system", "--no-pager", "introspect", "org.bluez",
        "/org/bluez", "org.bluez.ProfileManager1",
    )
    # Only capabilities, no Bluetooth addresses, device names or pairing state.
    print(json.dumps({
        "board": device_model(),
        "kernel": platform.release(),
        "architecture": platform.machine(),
        "controllers": [check_controller(name) for name in controllers],
        "rfcomm": check_rfcomm(),
        "bluez_profile_manager": None if profile is None else (
            profile.returncode == 0 and "RegisterProfile" in profile.stdout
        ),
    }, indent=2))


if __name__ == "__main__":
    main()
