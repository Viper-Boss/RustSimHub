# SPDX-License-Identifier: AGPL-3.0-only
"""Build an internal UFI003 A/B boot prototype. Never opens or writes a device."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import struct
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BOOT_BYTES = 64 * 1024 * 1024
RAMDISK_LIMIT = 64 * 1024 * 1024
ANCHOR = b'parse_numeric "${ROOT}"\nmaybe_break mountroot'
HOOK = b'''if ! ROOT=$(/usr/bin/rustsimhub-ab-root); then
    panic "RustSimHub A/B root device unavailable"
    exit 1
fi
''' + ANCHOR


def require(condition, message):
    if not condition:
        raise ValueError(message)


def boot_format():
    source = ROOT / "tools/rootfs/kernel/audio/replace-appended-dtb.py"
    spec = importlib.util.spec_from_file_location("android_boot_v0", source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(data):
    return hashlib.sha256(data).hexdigest()


def add_padding(blob):
    return blob + b"\0" * (-len(blob) % 4)


def cpio_entry(fields, name, content):
    values = list(fields)
    values[6] = len(content)
    values[11] = len(name) + 1
    header = b"070701" + b"".join(f"{value:08x}".encode() for value in values)
    return add_padding(header + name + b"\0") + add_padding(content)


def add_entry(name, data, inode):
    values = (inode, stat.S_IFREG | 0o755, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    return cpio_entry(values, name, data)


def patch_initramfs(data, selector, hook_script):
    offset, patched = 0, False
    records = bytearray()
    max_inode = 0
    extra = {b"usr/bin/rustsimhub-ab-select": selector, b"usr/bin/rustsimhub-ab-root": hook_script}
    while offset < len(data):
        require(data[offset:offset + 6] == b"070701", "Unexpected CPIO entry")
        require(offset + 110 <= len(data), "Incomplete CPIO header")
        fields = [int(data[i:i + 8], 16) for i in range(offset + 6, offset + 110, 8)]
        inode, size, namesize = fields[0], fields[6], fields[11]
        bare = offset + 110
        name = data[bare:bare + namesize]
        require(name.endswith(b"\0") and b"\0" not in name[:-1], "Invalid CPIO name")
        first = (bare + namesize + 3) & ~3
        end = (first + size + 3) & ~3
        require(first <= end <= len(data), "Invalid CPIO range")
        canonical_name = name[:-1].removeprefix(b"./")
        require(canonical_name not in extra, "Selector is already present")
        if canonical_name == b"TRAILER!!!":
            require(patched and not any(data[end:]), "Missing init or unexpected trailer")
            for i, (path, content) in enumerate(extra.items(), max_inode + 1):
                records.extend(add_entry(path, content, i))
            records.extend(data[offset:end])
            return bytes(records)
        if canonical_name == b"init":
            init = data[first:first + size]
            require(not patched and stat.S_ISREG(fields[1]) and init.count(ANCHOR) == 1, "Unexpected init hook location")
            records.extend(cpio_entry(fields, name[:-1], init.replace(ANCHOR, HOOK)))
            patched = True
        else:
            records.extend(data[offset:end])
        offset = end
        max_inode = max(max_inode, inode)
    raise ValueError("Missing CPIO trailer")


def check_arm64_selector(data):
    require(len(data) >= 64 and data[:6] == b"\x7fELF\x02\x01", "Expected little-endian ELF64 selector")
    _, machine = struct.unpack_from("<HH", data, 16)
    offset = struct.unpack_from("<Q", data, 32)[0]
    size, count = struct.unpack_from("<HH", data, 54)
    require(machine == 183 and size == 56 and 0 < count <= 32 and offset + size * count <= len(data),
            "Expected an ARM64 program header table")
    require(all(struct.unpack_from("<I", data, offset + size * i)[0] != 3 for i in range(count)),
            "Selector needs a runtime linker")


def build(source, selector_path, output):
    board = tomllib.loads((ROOT / "firmware/board.toml").read_text())
    require(not source.is_symlink() and source.is_file() and source.stat().st_size == BOOT_BYTES,
            "Expected the fixed 64 MiB UFI003 boot source")
    old = source.read_bytes()
    require(sha(old) == board["immutable_content"]["boot_sha256"], "Boot source differs from the lock")
    require(not selector_path.is_symlink() and selector_path.is_file() and selector_path.stat().st_size <= 2**20,
            "Selector must be a bounded program file")
    selector = selector_path.read_bytes()
    check_arm64_selector(selector)
    hook = (ROOT / "native/ab-boot-control/root-device.sh").read_bytes()
    format = boot_format()
    boot = format.parse(old)
    require(not any(old[boot["end"]:]) and boot["ramdisk"].startswith(b"\x28\xb5\x2f\xfd"),
            "Unrecognized padded boot image")
    initramfs = subprocess.run(["zstd", "-dq", "-c"], input=boot["ramdisk"],
                               check=True, capture_output=True, timeout=60).stdout
    require(len(initramfs) <= RAMDISK_LIMIT, "Initramfs exceeds its budget")
    new_archive = patch_initramfs(initramfs, selector, hook)
    new_ramdisk = subprocess.run(["zstd", "-q", "-c", "-19", "--threads=1"], input=new_archive,
                                 check=True, capture_output=True, timeout=180).stdout
    boot["ramdisk"] = new_ramdisk
    boot["fields"][3] = len(new_ramdisk)
    cmdline = boot["fields"][12].split(b"\0", 1)[0]
    require(cmdline.split().count(b"root=LABEL=rootfs") == 1 and b"panic=" not in cmdline and
            not any(boot["fields"][14]), "Unexpected old kernel command line")
    cmdline = cmdline.replace(b"root=LABEL=rootfs", b"root=/dev/rustsimhub-ab-root") + b" panic=10"
    require(len(cmdline) < 512, "Kernel command line too long")
    boot["fields"][12] = cmdline.ljust(512, b"\0")
    image = format.build(boot, boot["kernel"])
    check = format.parse(image)
    require(len(image) <= BOOT_BYTES and check["kernel"] == boot["kernel"] and
            check["ramdisk"] == new_ramdisk and check["fields"][12] == boot["fields"][12],
            "A/B boot boot-image readback differs")
    output.mkdir(mode=0o700)
    image_path = output / "boot.ab-candidate.img"
    with image_path.open("xb") as handle:
        handle.write(image)
        handle.truncate(BOOT_BYTES)
    stored = image_path.read_bytes()
    stored_parts = format.parse(stored)
    stored_archive = subprocess.run(["zstd", "-dq", "-c"], input=stored_parts["ramdisk"],
                                    check=True, capture_output=True, timeout=60).stdout
    require(len(stored) == BOOT_BYTES and stored[:len(image)] == image and stored_archive == new_archive,
            "Written boot candidate did not pass readback")
    report = {
        "kind": "internal-ab-boot-candidate",
        "source_boot_sha256": sha(old),
        "selector_sha256": sha(selector),
        "boot_sha256": sha(stored),
        "boot_bytes": BOOT_BYTES,
        "initramfs_readback": True,
        "arm64_static_selector": True,
        "device_writes": 0,
        "board_boot_verified": False,
        "flash_allowed": False,
        "release_ready": False,
    }
    (output / "AB-BOOT-CANDIDATE.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-boot", type=Path, required=True)
    parser.add_argument("--selector", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.source_boot, args.selector, args.out), indent=2))


if __name__ == "__main__":
    main()
