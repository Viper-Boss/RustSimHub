// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

fn program(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metadata");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    (dir, path)
}
fn script_metadata(path: &Path, budget: Duration) -> Result<Vec<u8>, Error> {
    // Execute the existing interpreter, read the just-written fixture as data.
    // This avoids transient ETXTBSY before the pipe/limit test even starts.
    let mut command = Command::new("/bin/sh");
    command.arg(path).arg("--build-info");
    metadata_command(command, budget)
}
#[test]
fn production_wrapper_passes_only_the_fixed_build_info_argument() {
    assert_eq!(
        metadata(Path::new("/bin/echo"), Duration::from_secs(1)),
        Ok(b"--build-info\n".to_vec())
    );
}
#[test]
fn metadata_queries_only_build_info_and_clears_environment() {
    let (_dir, path) = program("test \"$#\" = 1 && test \"$1\" = --build-info && test -z \"$HOME\" && printf '{\"ok\":true}'");
    assert_eq!(
        script_metadata(&path, Duration::from_secs(1)),
        Ok(b"{\"ok\":true}".to_vec())
    );
}
#[test]
fn nonzero_and_invalid_program_are_rejected() {
    let (_dir, path) = program("printf 'partial'; exit 1");
    assert_eq!(script_metadata(&path, Duration::from_secs(1)), Err(Error));
    assert_eq!(
        metadata(&path.with_extension("absent"), Duration::from_secs(1)),
        Err(Error)
    );
}
#[test]
fn output_at_limit_is_accepted_and_over_limit_is_refused() {
    for (size, accepted) in [(16384, true), (16385, false)] {
        // Shell builtin only: this boundary test must not depend on the host's
        // head implementation, its runtime libraries or /dev/zero policy.
        let (_dir, path) = program(&format!("printf '%s' '{}'", "a".repeat(size)));
        let output = script_metadata(&path, Duration::from_secs(1));
        assert_eq!(output.is_ok(), accepted, "size={size}, output={output:?}");
        if accepted {
            assert_eq!(output.unwrap().len(), size);
        }
    }
}
#[test]
fn hung_direct_child_is_killed_and_reaped() {
    let (dir, path) = program("printf '%s' \"$$\" > \"$0.pid\"; exec /bin/sleep 20");
    let before = Instant::now();
    assert_eq!(
        script_metadata(&path, Duration::from_millis(100)),
        Err(Error)
    );
    assert!(before.elapsed() < Duration::from_secs(2));
    let pid = fs::read_to_string(dir.path().join("metadata.pid")).unwrap();
    assert!(!Path::new("/proc").join(pid).exists());
}
#[test]
fn root_owned_metadata_admission_checks_regular_file_mode_size_and_link_count() {
    // UID-specific admission is also covered in the real-UID chroot harness.
    if !rustix::process::geteuid().is_root() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contract");
    fs::write(&path, b"1234").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(owned_file(&path, 4).is_ok());
    assert!(owned_file(&path, 3).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();
    assert!(owned_file(&path, 4).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o4644)).unwrap();
    assert!(owned_file(&path, 4).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let link = dir.path().join("hardlink");
    fs::hard_link(&path, &link).unwrap();
    assert!(owned_file(&path, 4).is_err());
    fs::remove_file(link).unwrap();
    let link = dir.path().join("symlink");
    symlink(&path, &link).unwrap();
    assert!(owned_file(&link, 4).is_err());
    assert!(owned_file(dir.path(), 4).is_err());
}
