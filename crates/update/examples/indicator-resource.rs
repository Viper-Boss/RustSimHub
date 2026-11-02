// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic Linux-only regression helper; never an installed update payload.
use rustsimhub_update::indicator::Activity;
use std::{fs, os::unix::fs::MetadataExt, thread};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!("{{\"development_fixtures\":true,\"role\":\"indicator-resource-fixture\"}}");
        return;
    }
    assert_eq!(args, ["--check"]);
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    // Refuse to write any hint unless the actual OS first denies a thread.
    match thread::Builder::new().spawn(|| {}) {
        Ok(worker) => {
            worker.join().unwrap();
            panic!("fixture requires actual thread exhaustion");
        }
        Err(error) => assert_eq!(error.raw_os_error(), Some(11)), // EAGAIN
    }
    let activity = Activity::begin();
    // Work after begin must remain reachable even when the hint worker cannot run.
    let continued = true;
    drop(activity);
    let path = "/run/rustsimhub-led-update/state.json";
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    let state: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(state["upgrading"], false);
    assert_eq!(state["available"], false);
    println!("{{\"actual_thread_exhaustion\":true,\"operation_continued\":{continued},\"upgrading\":false}}");
}
