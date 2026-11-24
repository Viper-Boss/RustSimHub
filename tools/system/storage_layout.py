# SPDX-License-Identifier: AGPL-3.0-only OR GPL-3.0-only
"""Read-only UFI003 manufacturing layout planner, shared with RustFlashKit."""

SECTOR_BYTES = 512
MIB = 1024 * 1024
ALIGN = MIB // SECTOR_BYTES
PROFILES = {
    4: (1328, 64, 64, 32, 32),
    8: (1792, 256, 128, 128, 64),
}
NATIVE = frozenset(("rootfs", "NSHSTATE"))
STOCK = frozenset(("system", "cache", "recovery", "userdata"))
TARGET_TYPES = ("RSH_A", "RSH_B", "RSH_RECORDS", "RSH_CONFIG", "RSH_ABUSE", "RSH_KEYS", "RSH_BOOTCTL")
TARGET = frozenset((*TARGET_TYPES, "RSH_AUDIO"))
REQUIRED = frozenset(("aboot", "boot", "sbl1", "tz", "rpm", "modemst1", "modemst2", "fsg"))
FIXED = frozenset(("cdt", "sbl1bak", "abootbak", "rpmbak", "tzbak", "hyp", "hypbak", "sec",
                   "fsc", "modem", "devinfo", "pad", "misc", "ssd", "splash", "DDR", "persist")) | REQUIRED


class LayoutError(ValueError):
    pass


def capacity_profile(disk_bytes):
    if type(disk_bytes) is not int:
        raise LayoutError("无有效的 eMMC 容量")
    for size in PROFILES:
        if size * 875_000_000 <= disk_bytes <= size * 1024**3:
            return size
    raise LayoutError("容量不属于已支持的 4 GB 或 8 GB 档")


def validate(layout):
    sectors = layout["disk_sectors"]
    first, last = layout["first_usable"], layout["last_usable"]
    count = layout["entry_count"]
    if not (34 <= first <= last < sectors - 33 and 0 < count <= 128 and layout["disk_bytes"] == sectors * SECTOR_BYTES):
        raise LayoutError("GPT 可用范围或容量不一致")
    partitions = sorted(layout["partitions"], key=lambda p: p["first_sector"])
    names, indexes = set(), set()
    for row in partitions:
        name, index = row["name"], row["index"]
        start, end = row["first_sector"], row["last_sector"]
        if name in names or index in indexes or not (1 <= index <= count and first <= start <= end <= last):
            raise LayoutError("GPT 分区范围或索引冲突")
        names.add(name)
        indexes.add(index)
        first = end + 1
    return partitions, names


def reusable_spans(partitions, reclaim):
    spans = []
    for row in partitions:
        if row["name"] not in reclaim:
            continue
        start = row["first_sector"]
        end = row["last_sector"] + 1
        if spans and spans[-1][1] == start:
            spans[-1] = (spans[-1][0], end)
        else:
            spans.append((start, end))
    return [
        ((start + ALIGN - 1) // ALIGN * ALIGN, end // ALIGN * ALIGN)
        for start, end in spans
        if (start + ALIGN - 1) // ALIGN < end // ALIGN
    ]


def allocate(spans, size, name):
    best = min(
        ((end - start, start, i) for i, (start, end) in enumerate(spans) if end - start >= size),
        default=None,
    )
    if best is None:
        raise LayoutError(f"安全可重用区域放不下 {name}（{size // ALIGN} MiB）")
    _, start, pos = best
    _, end = spans.pop(pos)
    if start + size < end:
        spans.append((start + size, end))
    return start, size


def plan_layout(layout, board_family, image_bytes=None):
    """Plan extents only. Preserve reserved gaps, existing protected entries and table bounds."""
    if board_family != "UFI003":
        raise LayoutError("仅为已识别的 UFI003 规划")
    profile = capacity_profile(layout["disk_bytes"])
    slot, records, config, abuse, keys = PROFILES[profile]
    partitions, names = validate(layout)
    if not REQUIRED <= names or names - FIXED - NATIVE - STOCK - TARGET:
        raise LayoutError("包含未知分区或缺少必需引导分区")
    if names & TARGET:
        raise LayoutError("盘上已有 A/B 布局，应使用槽更新流程")
    if NATIVE <= names and not names & STOCK:
        reclaim = NATIVE
    elif STOCK <= names and not names & NATIVE:
        reclaim = STOCK
    else:
        raise LayoutError("未识别旧系统布局")
    if image_bytes is not None and (type(image_bytes) is not int or not 0 < image_bytes <= slot * MIB):
        raise LayoutError(f"固件镜像超过 {slot} MiB 槽容量或大小无效")
    preserved = [row for row in partitions if row["name"] not in reclaim]
    if len(preserved) + len(TARGET) > layout["entry_count"]:
        # The accepted GPT envelope already reserves 32 sectors on each side.
        # A future writer must expand the entry count while preserving all existing indexes.
        entry_count = 128
    else:
        entry_count = layout["entry_count"]
    occupied = {row["index"] for row in preserved}
    available = iter(i for i in range(1, entry_count + 1) if i not in occupied)
    spans = reusable_spans(partitions, reclaim)
    result = []

    def add(name, start, size):
        result.append({"name": name, "index": next(available), "first_sector": start,
                       "last_sector": start + size - 1, "bytes": size * SECTOR_BYTES})

    sizes = (slot, slot, records, config, abuse, keys, 8)
    for name, mib in sorted(zip(TARGET_TYPES, sizes), key=lambda item: -item[1]):
        add(name, *allocate(spans, mib * ALIGN, name))
    start, end = max(spans, key=lambda span: span[1] - span[0], default=(0, 0))
    if end - start < 64 * ALIGN:
        raise LayoutError("录音分区不足 64 MiB")
    add("RSH_AUDIO", start, end - start)

    return {
        "profile_gb": profile, "disk_bytes": layout["disk_bytes"], "slot_bytes": slot * MIB,
        "entry_count": entry_count, "new_partitions": sorted(result, key=lambda row: row["index"]),
        "preserved_partitions": preserved, "reclaimed_names": sorted(reclaim), "write_capable": False,
        "image_size_checked": image_bytes is not None,
    }


def format_plan(plan):
    lines = [f'A/B 分区预览（{plan["profile_gb"]} GB 档，只读）']
    for row in plan["new_partitions"]:
        lines.append(f'  {row["name"]:<12} {row["bytes"] // MIB:>5} MiB'
                     f'  LBA {row["first_sector"]}–{row["last_sector"]}')
    return lines
