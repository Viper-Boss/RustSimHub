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
WORK = PROJECT / "work/ble-core-port-uids"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pull", action="store_true", help="explicitly pull the pinned public fixture image")
    args = parser.parse_args()
    target = "core_port_uid"
    features = "core-port"
    output_work = WORK
    output_work.mkdir(parents=True, exist_ok=True)
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
    with (output_work/"build.jsonl").open("w") as output, (output_work/"build.log").open("w") as errors:
        subprocess.run(["cargo", "test", "-p", "rustsimhub-ble", "--test", target,
                        "--features", features, "--no-run",
                        "--locked", "--offline", "--message-format=json"],
                       cwd=PROJECT, stdout=output, stderr=errors, check=True)
    artifacts = [json.loads(line) for line in (output_work/"build.jsonl").read_text().splitlines()]
    binary = next(row["executable"] for row in artifacts if row.get("reason")=="compiler-artifact"
                  and row.get("target", {}).get("name")==target)
    stage = output_work/"container-input"
    stage.mkdir(exist_ok=True)
    stage.chmod(0o755)  # public test code; UID1001 must traverse the read-only bind.
    shutil.copy2(binary, stage/"core_port_uid")
    shutil.copy2(PROJECT/"crates/ble/tests/fixtures/core_port_runner.py", stage/"runner.py")
    (stage/"core_port_uid").chmod(0o755)
    (stage/"runner.py").chmod(0o644)
    container = "rustsimhub-uid-fixture-"+uuid.uuid4().hex[:12]
    command = docker+["run", "--name", container, "--rm", "--pull=never", "--network=none",
                      "--read-only", "--tmpfs", "/tmp:rw,nosuid,nodev,size=128m",
                      "--cap-drop=ALL", "--cap-add=SETUID", "--cap-add=SETGID",
                      "--cap-add=CHOWN", "--cap-add=FOWNER", "--security-opt=no-new-privileges",
                      "--pids-limit=32", "--memory=256m", "--cpus=2",
                      "--mount", f"type=bind,src={stage},dst=/input,readonly",
                      IMAGE, "python3", "/input/runner.py", "/input/core_port_uid"]
    try:
        with (output_work/"container.log").open("w") as output:
            subprocess.run(command, env=docker_environment, stdout=output,
                           stderr=subprocess.STDOUT, check=True, timeout=120)
    finally:
        # Only this uniquely named fixture; no global container/daemon cleanup.
        subprocess.run(docker+["rm", "-f", container], env=docker_environment,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    output = (output_work/"container.log").read_text()
    cases = ("roundtrip", "cancel", "fence", "oversize", "wrong_tag", "wrong_version", "timeout",
             "wrong_kernel", "bad_mode", "symlink", "extra_group", "wrong_local")
    for case in cases:
        if "PORT_CASE "+case+" pass" not in output:
            raise RuntimeError("missing executed Core port case: "+case)
    if output.count("1 passed; 0 failed") != len(cases) or "CORE_PORT_COMPLETE 12" not in output:
        raise RuntimeError("missing actual libtest/server evidence")
    result = {"fixture": "ble-core-port-uid", "cases_passed": len(cases), "failed": 0}
    result["source_head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=PROJECT, text=True).strip()
    result["tracked_dirty"] = bool(subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=PROJECT, text=True).strip())
    result["image"] = IMAGE
    result["binary_sha256"] = hashlib.sha256((stage/"core_port_uid").read_bytes()).hexdigest()
    result["runner_sha256"] = hashlib.sha256((stage/"runner.py").read_bytes()).hexdigest()
    result["log_sha256"] = hashlib.sha256((output_work/"container.log").read_bytes()).hexdigest()
    (output_work/"result.json").write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
