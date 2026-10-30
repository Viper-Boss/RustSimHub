#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Only inside a network-disabled ephemeral container; no NSS/account changes."""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
import time

ROOT = Path("/tmp/recovery-uid-fixture")


def drop_to(uid, gid):
    os.setgroups([])
    os.setgid(gid)
    os.setuid(uid)
    os.umask(0o007)


def peer(marker):
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(6)
    connection.connect(str(ROOT / "core.sock"))
    payload = bytes([marker]) if marker in (0x11, 0x12) else b'{"uid":1002,"verified":true,"channel":"ble"}'
    body = b"\x01\x02" + payload
    received = bytearray()
    try:
        connection.sendall(len(body).to_bytes(4, "big") + body)
        while True:
            data = connection.recv(128)
            if not data:
                break
            received.extend(data)
    except (BrokenPipeError, ConnectionResetError):
        pass  # Exact rejection is asserted by the real Core test, not inferred here.
    finally:
        connection.close()
    if marker == 0x11:
        reply = b"\x01\x01fixture-ack"
    elif marker == 0x12:
        reply = b"\x01\x03"
    else:
        reply = b""
    expected = len(reply).to_bytes(4, "big")+reply if reply else b""
    assert received == expected, "exact real framed reply/terminal close required"


def launch_peer(uid, marker):
    return subprocess.Popen([sys.executable, __file__, "peer", str(marker)],
                            preexec_fn=lambda: drop_to(uid, 1002),
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)


def wait_peer(child):
    output = child.communicate(timeout=8)[0]
    if child.returncode:
        raise RuntimeError(output.decode("utf-8", "replace"))


def wait_line(child, expected):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        ready, _, _ = select.select([child.stdout], [], [], max(0, deadline-time.monotonic()))
        if not ready:
            break
        line = child.stdout.readline()
        if not line:
            break
        print(line.decode("utf-8", "replace"), end="", flush=True)
        # libtest prints its test-name prefix without a newline under nocapture.
        if line.strip().endswith(expected.encode()):
            return
    raise RuntimeError(f"Core did not report {expected}")


def run(binary):
    assert os.geteuid() == 0, "isolated container setup requires UID0"
    ROOT.mkdir(mode=0o700)
    os.chown(ROOT, 1001, 1002)
    # Without CAP_FSETID the kernel strips SGID when the setup process is not
    # in the directory's group. Join only this fixture group for chmod, then
    # clear it; do not add a broader capability or mutate any NSS account.
    os.setgroups([1002])
    os.chmod(ROOT, 0o2750)
    os.setgroups([])
    environment = dict(os.environ, RUSTSIMHUB_UID_FIXTURE_DIR=str(ROOT))
    core = subprocess.Popen([binary, "--exact", "isolated_real_uid_source_and_owned_auth_channel", "--nocapture", "--test-threads=1"],
                            preexec_fn=lambda: drop_to(1001, 1001), env=environment,
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, bufsize=0)
    children = []
    try:
        wait_line(core, "UID_READY")
        for uid in (0, 1001, 1003, 1004, 1005, 1006):
            child = launch_peer(uid, 0x99)
            children.append(child)
            wait_peer(child)
        child = launch_peer(1002, 0x99)
        children.append(child)
        wait_peer(child)
        primary = launch_peer(1002, 0x11)
        children.append(primary)
        wait_line(core, "UID_CASE primary_authorized")
        busy = launch_peer(1002, 0x99)
        children.append(busy)
        wait_peer(busy)
        wait_peer(primary)
        fresh = launch_peer(1002, 0x12)
        children.append(fresh)
        wait_peer(fresh)
        output = core.communicate(timeout=8)[0]
        print(output.decode("utf-8", "replace"), end="", flush=True)
        if core.returncode:
            raise RuntimeError("Core fixture failed")
        print(json.dumps({"fixture":"isolated-real-kernel-uids-v1", "core_uid":1001,
                          "ble_uid":1002, "denied_uids":[0,1001,1003,1004,1005,1006],
                          "tests_passed":1, "verifier":"cfg(test)-synthetic",
                          "production_roles_selected":False, "business_dispatch":False}))
    finally:
        for child in children+[core]:
            if child.poll() is None:
                child.kill()
            child.wait(timeout=3)


if __name__ == "__main__":
    if len(sys.argv)==3 and sys.argv[1]=="peer":
        peer(int(sys.argv[2]))
    elif len(sys.argv)==2:
        run(sys.argv[1])
    else:
        raise SystemExit("fixture runner requires the compiled isolated test binary")
