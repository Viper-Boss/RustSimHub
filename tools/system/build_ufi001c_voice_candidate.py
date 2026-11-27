# SPDX-License-Identifier: AGPL-3.0-only
"""Test the working QDSP6 voice topology on UFI001C without changing its other DT nodes."""

import argparse
import hashlib
from pathlib import Path
import re
import subprocess
import tempfile

from build_ab_boot_candidate import BOOT_BYTES, boot_format, require


SOC = "/soc@0"
SOUND = SOC + "/sound@7702000"
APR = SOC + "/remoteproc@4080000/smd-edge/apr"
PDM = SOC + "/pinctrl@1000000/cdc-pdm"
DIGITAL = SOC + "/audio-codec@771c000"
ANALOG = SOC + "/spmi@200f000/pmic@1/audio-codec@f000"
DAIS = (APR + "/service@4/dais", APR + "/service@7/dais", APR + "/service@8/routing",
        APR + "/apr-service@9/dais", DIGITAL, ANALOG)
PINS = (PDM + "-default-state", PDM + "-sleep-state")
REFERENCE_SHA256 = "afb1027cc0371d491349dba868bdbabb927ac87c71f8a447dc504303f93f7425"


def run(*args):
    return subprocess.run(args, capture_output=True, check=True, timeout=30).stdout.decode().strip()


def get(tree, node, name, kind="s"):
    return run("fdtget", "-t", kind, tree, node, name)


def cells(tree, node, name):
    return [int(x, 16) for x in get(tree, node, name, "x").split()]


def put(tree, node, name, *value, kind="s"):
    run("fdtput", "-t", kind, tree, node, name, *value)


def put_cells(tree, node, name, value):
    put(tree, node, name, *(format(word, "x") for word in value), kind="x")


def enable_voice(tree, reference):
    require(get(tree, "/", "compatible").split()[0] == "thwc,ufi001c", "Expected UFI001C tree")
    require(get(reference, "/", "compatible").split()[0] == "thwc,ufi003", "Expected UFI003 voice reference")
    require(get(reference, SOUND, "compatible") == "qcom,msm8916-qdsp6-sndcard", "Unexpected reference card")
    require(not run("fdtget", "-l", tree, SOUND), "Target already has audio links")
    require(cells(tree, SOUND, "reg") == cells(reference, SOUND, "reg"), "Different sound controller registers")

    source = run("dtc", "-q", "-I", "dtb", "-O", "dts", tree)
    handles = [int(x, 16) for x in re.findall(r"\bphandle = <0x([0-9a-f]+)>;", source)]
    next_handle = max(handles) + 1
    remapping = {}
    for node in DAIS + PINS:
        names = run("fdtget", "-p", tree, node).split()
        if node in DAIS:
            properties = ("compatible", "#sound-dai-cells")
            count, = cells(reference, node, "#sound-dai-cells")
        else:
            properties = ("pins", "function", "drive-strength")
            count = 0
        for prop in properties:
            require(get(reference, node, prop, "bx") == get(tree, node, prop, "bx"), f"Different {node}/{prop}")
        old, = cells(reference, node, "phandle")
        if "phandle" in names:
            new, = cells(tree, node, "phandle")
        else:
            new, next_handle = next_handle, next_handle + 1
            put_cells(tree, node, "phandle", [new])
        remapping[old] = (new, count)

    def remap(words):
        output = []
        offset = 0
        while offset < len(words):
            new, count = remapping[words[offset]]
            require(offset + count < len(words), "Incomplete DAI reference")
            output.extend((new, *words[offset + 1:offset + 1 + count]))
            offset += count + 1
        return output

    for node in (APR, DIGITAL, ANALOG, SOUND):
        require(get(reference, node, "status") == "okay", f"Reference node is disabled: {node}")
        put(tree, node, "status", "okay")
    put(tree, SOUND, "compatible", "qcom,msm8916-qdsp6-sndcard")
    # The signed application worker addresses this shared voice topology by
    # ALSA card ID. Keep that runtime ID while retaining the real board ID.
    put(tree, SOUND, "model", get(reference, SOUND, "model"))
    put(tree, SOUND, "pinctrl-names", "default", "sleep")
    for prop in ("pinctrl-0", "pinctrl-1"):
        put_cells(tree, SOUND, prop, remap(cells(reference, SOUND, prop)))
    for provider, properties, expected_count in ((DAIS[0], ("reg", "qcom,sd-lines"), 2),
                                                  (DAIS[1], ("reg", "direction"), 4)):
        children = run("fdtget", "-l", reference, provider).split()
        require(len(children) == expected_count and not run("fdtget", "-l", tree, provider),
                f"Unexpected DAI children: {provider}")
        for name in reversed(children):
            child = f"{provider}/{name}"
            require(name.startswith("dai@") and not run("fdtget", "-l", reference, child), "Unexpected DAI node")
            require(set(run("fdtget", "-p", reference, child).split()) == set(properties), "Unexpected DAI property")
            run("fdtput", "-c", tree, child)
            for prop in properties:
                put_cells(tree, child, prop, cells(reference, child, prop))
        require(run("fdtget", "-l", tree, provider).split() == children, "DAI order changed")
    links = run("fdtget", "-l", reference, SOUND).split()
    require(len(links) == 10, "Expected 10 voice links")
    # fdtput prepends new nodes. ALSA probes these links in DT order, so
    # Q6ASM frontends must register their widgets before Q6 routing backends.
    for name in reversed(links):
        node = f"{SOUND}/{name}"
        run("fdtput", "-c", tree, node)
        put(tree, node, "link-name", get(reference, node, "link-name"))
        for role in reversed(run("fdtget", "-l", reference, node).split()):
            require(role in ("cpu", "platform", "codec"), "Unexpected link role")
            child = f"{node}/{role}"
            run("fdtput", "-c", tree, child)
            put_cells(tree, child, "sound-dai", remap(cells(reference, child, "sound-dai")))
    require(run("fdtget", "-l", tree, SOUND).split() == links, "Sound link order changed")


def build(boot, reference, output):
    original = boot.read_bytes()
    require(len(original) == BOOT_BYTES, "Expected 64 MiB boot partition image")
    require(hashlib.sha256(reference.read_bytes()).hexdigest() == REFERENCE_SHA256, "Unexpected voice reference digest")
    codec = boot_format()
    parts = codec.parse(original)
    require(not any(original[parts["end"]:]), "Unexpected boot image tail")
    kernel, dtb = codec.split_kernel(parts["kernel"])
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp) / "board.dtb"
        tree.write_bytes(dtb)
        usb = SOC + "/usb@78d9000"
        require(get(tree, usb, "dr_mode") == "peripheral", "Expected the verified UFI001C USB candidate")
        enable_voice(tree, reference)
        require(get(tree, usb, "dr_mode") == "peripheral", "USB role changed")
        updated = tree.read_bytes()
    image = codec.build(parts, kernel + updated)
    require(len(image) <= BOOT_BYTES, "Boot image exceeds the partition")
    parsed = codec.parse(image)
    require(codec.split_kernel(parsed["kernel"]) == (kernel, updated), "Kernel/DTB readback differs")
    require(parsed["ramdisk"] == parts["ramdisk"] and parsed["fields"][12] == parts["fields"][12], "Boot payload changed")
    with output.open("xb") as file:
        file.write(image.ljust(BOOT_BYTES, b"\0"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("boot", "reference", "output"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    build(args.boot, args.reference, args.output)
