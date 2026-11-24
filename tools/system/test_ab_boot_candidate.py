# SPDX-License-Identifier: AGPL-3.0-only
import os
from pathlib import Path
import shlex
import struct
import subprocess
import tempfile
import unittest
import zlib

from build_ab_boot_candidate import ANCHOR, HOOK, add_entry, cpio_entry, patch_initramfs

SELECTOR = shlex.split(os.environ.get("RSH_AB_SELECTOR_COMMAND", ""))
MIB = 1024 * 1024
BLOCK = 4096


def boot_record(generation, confirmed=0, pending=255, verified=3, tries=0):
    content = struct.pack("<8sQBBBB8x", b"RSHABCT1", generation, confirmed, pending, verified, tries)
    return (content + struct.pack("<I", zlib.crc32(content))).ljust(BLOCK, bytes(1))


class ArchiveTests(unittest.TestCase):
    def test_rewrites_archive_with_boot_hook(self):
        tail = cpio_entry([0] * 13, b"TRAILER!!!", b"")
        raw = add_entry(b"init", b"#!/bin/sh\n" + ANCHOR + b"\n", 5) + tail
        patched = patch_initramfs(raw, b"placeholder bytes", b"#!/bin/sh\n")
        self.assertEqual(patched.count(HOOK), 1)
        self.assertIn(b"usr/bin/rustsimhub-ab-select\0", patched)
        self.assertIn(b"usr/bin/rustsimhub-ab-root\0", patched)
        with self.assertRaises(ValueError):
            patch_initramfs(patched, b"same bytes", b"#!/bin/sh\n")
        with self.assertRaises(ValueError):
            patch_initramfs(raw[:-8], b"same bytes", b"#!/bin/sh\n")


@unittest.skipUnless(SELECTOR, "requires RSH_AB_SELECTOR_COMMAND with a built selector")
class SelectorTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.image = Path(temp.name) / "ledger.img"
        with self.image.open("xb") as handle:
            handle.truncate(8 * MIB)

    def seed(self, index, data):
        with self.image.open("r+b") as handle:
            handle.seek(index * 4 * MIB)
            handle.write(data)

    def read_head(self, index):
        with self.image.open("rb") as handle:
            handle.seek(index * 4 * MIB)
            return handle.read(BLOCK)

    def select(self, mode="select-image", *session):
        return subprocess.run(SELECTOR + [mode, str(self.image), *session], capture_output=True, text=True, timeout=10)

    def boot(self):
        result = self.select()
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def test_decrements_retries_before_returning_candidate(self):
        self.seed(0, boot_record(5, pending=1, tries=2))
        self.assertEqual(self.boot(), "B")
        self.assertEqual(self.read_head(0), boot_record(5, pending=1, tries=2))
        self.assertEqual(self.read_head(1), boot_record(6, pending=1, tries=1))
        self.assertEqual(self.boot(), "B")
        self.assertEqual(self.read_head(0), boot_record(7, pending=1, tries=0))
        self.assertEqual(self.boot(), "A")
        self.assertEqual(self.read_head(1), boot_record(8, verified=1))
        self.assertEqual(self.boot(), "A")
        self.assertEqual(self.read_head(1), boot_record(8, verified=1))

    def test_confirmation_and_rollback_are_bound_to_the_running_generation(self):
        self.seed(0, boot_record(10, pending=1, tries=2))
        self.assertNotEqual(self.select("confirm-image", "B", "10").returncode, 0)
        self.assertEqual(self.select("select-session-image").stdout.strip(), "B 11 trial")
        self.assertEqual(self.select("check-trial-image", "B", "11").returncode, 0)
        self.assertNotEqual(self.select("confirm-image", "A", "11").returncode, 0)
        self.assertNotEqual(self.select("confirm-image", "B", "10").returncode, 0)
        self.assertNotEqual(self.select("rollback-image", "B", "11").returncode, 0)
        self.assertEqual(self.select("confirm-image", "B", "11").stdout.strip(), "B 12")
        self.assertEqual(self.select("select-session-image").stdout.strip(), "B 12 stable")
        self.assertNotEqual(self.select("check-trial-image", "B", "11").returncode, 0)
        self.assertNotEqual(self.select("rollback-image", "B", "11").returncode, 0)
        self.assertEqual(self.select("rollback-image", "B", "12").stdout.strip(), "B 13")
        self.assertEqual(self.select("select-session-image").stdout.strip(), "A 14 trial")
        self.assertEqual(self.select("select-session-image").stdout.strip(), "A 15 trial")
        self.assertEqual(self.select("select-session-image").stdout.strip(), "B 16 stable")
        self.assertNotEqual(self.select("rollback-image", "B", "16").returncode, 0)

    def test_ignores_corrupt_newer_record(self):
        self.seed(0, boot_record(1))
        record = bytearray(boot_record(2, confirmed=1))
        record[28] ^= 1
        self.seed(1, record)
        self.assertEqual(self.boot(), "A")

    def test_rejects_empty_or_conflicting_records_and_non_device(self):
        self.assertNotEqual(self.select().returncode, 0)
        self.seed(0, boot_record(3))
        self.seed(1, boot_record(3, confirmed=1))
        self.assertNotEqual(self.select().returncode, 0)
        self.assertNotEqual(self.select("select").returncode, 0)


if __name__ == "__main__":
    unittest.main()
