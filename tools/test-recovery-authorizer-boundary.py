#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""D-21: plan by default; --run tests current public Auth/Store/wire offline.

No proposed authorizer, new manifest, migration, device or transport is executed.
Uses the existing backup-recovery runner's locked-production-rlib approach.
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
FIXTURE = ROOT / "tools/fixtures/recovery-authorizer/boundary.rs"
LIBS = {"rustsimhub_auth", "rustsimhub_storage", "rustsimhub_policy",
        "rustsimhub_wire", "ed25519_dalek", "serde_json", "rusqlite"}
CASES = {
    "independent_recovery_key_has_no_login_owner_or_bootstrap_authority",
    "normal_network_client_and_nas_cannot_be_used_as_recent_owner",
    "revocation_restart_and_reapproval_do_not_resurrect_old_session_or_proof",
    "business_restore_keeps_device_pin_but_rotates_realm_and_deactivates_grants_source",
    "existing_scope_mask_and_schema_cannot_store_an_extra_recovery_bit",
    "payload_cannot_assert_ble_channel_peer_credentials_or_recovery_capability",
}


def command(args, env, timeout=180):
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        child = subprocess.Popen(
            [str(arg) for arg in args], cwd=ROOT, env=env,
            stdin=subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=True,
        )
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            raise RuntimeError("bounded_child_timeout") from None
        if out.tell() + err.tell() > 8 * 1024 * 1024:
            raise RuntimeError("bounded_output_exceeded")
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read().decode(), err.read().decode()
        if child.returncode:
            print((stderr + stdout)[-12000:], file=sys.stderr)
            raise RuntimeError("build_or_boundary_fixture_failed")
        return stdout


def run():
    if sys.platform != "linux":
        raise RuntimeError("fixture_requires_linux")
    env = os.environ.copy()
    command(["rustfmt", "--edition=2021", "--check", FIXTURE], env)
    (ROOT / "work").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="rsh-authorizer-", dir=ROOT / "work") as raw:
        work = Path(raw)
        target = work / "target"
        stream = command([
            "cargo", "build", "--lib", "--locked", "--offline",
            "-p", "rustsimhub-auth", "-p", "rustsimhub-wire",
            "--target-dir", target, "--message-format=json",
        ], env, timeout=600)
        artifacts = {}
        for line in stream.splitlines():
            item = json.loads(line)
            if item.get("reason") != "compiler-artifact":
                continue
            name = item["target"]["name"]
            if name not in LIBS:
                continue
            for filename in item["filenames"]:
                original = Path(filename)
                if original.suffix != ".rlib":
                    continue
                path = original.resolve(strict=True)
                if original.is_symlink() or not path.is_relative_to(target.resolve()):
                    raise RuntimeError("invalid_library_artifact")
                if name in artifacts and artifacts[name] != path:
                    raise RuntimeError("ambiguous_library_artifact")
                artifacts[name] = path
        if set(artifacts) != LIBS:
            raise RuntimeError("missing_production_library")
        driver = command(["rustup", "which", "clippy-driver"], env).strip()
        binary = work / "boundary"
        args = [driver, "--edition=2021", "--test", FIXTURE,
                "-D", "warnings", "-o", binary]
        for directory in sorted({path.parent for path in artifacts.values()}):
            args += ["-L", "dependency=" + str(directory)]
        for name, path in sorted(artifacts.items()):
            args += ["--extern", name + "=" + str(path)]
        command(args, env)
        listing = command([binary, "--list", "--format", "terse"], env)
        actual = {line.removesuffix(": test") for line in listing.splitlines()
                  if line.endswith(": test")}
        if actual != CASES:
            raise RuntimeError("case_inventory_mismatch")
        case_root = work / "cases"
        case_root.mkdir(mode=0o700)
        env["RSH_AUTHORIZER_CASE_ROOT"] = str(case_root)
        output = command([binary, "--test-threads=1"], env)
        passed = set(re.findall(r"^test (\w+) \.\.\. ok$", output, re.M))
        if passed != CASES or f"{len(CASES)} passed; 0 failed; 0 ignored" not in output:
            raise RuntimeError("incomplete_boundary_fixture")
        return {"passed": True, "case_count": len(passed), "cases": sorted(passed),
                "backend": "locked-production-rlibs", "authorizer_implemented": False,
                "migration_executed": False, "device_operations": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()
    try:
        if args.run:
            result = run()
        else:
            result = {"execute": False, "case_count": len(CASES),
                      "plan": "build --locked --offline; compile real API fixture; run six cases",
                      "authorizer_implemented": False, "device_operations": 0}
        print(json.dumps(result, sort_keys=True))
        return 0
    except (OSError, RuntimeError, ValueError, KeyError) as error:
        print(json.dumps({"passed": False, "reason": str(error)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
