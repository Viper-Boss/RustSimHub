# SPDX-License-Identifier: AGPL-3.0-only
"""Controller artifact selection tests; not substitutes for business recovery."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[3] / "tools/test-business-recovery-matrix.py"
SPEC = importlib.util.spec_from_file_location("business_matrix_runner", SOURCE)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class ArtifactSelection(unittest.TestCase):
    def fixture(self, root):
        target = root / "build"
        target.mkdir()
        rows = []
        for name in RUNNER.CRATES:
            path = target / ("lib" + name + ".rlib")
            path.write_bytes(b"synthetic metadata fixture; never executed")
            rows.append({"reason": "compiler-artifact", "target": {"name": name},
                         "package_id": "registry+https://example.invalid#" + name + "@" +
                         ("0.10.9" if name == "sha2" else "1.0.0"),
                         "filenames": [str(path)]})
        return target, rows

    def test_exact_artifacts_and_two_existing_sha_versions(self):
        with tempfile.TemporaryDirectory() as raw:
            target, rows = self.fixture(Path(raw))
            newer = target / "libsha2_newer.rlib"
            newer.write_bytes(b"synthetic second ABI")
            rows.append({"reason": "compiler-artifact", "target": {"name": "sha2"},
                         "package_id": "registry+https://example.invalid#sha2@0.11.1",
                         "filenames": [str(newer)]})
            result = RUNNER.artifacts("\n".join(map(json.dumps, rows)), target)
            self.assertEqual(set(result), set(RUNNER.CRATES))
            self.assertEqual(result["sha2"].name, "libsha2.rlib")

    def test_missing_library_is_not_silently_skipped(self):
        with tempfile.TemporaryDirectory() as raw:
            target, rows = self.fixture(Path(raw))
            with self.assertRaisesRegex(RUNNER.MatrixError, "missing"):
                RUNNER.artifacts("\n".join(map(json.dumps, rows[:-1])), target)

    def test_ambiguous_same_abi_is_refused(self):
        with tempfile.TemporaryDirectory() as raw:
            target, rows = self.fixture(Path(raw))
            extra = target / "libage_other.rlib"
            extra.write_bytes(b"synthetic ambiguous artifact")
            rows.append({**rows[-1], "filenames": [str(extra)]})
            with self.assertRaisesRegex(RUNNER.MatrixError, "ambiguous"):
                RUNNER.artifacts("\n".join(map(json.dumps, rows)), target)

    def test_artifact_outside_private_build_is_refused(self):
        with tempfile.TemporaryDirectory() as raw:
            target, rows = self.fixture(Path(raw))
            other = Path(raw) / "libage.rlib"
            other.write_bytes(b"synthetic outside artifact")
            rows[-1]["filenames"] = [str(other)]
            with self.assertRaisesRegex(RUNNER.MatrixError, "outside"):
                RUNNER.artifacts("\n".join(map(json.dumps, rows)), target)


if __name__ == "__main__":
    unittest.main()
