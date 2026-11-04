// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic disposable-container fixture, never a production writer entry.
#[cfg(feature = "dev-fixtures")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustsimhub_storage::{RecoveryChange, Store};
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !Path::new("/.dockerenv").exists() || rustix::process::geteuid().as_raw() != 986 {
        return Err("disposable container UID986 required".into());
    }
    let path = Path::new("/var/lib/rustsimhub/rustsimhub-v1/core/state");
    if args == ["--refuse-new-schema"] {
        if Store::open(path).is_ok() {
            return Err("old Store unexpectedly accepted new schema".into());
        }
        return Ok(());
    }
    if args != ["--make-schema6"] && args != ["--make-revoked-schema7"] {
        return Err("finite fixture option required".into());
    }
    let s = Store::open(path)?;
    s.handle().bind_identity([9; 32])?;
    let owner = s.handle().bootstrap_owner([1; 32])?;
    drop(s);
    let c = rusqlite::Connection::open(path.join("store.sqlite3"))?;
    c.execute("INSERT INTO sms_operations VALUES('00000000-0000-4000-8000-000000000001','00000000-0000-4000-8000-000000000002','synthetic-idempotency',1,?,'+10000000000','SYNTHETIC BODY','unknown')",[&[7u8;32][..]])?;
    drop(c);
    if args == ["--make-revoked-schema7"] {
        let backup = Path::new("/fixture/schema7-backup");
        fs::create_dir(backup)?;
        fs::set_permissions(backup, fs::Permissions::from_mode(0o700))?;
        let s = Store::migrate_recovery_grants(path, backup)?;
        s.handle().change_recovery_grant(
            owner.clone(),
            [2; 32],
            0,
            "00000000-0000-4000-8000-000000000003".into(),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )?;
        s.handle().change_recovery_grant(
            owner,
            [2; 32],
            1,
            "00000000-0000-4000-8000-000000000004".into(),
            RecoveryChange::Revoke,
        )?;
    }
    Ok(())
}
#[cfg(not(feature = "dev-fixtures"))]
fn main() {
    eprintln!("fixture build required");
    std::process::exit(4);
}
