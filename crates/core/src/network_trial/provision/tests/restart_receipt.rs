// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn bound(dir: &Path, p: &Principal) -> Audit {
    let mut a = Audit::new(dir, "once", p).unwrap();
    a.record = json!({"schema":2,"principal":p.id,"generation":p.generation,"binding":"synthetic-binding","trial_id":"a".repeat(64)});
    a
}
#[test]
fn interrupted_legacy_missing_and_terminal_records_never_resume_or_overwrite() {
    let d = directory();
    let p = principal();
    assert_eq!(
        receipt::read(d.path(), "absent", &p, "synthetic-binding").unwrap()["known"],
        false
    );
    let a = bound(d.path(), &p);
    a.begin().unwrap();
    for confirmed in [false, true] {
        if confirmed {
            a.confirm(&"a".repeat(64)).unwrap();
        }
        let v = receipt::read(d.path(), "once", &p, "synthetic-binding").unwrap();
        assert_eq!(v["state"], "unknown");
        assert_eq!(v["terminal_recorded"], false);
        assert_eq!(v["requires_confirmation"], false);
        assert!(a.begin().is_err());
    }
    a.finish(State::Saved).unwrap();
    let before = std::fs::read(&a.result).unwrap();
    let v = receipt::read(d.path(), "once", &p, "synthetic-binding").unwrap();
    assert_eq!(v["state"], "saved");
    assert_eq!(v["current_network_verified"], false);
    assert_eq!(std::fs::read(&a.result).unwrap(), before);
    let legacy = Audit::new(d.path(), "legacy", &p).unwrap();
    legacy.begin().unwrap();
    legacy.finish(State::Saved).unwrap();
    assert_eq!(
        receipt::read(d.path(), "legacy", &p, "synthetic-binding").unwrap()["known"],
        false
    );
    assert!(legacy.begin().is_err());
}
#[test]
fn foreign_principal_realm_generation_corrupt_and_linked_records_are_rejected() {
    let d = directory();
    let p = principal();
    let a = bound(d.path(), &p);
    a.begin().unwrap();
    assert!(receipt::read(d.path(), "once", &p, "other-realm-binding").is_err());
    let mut other = p.clone();
    other.generation += 1;
    assert!(receipt::read(d.path(), "once", &other, "synthetic-binding").is_err());
    other = p.clone();
    other.id = "other".into();
    assert!(receipt::read(d.path(), "once", &other, "synthetic-binding").is_err());
    let mut bad = a.record.clone();
    bad["state"] = json!("ready");
    a.write(&a.result, &bad).unwrap();
    assert!(receipt::read(d.path(), "once", &p, "synthetic-binding").is_err());
    std::fs::remove_file(&a.result).unwrap();
    std::os::unix::fs::symlink(&a.started, &a.result).unwrap();
    assert!(receipt::read(d.path(), "once", &p, "synthetic-binding").is_err());
}
fn login(a: &mut Auth) -> String {
    let key = SigningKey::from_bytes(&[201; 32]);
    let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
#[tokio::test]
async fn actual_process_restart_requires_fresh_auth_and_preserves_original_receipt() {
    let d = directory();
    let i = directory();
    let journal = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let mut a = Auth::new(
        rustsimhub_auth::Identity::open(i.path()).unwrap(),
        store.handle(),
    )
    .unwrap();
    let key = SigningKey::from_bytes(&[201; 32]);
    let c = a
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let old = login(&mut a);
    let auth = Arc::new(Mutex::new(a));
    let (p, binding) = receipt::authority(auth.clone(), old.clone()).await.unwrap();
    let mut audit = bound(journal.path(), &p);
    audit.record["binding"] = json!(binding);
    audit.begin().unwrap();
    audit.finish(State::Saved).unwrap();
    drop(auth);
    drop(store);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "network_trial::provision::tests::restart_receipt::restart_reader_child",
            "--nocapture",
        ])
        .env("RSH_RECEIPT_CHILD", "1")
        .env("RSH_RECEIPT_STORE", d.path())
        .env("RSH_RECEIPT_IDENTITY", i.path())
        .env("RSH_RECEIPT_AUDIT", journal.path())
        .env("RSH_RECEIPT_OLD_TOKEN", old)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("restart reader deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    assert!(audit.begin().is_err());
}
#[tokio::test]
#[ignore = "only launched by the explicit restart parent with synthetic persistent state"]
async fn restart_reader_child() {
    assert_eq!(std::env::var("RSH_RECEIPT_CHILD").unwrap(), "1");
    let store =
        rustsimhub_storage::Store::open(Path::new(&std::env::var("RSH_RECEIPT_STORE").unwrap()))
            .unwrap();
    let mut a = Auth::new(
        rustsimhub_auth::Identity::open(Path::new(&std::env::var("RSH_RECEIPT_IDENTITY").unwrap()))
            .unwrap(),
        store.handle(),
    )
    .unwrap();
    assert!(a
        .network_manager(&std::env::var("RSH_RECEIPT_OLD_TOKEN").unwrap())
        .is_err());
    let token = login(&mut a);
    let (p, binding) = receipt::authority(Arc::new(Mutex::new(a)), token)
        .await
        .unwrap();
    let v = receipt::read(
        Path::new(&std::env::var("RSH_RECEIPT_AUDIT").unwrap()),
        "once",
        &p,
        &binding,
    )
    .unwrap();
    assert_eq!(v["state"], "saved");
    assert_eq!(v["requires_confirmation"], false);
    assert_eq!(v["current_network_verified"], false);
}
