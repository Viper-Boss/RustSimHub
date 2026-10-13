// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Mock {
    effects: Mutex<Vec<&'static str>>,
    owner_changes: bool,
    activation_fails: bool,
}
fn snapshot() -> Snapshot {
    Snapshot {
        owner: ":1.2".into(),
        mm_owner: ":1.3".into(),
        wifi: "/wifi".try_into().unwrap(),
        usb: "/usb".try_into().unwrap(),
        profile: "/profile".try_into().unwrap(),
        wwan: true,
        profile_identity: [7; 32],
    }
}
impl Backend for Mock {
    async fn snapshot(&self) -> Result<Snapshot> {
        Ok(snapshot())
    }
    async fn fence(&self, _: &Snapshot) -> Result<()> {
        if self.owner_changes {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
    async fn checkpoint(&self, _: &Snapshot) -> Result<OwnedObjectPath> {
        self.effects.lock().unwrap().push("checkpoint");
        Ok("/checkpoint".try_into().unwrap())
    }
    async fn activate(&self, _: &Snapshot) -> Result<()> {
        self.effects.lock().unwrap().push("wifi");
        if self.activation_fails {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn restore(&self, _: &Snapshot, _: &OwnedObjectPath) -> Result<()> {
        self.effects.lock().unwrap().push("restore");
        Ok(())
    }
    async fn restored(&self, _: &Snapshot) -> Result<()> {
        Ok(())
    }
}
struct Allow {
    checks: AtomicUsize,
    revoke_at: usize,
}
impl Gate for Allow {
    async fn check(&self) -> Result<()> {
        let n = self.checks.fetch_add(1, Ordering::SeqCst);
        if n >= self.revoke_at {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
}
fn mock() -> Mock {
    Mock {
        effects: Mutex::new(vec![]),
        owner_changes: false,
        activation_fails: false,
    }
}
fn p() -> Principal {
    Principal {
        id: "synthetic-client".into(),
        public_key: [7; 32],
        kind: rustsimhub_policy::Kind::Owner,
        scopes: rustsimhub_policy::ALL_SCOPES,
        generation: 1,
        active: true,
    }
}
fn directory() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
#[tokio::test]
async fn intent_replay_does_not_repeat_even_after_restart() {
    let d = directory();
    let b = mock();
    let g = Allow {
        checks: AtomicUsize::new(0),
        revoke_at: usize::MAX,
    };
    let mut j = Journal::new(d.path(), "key", &p()).unwrap();
    execute(&b, &g, &mut j).await.unwrap();
    assert_eq!(
        *b.effects.lock().unwrap(),
        ["checkpoint", "wifi", "restore"]
    );
    let re = Journal::new(d.path(), "key", &p()).unwrap();
    assert!(re.previous().unwrap().unwrap().rollback_verified);
    let uncertain = Journal::new(d.path(), "uncertain", &p()).unwrap();
    uncertain.begin().unwrap();
    assert!(Journal::new(d.path(), "uncertain", &p())
        .unwrap()
        .previous()
        .is_err());
    let mut changed = p();
    changed.generation += 1;
    assert!(Journal::new(d.path(), "key", &changed)
        .unwrap()
        .previous()
        .is_err());
}
#[tokio::test]
async fn revocation_before_activation_still_restores_owned_checkpoint() {
    let d = directory();
    let b = mock();
    let g = Allow {
        checks: AtomicUsize::new(0),
        revoke_at: 3,
    };
    let mut j = Journal::new(d.path(), "key", &p()).unwrap();
    assert!(execute(&b, &g, &mut j).await.is_err());
    assert_eq!(*b.effects.lock().unwrap(), ["checkpoint", "restore"]);
    assert!(j.previous().is_err());
}
#[tokio::test]
async fn owner_change_and_failed_activation_never_reapply() {
    let d = directory();
    let mut b = mock();
    b.owner_changes = true;
    let g = Allow {
        checks: AtomicUsize::new(0),
        revoke_at: usize::MAX,
    };
    let mut j = Journal::new(d.path(), "key", &p()).unwrap();
    assert!(execute(&b, &g, &mut j).await.is_err());
    assert!(b.effects.lock().unwrap().is_empty());
    b.owner_changes = false;
    b.activation_fails = true;
    assert!(execute(&b, &g, &mut j).await.is_err());
    assert_eq!(
        *b.effects.lock().unwrap(),
        ["checkpoint", "wifi", "restore"]
    );
    assert!(j.previous().is_err());
}
#[test]
fn private_journal_rejects_symlink_and_broad_directory() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let d = directory();
    let j = Journal::new(d.path(), "key", &p()).unwrap();
    std::fs::write(d.path().join("target"), b"preserve").unwrap();
    symlink(d.path().join("target"), &j.marker).unwrap();
    assert!(j.begin().is_err());
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Journal::new(d.path(), "other", &p()).is_err());
}
