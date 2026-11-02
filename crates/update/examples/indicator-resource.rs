// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic Linux-only regression helper; never an installed update payload.
use rustsimhub_update::indicator::Activity;
use std::{fs, os::unix::fs::MetadataExt, thread, time::Duration};

fn state(active: bool) -> serde_json::Value {
    let path = "/run/rustsimhub-led-update/state.json";
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["upgrading"], active);
    assert_eq!(value["available"], false);
    value
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!("{{\"development_fixtures\":true,\"role\":\"indicator-resource-fixture\"}}");
        return;
    }
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    if args == ["--check-normal"] {
        let activity = Activity::begin();
        let first = state(true)["expires_ms"].as_u64().unwrap();
        thread::sleep(Duration::from_millis(3300));
        assert!(state(true)["expires_ms"].as_u64().unwrap() > first);
        drop(activity);
        state(false);
        println!("{{\"heartbeat_refreshed\":true,\"upgrading\":false}}");
        return;
    }
    if args == ["--check-unsafe-directory"] {
        let directory = "/run/rustsimhub-led-update";
        assert_eq!(
            fs::symlink_metadata(directory).unwrap().mode() & 0o777,
            0o755
        );
        let activity = Activity::begin();
        drop(activity);
        assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
        println!("{{\"unsafe_directory_untouched\":true,\"operation_continued\":true}}");
        return;
    }
    assert_eq!(args, ["--check"]);
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
    state(false);
    println!("{{\"actual_thread_exhaustion\":true,\"operation_continued\":{continued},\"upgrading\":false}}");
}
