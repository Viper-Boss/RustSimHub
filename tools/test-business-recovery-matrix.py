#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""C-09..12: build locked real libraries and test only isolated synthetic files.

The default prints a plan. --run is Linux-only. No device/service/network calls,
production restore scripts, new Cargo manifest or replacement restore logic.
"""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tools/fixtures/backup-recovery/matrix.rs"
CRATES = (
    "rustsimhub_auth", "rustsimhub_storage", "rustsimhub_recovery",
    "rustsimhub_forwarder", "rustsimhub_domain", "rustsimhub_policy",
    "rusqlite", "serde_json", "sha2", "age",
)
CASE_NAMES = {
    "wal_backup_captures_committed_data_and_omits_other_backup_domains",
    "matching_identity_and_business_restore_recover_only_the_original_owner",
    "sms_and_calls_restore_preserve_ids_unknowns_and_invalidate_old_cursors",
    "repeated_restore_rotates_realms_and_never_overwrites_existing_targets",
    "corrupt_missing_and_newer_database_backups_preserve_original_state",
    "identity_ciphertext_key_pin_and_inner_version_matrix_writes_no_plaintext",
    "identity_restore_repetition_links_and_private_modes_do_not_overwrite",
    "mixed_device_and_missing_identity_fail_at_the_actual_auth_boundary",
    "wrong_owner_proof_never_reactivates_another_principal",
    "closed_outbox_reopen_preserves_confirmed_unknown_and_attempt_ids",
    "restored_realm_rejects_both_matching_and_mixed_old_outbox_without_adoption",
    "same_device_mixed_batches_have_no_product_wide_consistency_proof",
    "missing_outbox_and_configuration_are_not_reconstructed_by_store_restore",
    "database_backup_and_restore_path_failures_keep_existing_data",
    "restored_unknown_operations_cannot_become_new_current_line_actions",
}
IDENTITY_CASES = {
    "identity_ciphertext_key_pin_and_inner_version_matrix_writes_no_plaintext",
    "identity_restore_repetition_links_and_private_modes_do_not_overwrite",
    "mixed_device_and_missing_identity_fail_at_the_actual_auth_boundary",
}
OUTBOX_CASES = {
    "closed_outbox_reopen_preserves_confirmed_unknown_and_attempt_ids",
    "restored_realm_rejects_both_matching_and_mixed_old_outbox_without_adoption",
    "same_device_mixed_batches_have_no_product_wide_consistency_proof",
}
CASES = {("identity::" if name in IDENTITY_CASES else
          "outbox::" if name in OUTBOX_CASES else "business::") + name
         for name in CASE_NAMES}


class MatrixError(RuntimeError):
    pass


def command(args, *, env=None, timeout=120):
    child = subprocess.Popen(
        [str(a) for a in args], cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
    )
    try:
        out, err = child.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.communicate()
        raise MatrixError("bounded_child_timeout") from None
    if len(out) + len(err) > 8 * 1024 * 1024:
        raise MatrixError("bounded_child_output_exceeded")
    if child.returncode:
        # Only compiler/test diagnostics from the public source and synthetic
        # fixtures; never dump Cargo's full artifact stream or generated keys.
        diagnostic = err.decode("utf-8", "replace")[-12000:]
        if args[0] != "cargo":
            diagnostic += out.decode("utf-8", "replace")[-12000:]
        if diagnostic:
            print(diagnostic, file=sys.stderr)
        raise MatrixError("build_or_real_matrix_failed")
    return out.decode("utf-8")


def artifacts(stream, target):
    found = {}
    for line in stream.splitlines():
        item = json.loads(line)
        if item.get("reason") != "compiler-artifact":
            continue
        name = item["target"]["name"]
        if name not in CRATES:
            continue
        # Existing workspace libraries use sha2 0.10; age also brings 0.11.
        # Keep both transitive ABI artifacts in -L, but bind this fixture's
        # explicit sha2 import to the existing workspace's 0.10 dependency.
        if name == "sha2" and not re.search(r"(?:#| )sha2(?:@| )0\.10\.", item["package_id"]):
            continue
        for filename in item["filenames"]:
            original = Path(filename)
            if original.is_symlink():
                raise MatrixError("linked_library_artifact")
            path = original.resolve(strict=True)
            if path.suffix != ".rlib":
                continue
            if not path.is_relative_to(target.resolve()) or path.is_symlink():
                raise MatrixError("artifact_outside_fresh_build")
            if name in found and found[name] != path:
                raise MatrixError("ambiguous_library_artifact")
            found[name] = path
    if set(found) != set(CRATES):
        raise MatrixError("required_production_library_missing")
    return found


def run():
    if sys.platform != "linux":
        raise MatrixError("real_matrix_requires_linux")
    command(["rustfmt", "--edition", "2024", "--check", FIXTURE])
    with tempfile.TemporaryDirectory(prefix="rsh-synthetic-recovery-") as raw:
        work = Path(raw)
        work.chmod(0o700)
        target = work / "build"
        env = os.environ.copy()
        for name in ("CARGO_MAKEFLAGS", "MAKEFLAGS"):
            env.pop(name, None)
        env["CARGO_BUILD_JOBS"] = "2"
        args = ["cargo", "build", "--lib", "--locked", "--offline",
                "--message-format=json", "--target-dir", target]
        for package in ("storage", "auth", "recovery", "forwarder"):
            args += ["-p", "rustsimhub-" + package]
        libs = artifacts(command(args, env=env, timeout=600), target)
        driver = command(["rustup", "which", "clippy-driver"]).strip()
        binary = work / "business-recovery-matrix"
        compile_args = [driver, "--edition=2024", "--test", FIXTURE,
                        "-D", "warnings", "-o", binary]
        dependency_dirs = {p.parent for p in libs.values()}
        for directory in sorted(dependency_dirs):
            compile_args += ["-L", "dependency=" + str(directory)]
        for name, path in sorted(libs.items()):
            compile_args += ["--extern", name + "=" + str(path)]
        command(compile_args, env=env)
        listing = command([binary, "--list", "--format", "terse"])
        actual = {line.removesuffix(": test") for line in listing.splitlines()
                  if line.endswith(": test")}
        if actual != CASES:
            raise MatrixError("matrix_case_inventory_mismatch")
        case_root = work / "cases"
        case_root.mkdir(mode=0o700)
        env["RSH_RECOVERY_SYNTHETIC_ROOT"] = str(case_root)
        output = command([binary, "--test-threads=1"], env=env)
        passed = set(re.findall(r"^test ([\w:]+) \.\.\. ok$", output, re.M))
        if passed != CASES or f"{len(CASES)} passed; 0 failed; 0 ignored" not in output:
            raise MatrixError("real_matrix_incomplete_or_skipped")
        return {"passed": True, "backend": "locked-production-rust-rlibs",
                "case_count": len(CASES), "cases": sorted(passed),
                "production_changes": False, "device_operations": 0,
                "transport_sends": 0, "new_cargo_dependencies": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()
    try:
        result = run() if args.run else {
            "execute": False, "case_count": len(CASES),
            "plan": ["build existing workspace libraries --locked --offline",
                     "compile public fixture against exact Cargo .rlib artifacts",
                     "run all cases in a fresh private temporary directory"],
            "requires": ["Linux", "pinned Rust and Clippy", "cached locked dependencies"],
            "device_operations": 0,
        }
        print(json.dumps(result, sort_keys=True))
    except (MatrixError, OSError, ValueError, KeyError) as error:
        print(json.dumps({"passed": False, "reason": str(error),
                          "device_operations": 0}), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
