#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Replay a post-encode stall against the real RTC, then omit only its age gate.

Both variants are private git-archive exports. The original loopback test and
product deadlines remain unchanged. Requires the existing local TLS image.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import uuid

PROJECT = Path(__file__).resolve().parents[3]
CASE = "rtc::tests::encrypted_loopback_roundtrip_and_ticket_identity_revocation"
AGE_GATE = """        if frame.at_ms > actual_now || actual_now - frame.at_ms >= MAX_AGE_MS {
            return Err(Error::Bounds);
        }
"""
DELAY = """        // Private replay only: stall once after real Opus encode.
        static STALLED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if !STALLED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(75));
            eprintln!("private_post_encode_stall age_ms={} sent_frames={}",
                rustsimhub_media::local::now_ms().saturating_sub(frame.at_ms),
                self.sent_frames);
        }
"""


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="rustsimhub-private-turn-pinned:20261101")
    args = parser.parse_args()
    env = dict(os.environ)
    for key in ("DOCKER_HOST", "DOCKER_CONTEXT", "DOCKER_TLS", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"):
        env.pop(key, None)
    docker = ["docker", "--host=unix:///var/run/docker.sock"]
    image = subprocess.check_output(docker + ["image", "inspect", args.image, "--format", "{{.Id}}"], env=env, text=True).strip()
    assert image.startswith("sha256:")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=PROJECT, text=True).strip()
    archive = subprocess.check_output(["git", "archive", "--format=tar", head], cwd=PROJECT)
    (PROJECT / "work").mkdir(exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="rtc-frame-deadline-", dir=PROJECT / "work"))
    source = output / "source"
    source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(archive)) as bundle:
        bundle.extractall(source, filter="data")
    path = source / "native/audio-alsa/src/rtc.rs"
    original = path.read_text()
    start = original.index("    pub(crate) fn enqueue_frame(")
    end = original.index("    pub fn receive(", start)
    method = original[start:end]
    clock = "        let actual_now = rustsimhub_media::local::now_ms();\n"
    assert method.count(AGE_GATE) == 1 and method.count(clock) == 1
    rows = []
    stage = output / "input"
    stage.mkdir()
    stage.chmod(0o755)
    build_env = dict(env, CARGO_INCREMENTAL="0", CARGO_TARGET_DIR=str(PROJECT / "native/audio-alsa/target"))
    for name, keep_gate in (("original-gate-stalled", True), ("omitted-gate-stalled", False)):
        variant = method if keep_gate else method.replace(AGE_GATE, "")
        variant = variant.replace(clock, DELAY + clock)
        path.write_text(original[:start] + variant + original[end:])
        build = ["cargo", "test", "--manifest-path", str(source / "native/audio-alsa/Cargo.toml"),
                 "--test", "turn_private_tls", "--features", "turn-relay", "--no-run", "--locked", "--offline", "--message-format=json"]
        with (output / (name + "-build.jsonl")).open("w") as out, (output / (name + "-build.log")).open("w") as err:
            subprocess.run(build, cwd=source, env=build_env, stdout=out, stderr=err, check=True)
        artifacts = [json.loads(line) for line in (output / (name + "-build.jsonl")).read_text().splitlines()]
        binary = next(row["executable"] for row in artifacts if row.get("executable") and row["target"]["name"] == "turn_private_tls")
        shutil.copyfile(binary, stage / "rtc-test")
        (stage / "rtc-test").chmod(0o755)
        container = "rustsimhub-rtc-frame-deadline-" + uuid.uuid4().hex[:12]
        command = docker + ["run", "--name", container, "--rm", "--pull=never", "--network=none", "--read-only",
                           "--user=1000:1000", "--cap-drop=ALL", "--security-opt=no-new-privileges",
                           "--tmpfs=/tmp:rw,nosuid,nodev,size=32m,uid=1000,gid=1000", "--memory=128m", "--pids-limit=12", "--cpus=2",
                           "--mount", f"type=bind,src={stage},dst=/input,readonly", image,
                           "/input/rtc-test", "--exact", CASE, "--nocapture", "--test-threads=1"]
        log = output / (name + ".log")
        try:
            with log.open("w") as out:
                result = subprocess.run(command, env=env, stdout=out, stderr=subprocess.STDOUT, timeout=25)
        finally:
            subprocess.run(docker + ["rm", "-f", container], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        text = log.read_text()
        match = re.search(r"private_post_encode_stall age_ms=(\d+) sent_frames=0", text)
        assert match and int(match[1]) >= 75, "real post-encode clock must show the bounded stall before any RTP write"
        if keep_gate:
            assert result.returncode == 101 and "1 failed;" in text and "Bounds" in text
        else:
            assert result.returncode == 0 and "1 passed; 0 failed;" in text
        rows.append({"variant": name, "exit_code": result.returncode, "stall_age_ms": int(match[1]),
                     "sent_frames_before_gate": 0, "source_sha256": digest(path), "binary_sha256": digest(stage / "rtc-test"),
                     "build_log_sha256": digest(output / (name + "-build.log")), "log_sha256": digest(log)})
        print(json.dumps(rows[-1]), flush=True)
    path.write_text(original)
    report = {"source_head": head, "source_archive_sha256": hashlib.sha256(archive).hexdigest(),
              "original_rtc_sha256": digest(path), "image": image, "original_test_unchanged": True,
              "production_tree_modified": False, "product_deadline_changed": False, "injected_stall_ms": 75,
              "network": "none", "read_only": True, "uid": 1000, "capabilities": [],
              "memory_limit_bytes": 134217728, "pids_limit": 12, "cpus": 2, "variants": rows,
              "scope": "deliberate post-encode scheduling stall; not proof of the exact earlier ARM failure site"}
    (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"result_path": str(output / "result.json"), **report}), flush=True)


if __name__ == "__main__":
    main()
