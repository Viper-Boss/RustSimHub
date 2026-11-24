# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory
import unittest

from build_ab_data_images import build
from test_dual_slot import geometry, layout_legacy


@unittest.skipUnless(all(shutil.which(p) for p in ("mke2fs", "debugfs", "e2fsck", "blkid", "ssh-keygen")),
                     "requires ext4 and OpenSSH tools")
class DataImagesTests(unittest.TestCase):
    def test_both_capacity_profiles_create_verified_data_images(self):
        four = layout_legacy()
        parts = [(p["name"], p["first_sector"], p["last_sector"] - p["first_sector"] + 1)
                 for p in four["partitions"][:-2]]
        parts += [("system", 659456, 4194304), ("cache", 4853760, 131072),
                  ("recovery", 4984832, 65536), ("userdata", 5050368, 9980000)]
        eight = geometry(15269920, parts)
        eight["entry_count"] = 32
        with TemporaryDirectory() as tmp:
            folder = Path(tmp)
            key = folder/"synthetic"
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
            pub = key.with_suffix(".pub")
            digest = hashlib.sha256(pub.read_bytes()).hexdigest()
            with self.assertRaises(ValueError):
                build(four, "UFI003", key, digest, folder/"rejected")
            self.assertFalse((folder/"rejected").exists())
            for capacity, layout in ((4, four), (8, eight)):
                output = folder/str(capacity)
                result = build(layout, "UFI003", pub, digest, output)
                self.assertEqual(result["profile_gb"], capacity)
                self.assertFalse(result["flash_allowed"])
                self.assertEqual(set(result["partitions"]),
                                 {"RSH_CONFIG", "RSH_AUDIO", "RSH_RECORDS", "RSH_KEYS", "RSH_ABUSE", "RSH_BOOTCTL"})
                self.assertEqual(result["partitions"]["RSH_AUDIO"]["bytes"], 64 * 1024**2)
                expected = result["partitions"]["RSH_BOOTCTL"]
                image = output/"RSH_BOOTCTL.img"
                self.assertEqual(image.stat().st_size, expected["bytes"])
                with image.open("rb") as handle:
                    record = handle.read(4096)
                    handle.seek(expected["bytes"] // 2)
                    self.assertEqual(handle.read(4096), record)
                self.assertEqual(record[16:20], bytes((0, 255, 1, 0)))


if __name__ == "__main__":
    unittest.main()
