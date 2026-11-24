# SPDX-License-Identifier: AGPL-3.0-only
import hashlib
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import verify_ab_flash_inputs as assembly


class FlashInputsTests(unittest.TestCase):
    def test_builds_one_receipt_and_rejects_mixed_slot_provenance(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in ("boot", "A", "B", "shared"):
                (root/name).mkdir()
            gate = {"flash_allowed": False, "release_ready": False}

            def binary(path, data):
                path.write_bytes(data)
                return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}

            def put(path, data):
                path.write_text(json.dumps(data))

            boot = binary(root/"boot"/"boot.ab-candidate.img", b"boot")
            selector = "f" * 64
            put(root/"boot"/"AB-BOOT-CANDIDATE.json", {
                **gate, "kind": "internal-ab-boot-candidate", "initramfs_readback": True,
                "arm64_static_selector": True, "boot_sha256": boot["sha256"], "selector_sha256": selector,
            })
            for slot in ("A", "B"):
                image = binary(root/slot/"rootfs.img", slot.encode()*10)
                put(root/slot/"SYSTEM-IMAGE.json", {
                    **gate, "kind": "internal-unflashable-system-image", "ab_slot": slot,
                    "ab_capacity_gb": 4, "filesystem_label": "RSH_"+slot, "ab_boot_sha256": boot["sha256"],
                    "ab_selector_sha256": selector, "image_sha256": image["sha256"], "passed": True,
                    "source_tree_unchanged": True, "filesystem_clean_readonly_check": True,
                    "candidate_manifest_sha256": "1"*64, "tree_sha256": "2"*64,
                    "application_revision": "3"*40, "composition_revision": "4"*40,
                })
            shared = {}
            for label in assembly.DATA:
                length = 64 if label == "RSH_AUDIO" else 2
                shared[label] = binary(root/"shared"/f"{label}.img", bytes(length))
            shared["RSH_BOOTCTL"] = {"bytes": 8, "sha256": "0"*64}
            put(root/"shared"/"AB-DATA-IMAGES.json",
                {**gate, "kind": "internal-ab-data-images", "partitions": shared, "profile_gb": 4})

            def request():
                return assembly.verify(root/"boot", root/"A", root/"B", root/"shared", 4)

            with patch.object(assembly, "MIB", 1), patch.object(assembly, "BOOT_BYTES", 4), patch.object(
                assembly, "PROFILES", {4: (10, 2, 2, 2, 2)}
            ):
                result = request()
                self.assertEqual(set(result["images"]), assembly.DATA | {"RSH_A", "RSH_B", "boot"})
                self.assertFalse(result["flash_allowed"])
                report = root/"B"/"SYSTEM-IMAGE.json"
                saved = report.read_text()
                put(report, {**json.loads(saved), "ab_selector_sha256": "e"*64})
                with self.assertRaisesRegex(ValueError, "different boot candidate"):
                    request()
                report.write_text(saved)
                (root/"B"/"rootfs.img").write_bytes(b"wrong bytes"[:10])
                with self.assertRaisesRegex(ValueError, "Artifact hash differs"):
                    request()


if __name__ == "__main__":
    unittest.main()
