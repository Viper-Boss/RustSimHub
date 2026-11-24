# SPDX-License-Identifier: AGPL-3.0-only
"""Make private, offline-checked A/B data images from a verified layout report."""

import argparse
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import struct
import subprocess
from tempfile import TemporaryDirectory
import zlib

from storage_layout import MIB, plan_layout

SPEC = importlib.util.spec_from_file_location("generic_state", Path(__file__).with_name("build-generic-state.py"))
generic = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generic)

EPOCH = str(generic.EPOCH)
OWNERS = {"RSH_CONFIG": 0, "RSH_RECORDS": 986, "RSH_KEYS": 0, "RSH_AUDIO": 984, "RSH_ABUSE": 0}
RECORD_BYTES = 4096


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def boot_control(image, size):
    # This fresh-install seed exposes only A until a signed installer has
    # separately written and read back the other slot.
    record = struct.pack("<8sQBBBB8x", b"RSHABCT1", 1, 0, 255, 1, 0)
    page = (record + struct.pack("<I", zlib.crc32(record))).ljust(RECORD_BYTES, bytes(1))
    with image.open("r+b") as handle:
        handle.write(page)
        handle.seek(size // 2)
        handle.write(page)
        handle.flush()
        os.fsync(handle.fileno())
        for offset in (0, size // 2):
            handle.seek(offset)
            if handle.read(RECORD_BYTES) != page:
                raise ValueError("Initial boot ledger readback differs")


def filesystem(image, label, size, key, scratch):
    owner = OWNERS[label]
    env = dict(os.environ, E2FSPROGS_FAKE_TIME=EPOCH, LC_ALL="C")
    subprocess.run(
        ["mke2fs", "-q", "-t", "ext4", "-F", "-b", "4096", "-L", label,
         "-E", f"lazy_itable_init=0,lazy_journal_init=0,root_owner={owner}:{owner}",
         str(image), str(size // 4096)],
        check=True, env=env, timeout=120,
    )
    commands = ["set_inode_field / mode 040700"]
    expected = {"lost+found"}
    if label == "RSH_CONFIG":
        (scratch / "provision.txt").write_bytes(generic.REQUEST)
        paths = ["/rustsimhub-v1", "/rustsimhub-v1/system"]
        for path in paths:
            commands += [f"mkdir {path}", f"set_inode_field {path} mode 040700"]
        request = "/rustsimhub-v1/system/generic-provision.request"
        commands += [f"write provision.txt {request}", f"set_inode_field {request} mode 0100600"]
        expected.update(p.lstrip("/") for p in (*paths, request))
    if label == "RSH_KEYS":
        (scratch / "maintenance.pub").write_bytes(key)
        commands += ["mkdir /ssh-access", "set_inode_field /ssh-access mode 040700",
                     "write maintenance.pub /ssh-access/authorized_keys",
                     "set_inode_field /ssh-access/authorized_keys mode 0100600"]
        expected.update(("ssh-access", "ssh-access/authorized_keys"))
    (scratch / "edits.txt").write_text("\n".join(commands) + "\n")
    subprocess.run(["debugfs", "-w", "-f", "edits.txt", str(image)], cwd=scratch, env=env,
                   capture_output=True, check=True, timeout=60)
    subprocess.run(["e2fsck", "-fn", str(image)], check=True, env=env, capture_output=True, timeout=60)
    fields = dict(line.split("=", 1) for line in subprocess.check_output(
        ["blkid", "-p", "-o", "export", str(image)], env=env, text=True, timeout=20
    ).splitlines() if "=" in line)
    if fields.get("TYPE") != "ext4" or fields.get("LABEL") != label:
        raise ValueError("Data filesystem identity differs")
    super_root = subprocess.check_output(
        ["debugfs", "-R", "stat /", str(image)], env=env, text=True, timeout=20,
        stderr=subprocess.DEVNULL,
    )
    if not re.search(r"(?m)Type: directory\s+Mode:\s+0700\b", super_root) or not re.search(
        rf"(?m)User:\s+{owner}\s+Group:\s+{owner}\b", super_root
    ):
        raise ValueError("Expected a privately owned filesystem root")
    rows = generic.tree(image, env)
    if set(rows) != expected:
        raise ValueError("Unexpected contents in new shared partition")
    for path, row in rows.items():
        if (row["uid"], row["gid"]) != (0, 0) or row["mode"] != ("0o700" if row["directory"] else "0o600"):
            raise ValueError(f"Unexpected owner or mode on {label}/{path}")
    if label in ("RSH_CONFIG", "RSH_KEYS"):
        member, contents = (("/rustsimhub-v1/system/generic-provision.request", generic.REQUEST)
                            if label == "RSH_CONFIG" else ("/ssh-access/authorized_keys", key))
        extracted = subprocess.run(["debugfs", "-R", f"cat {member}", str(image)], check=True,
                                   timeout=30, capture_output=True, env=env).stdout
        if extracted != contents:
            raise ValueError("First-boot input readback differs")
    return fields["UUID"]


def build(layout, board, key_path, key_digest, output):
    plan = plan_layout(layout, board)
    key = generic.read_public_key(key_path, key_digest)
    output = Path(output).absolute()
    output.mkdir(mode=0o700)
    result = {"kind": "internal-ab-data-images", "profile_gb": plan["profile_gb"], "device_writes": 0,
              "flash_allowed": False, "release_ready": False, "partitions": {}}
    with TemporaryDirectory(dir=output) as scratch:
        for row in plan["new_partitions"]:
            label, size = row["name"], row["bytes"]
            if label in ("RSH_A", "RSH_B"):
                continue
            if label == "RSH_AUDIO":
                # A common seed fits every accepted geometry and is expanded at first boot.
                size = 64 * MIB
            image = output / f"{label}.img"
            with image.open("xb") as handle:
                os.fchmod(handle.fileno(), 0o600)
                handle.truncate(size)
            if label == "RSH_BOOTCTL":
                boot_control(image, size)
                uuid = None
            else:
                uuid = filesystem(image, label, size, key, Path(scratch))
            if image.stat().st_size != size:
                raise ValueError("Prepared image size differs from the partition plan")
            result["partitions"][label] = {"bytes": size, "sha256": digest(image), "uuid": uuid}
    (output/"AB-DATA-IMAGES.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--layout-readback", type=Path, required=True, help="Normalized read-only UFI003 GPT geometry")
    parser.add_argument("--maintenance-public-key", type=Path, required=True)
    parser.add_argument("--expected-public-sha256", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    layout = json.loads(args.layout_readback.read_text())
    print(json.dumps(build(layout, "UFI003", args.maintenance_public_key, args.expected_public_sha256, args.out), indent=2))


if __name__ == "__main__":
    main()
