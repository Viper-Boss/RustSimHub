# SPDX-License-Identifier: AGPL-3.0-only
"""Build a UFI001C A/B boot candidate with a fixed USB peripheral role."""

import argparse
import hashlib
from pathlib import Path
import subprocess
import tempfile

from build_ab_boot_candidate import BOOT_BYTES, boot_format, cpio_entry, require


SELECT = b'if ! ROOT=$(/usr/bin/rustsimhub-ab-root); then\n'
SELECTED = b'parse_numeric "${ROOT}"\nmaybe_break mountroot'
USB = "/soc@0/usb@78d9000"
POWER_LED = "/leds/led-r"
LED_HELPER = b'''rsh_boot_led() {
    led=/sys/class/leds/$1/brightness
    if [ -w "$led" ]; then
        printf '%s\\n' "$2" > "$led" || :
    fi
}
rsh_boot_led blue:wan 1
echo "RustSimHub: selecting A/B root" >&2
'''
LED_SELECTED = b'''rsh_boot_led blue:wan 0
rsh_boot_led green:wlan 1
echo "RustSimHub: selected A/B root $ROOT" >&2
'''


def patch_init(archive):
    offset = 0
    while offset + 110 <= len(archive):
        require(archive[offset:offset + 6] == b"070701", "Unexpected initramfs CPIO format")
        fields = [int(archive[i:i + 8], 16) for i in range(offset + 6, offset + 110, 8)]
        name_start = offset + 110
        name_end = name_start + fields[11]
        name = archive[name_start:name_end]
        payload = (name_end + 3) & ~3
        end = (payload + fields[6] + 3) & ~3
        require(end <= len(archive) and name.endswith(b"\0"), "Invalid initramfs entry")
        if name in (b"init\0", b"./init\0"):
            script = archive[payload:payload + fields[6]]
            require(script.count(SELECT) == 1 and script.count(SELECTED) == 1, "Expected the original A/B init")
            script = script.replace(SELECT, LED_HELPER + SELECT).replace(SELECTED, LED_SELECTED + SELECTED)
            entry = cpio_entry(fields, name[:-1], script)
            return archive[:offset] + entry + archive[end:], script
        if name == b"TRAILER!!!\0":
            break
        offset = end
    raise ValueError("Missing initramfs init")


def run(*command, data=None):
    return subprocess.run(command, input=data, capture_output=True, check=True, timeout=120).stdout


def peripheral_tree(source):
    with tempfile.TemporaryDirectory() as scratch:
        path = Path(scratch) / "board.dtb"
        path.write_bytes(source)
        compatible = run("fdtget", "-t", "s", path, "/", "compatible").split()
        require(compatible and compatible[0] == b"thwc,ufi001c", "Expected UFI001C device tree")
        require(run("fdtget", "-t", "s", path, USB, "dr_mode").strip() == b"otg", "Expected OTG source tree")
        run("fdtput", "-t", "s", path, USB, "dr_mode", "peripheral")
        run("fdtput", "-d", path, USB, "usb-role-switch")
        run("fdtput", "-t", "s", path, POWER_LED, "linux,default-trigger", "heartbeat")
        require(run("fdtget", "-t", "s", path, USB, "dr_mode").strip() == b"peripheral", "USB role verification failed")
        require(run("fdtget", "-t", "s", path, POWER_LED, "linux,default-trigger").strip() == b"heartbeat", "LED trigger verification failed")
        return path.read_bytes()


def build(source, output):
    original = source.read_bytes()
    require(len(original) == BOOT_BYTES, "Expected a padded 64 MiB boot image")
    codec = boot_format()
    parts = codec.parse(original)
    require(not any(original[parts["end"]:]), "Unexpected boot image tail")
    kernel, dtb = codec.split_kernel(parts["kernel"])
    new_dtb = peripheral_tree(dtb)
    archive = run("zstd", "-dqc", data=parts["ramdisk"])
    archive, script = patch_init(archive)
    run("sh", "-n", data=script)
    parts["ramdisk"] = run("zstd", "-qc", "-19", "--threads=1", data=archive)
    parts["fields"][3] = len(parts["ramdisk"])
    result = codec.build(parts, kernel + new_dtb)
    require(len(result) <= BOOT_BYTES, "Boot image exceeds the partition")
    result = result.ljust(BOOT_BYTES, b"\0")
    checked = codec.parse(result)
    require(codec.split_kernel(checked["kernel"]) == (kernel, new_dtb), "Kernel or DTB readback differs")
    require(checked["fields"][12] == parts["fields"][12], "Kernel command line changed")
    checked_archive = run("zstd", "-dqc", data=checked["ramdisk"])
    require(run("cpio", "-i", "--to-stdout", "--quiet", "init", data=checked_archive) == script, "Init script readback differs")
    with output.open("xb") as handle:
        handle.write(result)
    require(output.read_bytes() == result, "Boot image file readback differs")
    return hashlib.sha256(result).hexdigest()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    print(build(args.source, args.output))
