# SPDX-License-Identifier: AGPL-3.0-only
import unittest

from boot_slot_policy import BootState
from storage_layout import ALIGN, MIB, LayoutError, format_plan, plan_layout


def layout_legacy():
    sectors = 7634944
    parts = [
        ("cdt", 131072, 4), ("sbl1", 262144, 1024), ("rpm", 263168, 1024),
        ("tz", 264192, 2048), ("hyp", 266240, 1024), ("sec", 267264, 32),
        ("modemst1", 267296, 4096), ("modemst2", 271392, 4096),
        ("fsc", 275488, 2), ("fsg", 393216, 4096), ("aboot", 524288, 2048),
        ("boot", 526336, 131072), ("devinfo", 657408, 2048),
        ("rootfs", 659456, 6711296), ("NSHSTATE", 7370752, 264159),
    ]
    return geometry(sectors, parts)


def geometry(sectors, parts):
    return {
        "disk_sectors": sectors, "disk_bytes": sectors * 512, "first_usable": 34,
        "last_usable": sectors - 34, "entry_count": 16,
        "partitions": [
            {"name": name, "index": i, "first_sector": start, "last_sector": start + size - 1}
            for i, (name, start, size) in enumerate(parts, 1)
        ],
    }


class LayoutTests(unittest.TestCase):
    def assert_valid_plan(self, layout, plan):
        original = {row["name"]: row for row in layout["partitions"]}
        result = sorted(plan["preserved_partitions"] + plan["new_partitions"], key=lambda row: row["first_sector"])
        self.assertEqual(len({row["index"] for row in result}), len(result))
        for left, right in zip(result, result[1:]):
            self.assertLess(left["last_sector"], right["first_sector"])
        self.assertLessEqual(result[-1]["last_sector"], layout["last_usable"])
        for row in plan["preserved_partitions"]:
            self.assertEqual(row, original[row["name"]])
        for row in plan["new_partitions"]:
            self.assertEqual(row["first_sector"] % ALIGN, 0)
            self.assertEqual((row["last_sector"] + 1) % ALIGN, 0)
        self.assertFalse(plan["write_capable"])

    def test_4gb_actual_legacy_geometry(self):
        layout = layout_legacy()
        plan = plan_layout(layout, "UFI003", image_bytes=608 * MIB)
        self.assert_valid_plan(layout, plan)
        self.assertEqual(plan["entry_count"], 128)
        self.assertEqual(plan["slot_bytes"], 608 * MIB)
        self.assertGreaterEqual(next(row["bytes"] for row in plan["new_partitions"] if row["name"] == "RSH_AUDIO"), 64 * MIB)
        self.assertEqual(plan["reclaimed_names"], ["NSHSTATE", "rootfs"])
        self.assertIn("RSH_AUDIO", "\n".join(format_plan(plan)))

    def test_8gb_stock_geometry_uses_only_reclaimable_ranges(self):
        original = layout_legacy()["partitions"][:-2]
        parts = [(p["name"], p["first_sector"], p["last_sector"] - p["first_sector"] + 1) for p in original]
        parts += [("system", 659456, 4194304), ("cache", 4853760, 131072),
                  ("recovery", 4984832, 65536), ("userdata", 5050368, 9980000)]
        layout = geometry(15269920, parts)
        layout["entry_count"] = 32
        plan = plan_layout(layout, "UFI003")
        self.assert_valid_plan(layout, plan)
        self.assertEqual(plan["profile_gb"], 8)
        self.assertEqual(plan["slot_bytes"], 1792 * MIB)
        self.assertFalse(plan["image_size_checked"])

    def test_reject_unknown_geometry_and_oversized_image(self):
        layout = layout_legacy()
        with self.assertRaises(LayoutError):
            plan_layout(layout, "UFI001C")
        with self.assertRaises(LayoutError):
            plan_layout(layout, "UFI003", 1456963584)
        layout["partitions"][-1]["name"] = "not-recognized"
        with self.assertRaises(LayoutError):
            plan_layout(layout, "UFI003")

    def test_disjoint_space_too_small_cannot_take_unallocated_areas(self):
        layout = layout_legacy()
        layout["partitions"][-2]["last_sector"] = 659456 + 1000 * ALIGN - 1
        with self.assertRaises(LayoutError):
            plan_layout(layout, "UFI003")


class BootPolicyTests(unittest.TestCase):
    def test_confirmed_slot_cannot_be_overwritten(self):
        state = BootState("a", frozenset(("a", "b")))
        with self.assertRaises(ValueError):
            state.begin_write("a")
        writing = state.begin_write("b")
        with self.assertRaises(ValueError):
            writing.schedule("b")
        self.assertEqual(writing.next_boot()[0], "a")

    def test_failed_trials_return_to_confirmed_slot_and_invalidate_candidate(self):
        state = BootState("a", frozenset(("a", "b"))).schedule("b")
        with self.assertRaises(ValueError):
            state.confirm("b")
        first, state = state.next_boot()
        second, state = state.next_boot()
        fallback, state = state.next_boot()
        self.assertEqual((first, second, fallback), ("b", "b", "a"))
        with self.assertRaises(ValueError):
            state.schedule("b")

    def test_health_check_switches_current_slot_and_allows_manual_rollback(self):
        original = BootState("a", frozenset(("a",)))
        candidate = original.verify_inactive("b").schedule("b")
        booted, candidate = candidate.next_boot()
        with self.assertRaises(ValueError):
            candidate.confirm("a")
        confirmed = candidate.confirm(booted)
        self.assertEqual(confirmed.confirmed, "b")
        self.assertEqual(confirmed.schedule("a").next_boot()[0], "a")


if __name__ == "__main__":
    unittest.main()
