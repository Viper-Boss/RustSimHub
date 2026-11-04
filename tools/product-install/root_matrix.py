#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run same-source test binaries in disposable, networkless private Root fixtures.

Requires an existing local Python 3 image. Never pulls an image, modifies host
accounts/services, mounts host devices, or writes through a host bind mount.
The extra SYS_ADMIN capability is confined to the read-only tmpfs scenario.
"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, help="existing local immutable image ID")
    parser.add_argument("--executor", required=True, type=Path)
    parser.add_argument("--rootio", required=True, type=Path)
    args = parser.parse_args()
    if not args.image.startswith("sha256:") or len(args.image) != 71:
        parser.error("an immutable local sha256 image ID is required")
    repo = Path(__file__).resolve().parents[2]
    private = repo / ".local" / "root-matrix"
    private.mkdir(parents=True, exist_ok=True, mode=0o700)
    env = dict(os.environ)
    for name in ("DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_TLS", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"):
        env.pop(name, None)
    docker = ["docker", "--host=unix:///var/run/docker.sock"]

    def run(command):
        subprocess.run(command, check=True, env=env, timeout=180)

    run(docker + ["image", "inspect", "--format={{.Id}}", args.image])
    with tempfile.TemporaryDirectory(prefix="inputs-", dir=private) as directory:
        stage = Path(directory)
        stage.chmod(0o755)
        for name, source in (("executor", args.executor), ("rootio", args.rootio)):
            shutil.copyfile(source.resolve(strict=True), stage / name)
            (stage / name).chmod(0o555)
        materials = stage / "materials"
        shutil.copytree(repo / "firmware/system/product-profiles", materials)
        for entry in [materials, *materials.rglob("*")]:
            entry.chmod(0o755 if entry.is_dir() else 0o444)
        common = docker + [
            "run", "--rm", "--pull=never", "--network=none", "--read-only",
            "--user=0:0", "--cap-drop=ALL", "--security-opt=no-new-privileges",
            "--pids-limit=48", "--memory=256m", "--cpus=1",
            "--tmpfs=/tmp:rw,nosuid,nodev,size=64m,mode=1777",
            "--mount", f"type=bind,src={stage},dst=/tests,readonly",
            "--mount", f"type=bind,src={stage},dst={repo / 'crates/update'},readonly",
            # Test fixtures use their compiled CARGO_MANIFEST_DIR, so expose
            # only the public fixed templates at that exact path, read-only.
            "--mount", f"type=bind,src={materials},dst={repo / 'firmware/system/product-profiles'},readonly",
        ]
        def container(arguments):
            name = "web4-install-test-" + uuid.uuid4().hex
            try:
                run(common + ["--name", name] + arguments)
            finally:
                subprocess.run(docker + ["rm", "--force", name], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               timeout=15, check=False)

        container(["--cap-add=CHOWN", "--cap-add=FOWNER", "--cap-add=FSETID",
                      args.image, "/tests/executor", "--test-threads=1", "--nocapture"])
        container([args.image, "/tests/rootio", "--test-threads=1", "--nocapture"])
        container(["--tmpfs=/small:rw,nosuid,nodev,size=1m,mode=700",
                      "--env=PRODUCT_INSTALL_ENOSPC_ROOT=/small", args.image,
                      "/tests/rootio", "--ignored", "--exact",
                      "dedicated_private_tmpfs_enospc_cleans_own_stage_without_target_write", "--nocapture"])
        # A new anonymous tmpfs in this container's private mount namespace.
        # Populate first, then remount read-only; no host path is remounted.
        readonly = """
import ctypes, os, subprocess
with open('/ro/target', 'wb') as file:
    file.write(b'original private readonly fixture')
os.chmod('/ro/target', 0o600)
libc = ctypes.CDLL(None, use_errno=True)
if libc.mount(None, b'/ro', b'tmpfs', 33, None):
    raise OSError(ctypes.get_errno(), 'private tmpfs remount')
subprocess.run(['/tests/rootio', '--ignored', '--exact',
    'dedicated_readonly_tmpfs_rejects_write_and_retains_baseline', '--nocapture'], check=True)
"""
        container(["--cap-add=SYS_ADMIN", "--security-opt=seccomp=unconfined",
                      "--tmpfs=/ro:rw,nosuid,nodev,size=1m,mode=700",
                      "--env=PRODUCT_INSTALL_READONLY_ROOT=/ro", args.image,
                      "python3", "-c", readonly])


if __name__ == "__main__":
    main()
