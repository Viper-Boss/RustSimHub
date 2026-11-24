# SPDX-License-Identifier: AGPL-3.0-only
"""Check separately built A/B inputs as one offline, unsigned assembly."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat

from storage_layout import MIB, PROFILES

BOOT_BYTES = 64 * MIB
DATA = frozenset(("RSH_RECORDS", "RSH_CONFIG", "RSH_ABUSE", "RSH_KEYS", "RSH_AUDIO"))


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def evidence(directory, filename, kind):
    report = directory / filename
    require(not report.is_symlink() and report.is_file() and report.stat().st_size <= 1 << 20,
            f"Missing or oversized report: {filename}")
    data = json.loads(report.read_text())
    require(data.get("kind") == kind and
            all(data.get(name) is False for name in ("flash_allowed", "release_ready")), "Wrong candidate report")
    return data


def check_binary(path, size, expected):
    absolute = path.absolute()
    info = absolute.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_size == size, f"Wrong file or size: {path.name}")
    require(isinstance(expected, str) and re.fullmatch(r"[a-f0-9]{64}", expected), "Invalid SHA-256")
    with absolute.open("rb") as handle:
        require(hashlib.file_digest(handle, "sha256").hexdigest() == expected, f"Artifact hash differs: {path.name}")
    return {"path": str(absolute), "bytes": size, "sha256": expected}


def verify(boot, a, b, shared, profile):
    require(type(profile) is int and profile in PROFILES, "Unsupported capacity")
    boot_report = evidence(boot, "AB-BOOT-CANDIDATE.json", "internal-ab-boot-candidate")
    require(boot_report.get("initramfs_readback") is True and boot_report.get("arm64_static_selector") is True,
            "Boot candidate was not read back")
    require(re.fullmatch(r"[a-f0-9]{64}", boot_report.get("selector_sha256", "")), "Invalid selector SHA-256")
    images = {"boot": check_binary(boot / "boot.ab-candidate.img", BOOT_BYTES, boot_report["boot_sha256"])}
    reports = []
    for slot, folder in (("A", a), ("B", b)):
        report = evidence(folder, "SYSTEM-IMAGE.json", "internal-unflashable-system-image")
        require(all(report.get(name) is True for name in ("passed", "source_tree_unchanged",
                                                         "filesystem_clean_readonly_check")),
                f"Slot {slot} did not pass the root image check")
        require((report.get("ab_slot"), report.get("ab_capacity_gb"), report.get("filesystem_label"),
                 report.get("ab_boot_sha256"), report.get("ab_selector_sha256")) ==
                (slot, profile, f"RSH_{slot}", boot_report["boot_sha256"], boot_report["selector_sha256"]),
                f"Slot {slot} was built for a different boot candidate, capacity or slot")
        images[f"RSH_{slot}"] = check_binary(folder / "rootfs.img", PROFILES[profile][0] * MIB,
                                            report["image_sha256"])
        reports.append(report)
    for name in ("candidate_manifest_sha256", "tree_sha256", "composition_revision", "application_revision"):
        require(isinstance(reports[0].get(name), str) and reports[0][name] == reports[1].get(name),
                "Both slots must use the same verified system composition")

    shared_report = evidence(shared, "AB-DATA-IMAGES.json", "internal-ab-data-images")
    partitions = shared_report["partitions"]
    require(shared_report["profile_gb"] == profile and set(partitions) == DATA | {"RSH_BOOTCTL"},
            "Shared data layout differs")
    _, records, config, abuse, keys = PROFILES[profile]
    sizes = {"RSH_RECORDS": records, "RSH_CONFIG": config, "RSH_ABUSE": abuse, "RSH_KEYS": keys}
    for label in sorted(DATA):
        desc = partitions[label]
        require(type(desc["bytes"]) is int and desc["bytes"] % MIB == 0, "Unaligned shared filesystem")
        if label in sizes:
            require(desc["bytes"] == sizes[label] * MIB, f"Wrong size for {label}")
        else:
            require(64 * MIB <= desc["bytes"] < profile * 1024**3, "Wrong audio filesystem size")
        images[label] = check_binary(shared / f"{label}.img", desc["bytes"], desc["sha256"])
    return {"kind": "internal-ab-flash-inputs", "profile_gb": profile,
            "composition_revision": reports[0]["composition_revision"], "images": images,
            "spaces": {label: desc["bytes"] for label, desc in images.items()},
            "board_boot_verified": False, "flash_allowed": False, "release_ready": False}


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    for name in ("boot", "slot-a", "slot-b", "shared", "output"):
        cli.add_argument("--"+name, required=True, type=Path)
    cli.add_argument("--profile", required=True, type=int, choices=tuple(PROFILES))
    args = cli.parse_args()
    result = verify(args.boot, args.slot_a, args.slot_b, args.shared, args.profile)
    with args.output.open("x", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2)
        handle.write("\n")


if __name__ == "__main__":
    main()
