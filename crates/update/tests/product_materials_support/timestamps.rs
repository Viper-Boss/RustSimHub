// SPDX-License-Identifier: AGPL-3.0-only
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};

/// Exercise a restored-data/metadata mutation until the filesystem records it.
/// A short sleep alone does not establish a different ctime on every Linux fs.
/// This is test setup, not a promise to detect writes within one ctime tick.
pub fn mutate_until_ctime_changes(path: &Path, mut mutate: impl FnMut()) {
    let initial = fs::metadata(path).unwrap();
    let initial = (initial.ctime(), initial.ctime_nsec());
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        mutate();
        let observed = fs::metadata(path).unwrap();
        if (observed.ctime(), observed.ctime_nsec()) != initial {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "filesystem did not record the test mutation within one second"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
