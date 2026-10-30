#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Explicit manual fixture: no host account changes, no production deployment."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import uuid

IMAGE = "python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb"
PROJECT = Path(__file__).resolve().parents[3]
WORK = PROJECT / "work/recovery-kernel-uids"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull", action="store_true", help="explicitly pull the pinned public fixture image")
    args = parser.parse_args()
    WORK.mkdir(parents=True, exist_ok=True)
    docker_environment = dict(os.environ)
    for name in ("DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_TLS", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"):
        docker_environment.pop(name, None)
    # Keep DOCKER_CONFIG/registry auth/proxy configuration intact; choose only
    # this managed local daemon, never an inherited remote currentContext.
    docker = ["docker", "--host=unix:///var/run/docker.sock"]
    subprocess.run(docker+["info", "--format", "{{.ServerVersion}}"], env=docker_environment, check=True)
    if args.pull:
        subprocess.run(docker+["pull", IMAGE], env=docker_environment, check=True)
    subprocess.run(docker+["image", "inspect", IMAGE, "--format", "{{.Id}}"], env=docker_environment, check=True)
    with (WORK/"build.jsonl").open("w") as output, (WORK/"build.log").open("w") as errors:
        subprocess.run(["cargo", "test", "-p", "rustsimhub-core", "--test", "recovery_kernel_uid",
                        "--features", "recovery-transport,recovery-grants", "--no-run",
                        "--locked", "--offline", "--message-format=json"],
                       cwd=PROJECT, stdout=output, stderr=errors, check=True)
    artifacts = [json.loads(line) for line in (WORK/"build.jsonl").read_text().splitlines()]
    binary = next(row["executable"] for row in artifacts if row.get("reason")=="compiler-artifact"
                  and row.get("target", {}).get("name")=="recovery_kernel_uid")
    stage = WORK/"container-input"
    stage.mkdir(exist_ok=True)
    stage.chmod(0o755)  # public test code; UID1001 must traverse the read-only bind.
    shutil.copy2(binary, stage/"recovery_kernel_uid")
    shutil.copy2(PROJECT/"crates/core/tests/fixtures/recovery_uid_runner.py", stage/"runner.py")
    (stage/"recovery_kernel_uid").chmod(0o755)
    (stage/"runner.py").chmod(0o644)
    container = "rustsimhub-uid-fixture-"+uuid.uuid4().hex[:12]
    command = docker+["run", "--name", container, "--rm", "--pull=never", "--network=none",
                      "--read-only", "--tmpfs", "/tmp:rw,nosuid,nodev,size=128m",
                      "--cap-drop=ALL", "--cap-add=SETUID", "--cap-add=SETGID",
                      "--cap-add=CHOWN", "--cap-add=FOWNER", "--security-opt=no-new-privileges",
                      "--pids-limit=32", "--memory=256m", "--cpus=2",
                      "--mount", f"type=bind,src={stage},dst=/input,readonly",
                      IMAGE, "python3", "/input/runner.py", "/input/recovery_kernel_uid"]
    try:
        with (WORK/"container.log").open("w") as output:
            subprocess.run(command, env=docker_environment, stdout=output,
                           stderr=subprocess.STDOUT, check=True, timeout=45)
    finally:
        # Only this uniquely named fixture; no global container/daemon cleanup.
        subprocess.run(docker+["rm", "-f", container], env=docker_environment,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    output = (WORK/"container.log").read_text()
    for case in [f"denied_{uid}" for uid in (0,1001,1003,1004,1005,1006)]+[
            "no_provider", "write_roundtrip", "primary_authorized", "occupied_slot_denied", "drop_fenced", "close_fenced", "reconnect_fresh_nonce"]:
        if "UID_CASE "+case not in output:
            raise RuntimeError("missing executed Core case: "+case)
    if "1 passed; 0 failed" not in output:
        raise RuntimeError("missing actual libtest success")
    result = json.loads(next(line for line in reversed(output.splitlines()) if line.startswith('{"fixture":')))
    result["source_head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=PROJECT, text=True).strip()
    result["tracked_dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=PROJECT, text=True).strip())
    result["image"] = IMAGE
    result["binary_sha256"] = hashlib.sha256((stage/"recovery_kernel_uid").read_bytes()).hexdigest()
    result["runner_sha256"] = hashlib.sha256((stage/"runner.py").read_bytes()).hexdigest()
    result["log_sha256"] = hashlib.sha256((WORK/"container.log").read_bytes()).hexdigest()
    (WORK/"result.json").write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
