// SPDX-License-Identifier: AGPL-3.0-only
//! C14: real public APIs and isolated synthetic state. Never executes application payloads.
#![cfg(target_os = "linux")]
mod c14_support;
use c14_support::*;
use rustsimhub_update::Error;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "only spawned by the two bounded parent tests"]
fn c14_process_worker() {
    let root = PathBuf::from(std::env::var_os("C14_ROOT").unwrap());
    assert_eq!(root.canonicalize().unwrap(), root);
    let phase = std::env::var("C14_PHASE").unwrap();
    assert!(phase == "stage" || phase == "choose");
    let format = std::env::var("C14_FORMAT").unwrap().parse().unwrap();
    let p = package(format, 2, 6);
    let mut s = open(&root);
    assert_eq!(
        s.stage(&p.manifest, &p.signature, &p.payloads),
        Ok(p.hash.clone())
    );
    if phase == "choose" {
        assert_eq!(s.choose().unwrap(), root.join("versions").join(&p.hash));
    }
    println!("\nC14_READY {phase} {}", p.hash);
    std::io::stdout().flush().unwrap();
    let mut stop = [0u8; 1];
    std::io::stdin().read_exact(&mut stop).unwrap();
    panic!("worker must be killed by its parent after verified handshake");
}
fn interrupted(phase: &str) {
    use std::os::unix::process::ExitStatusExt;
    for format in [1, 2] {
        let f = Fixture::new(format, None);
        let p = package(format, 2, 6);
        let original = snapshot(&f.a);
        let mut worker = Worker(
            Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "c14_process_worker", "--nocapture"])
                .env_clear()
                .env("C14_ROOT", f.root.path())
                .env("C14_PHASE", phase)
                .env("C14_FORMAT", format.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let pipe = worker.0.stdout.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().take(16) {
                let line = line.unwrap();
                if line.starts_with("C14_READY ") {
                    let _ = tx.send(line);
                    break;
                }
            }
        });
        let ready = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("exact API-completion handshake required");
        assert_eq!(ready, format!("C14_READY {phase} {}", p.hash));
        reader.join().unwrap();
        let disk: Value = serde_json::from_slice(&f.ledger()).unwrap();
        assert_eq!(disk["floor"], 2);
        assert_eq!(disk["attempted"], phase == "choose");
        assert_eq!(disk["candidate"]["hash"], p.hash);
        worker.0.kill().unwrap();
        assert_eq!(worker.0.wait().unwrap().signal(), Some(9));
        assert_eq!(snapshot(&f.a), original);
        let mut reopened = open(f.root.path());
        if phase == "stage" {
            assert_eq!(reopened.choose().unwrap(), f.target(&p));
            drop(reopened);
            reopened = open(f.root.path());
        }
        assert_eq!(reopened.choose().unwrap(), f.a);
        assert_eq!(reopened.floor(), 2);
        assert!(reopened.status()["candidate"].is_null());
        assert_eq!(snapshot(&f.a), original);
        println!("C14_PROCESS phase={phase} format={format} signal=9 handshake=true floor=2 fallback=true");
    }
}
#[test]
fn c14_stage_completed_then_sigkill_reopens_candidate_once() {
    interrupted("stage");
}
#[test]
fn c14_chosen_unconfirmed_then_sigkill_reopens_fallback() {
    interrupted("choose");
}

#[test]
fn c14_signature_and_schema_rejection_preserve_actual_store() {
    use ed25519_dalek::{Signer, SigningKey};
    for format in [1, 2] {
        for fault in ["key", "payload", "schema"] {
            let f = Fixture::new(format, None);
            let mut s = open(f.root.path());
            let before = f.ledger();
            let original = snapshot(&f.a);
            let mut p = package(format, 2, if fault == "schema" { 99 } else { 6 });
            if fault == "key" {
                p.signature = SigningKey::from_bytes(&[78; 32])
                    .sign(&rustsimhub_update::signing_bytes(&p.manifest).unwrap())
                    .to_bytes();
            }
            if fault == "payload" {
                p.payloads[0][63] ^= 1;
            }
            let expected = match fault {
                "key" => Error::Signature,
                "payload" => Error::Integrity,
                _ => Error::Schema,
            };
            assert_eq!(
                s.stage(&p.manifest, &p.signature, &p.payloads),
                Err(expected)
            );
            assert_eq!(f.ledger(), before);
            assert_eq!(snapshot(&f.a), original);
            assert!(!f.target(&p).exists());
            drop(s);
            let mut s = open(f.root.path());
            assert_eq!(s.choose().unwrap(), f.a);
            assert_eq!(s.floor(), 1);
        }
    }
}

fn programs(format: i64) -> Value {
    let runtime = json!({"format":1,"worker_protocol":4,"outbox":{"application_id":0x52534657,"read_format":format,"write_format":format,"migrate_from":(1..format).collect::<Vec<_>>()}});
    json!({"core":json!({"component":"core","target_arch":"aarch64","source_archive_sha256":"a".repeat(64),"database_schema":6,
        "development_fixtures":false,"service_launch":{"format":1,"modes":["device","forwarding_managed","setup_ap"]},
        "setup_ap_installation_contract":rustsimhub_core_launcher::installation_contract(),"forwarding_runtime_contract":runtime}).to_string(),
        "worker":json!({"worker":true,"protocol":4,"source_archive_sha256":"a".repeat(64),"development_fixtures":false,"forwarding_runtime_contract":runtime}).to_string()})
}
#[test]
fn c14_real_outbox_unknown_and_projected_fallback_rejection_preserve_receipts() {
    use rustsimhub_forwarder::{inspect_outbox, Binding, Policy, Queue, State};
    let d = tempfile::tempdir().unwrap();
    mode(d.path(), 0o700);
    let binding = Binding {
        device_key: [1; 32],
        owner_key: [2; 32],
        owner_generation: 1,
        realm: "00000000-0000-4000-8000-000000000001".into(),
    };
    let target = "00000000-0000-4000-8000-000000000002";
    let request = "00000000-0000-4000-8000-000000000003";
    let policy:Policy=serde_json::from_value(json!({"enabled":false,"targets":[{"id":target,"destination":{"kind":"webhook","url":"https://example.invalid/synthetic","bearer":null}}],
        "rules":[{"id":"00000000-0000-4000-8000-000000000004","targets":[target],"sender_equals":null,"body_contains":null,"line":null,"share":"notice","conditions":null}]})).unwrap();
    let policy_again = serde_json::to_value(&policy).unwrap();
    let mut q = Queue::open(d.path(), binding.clone()).unwrap();
    q.configure(
        &binding,
        0,
        "00000000-0000-4000-8000-000000000005",
        policy,
        serde_json::from_value(json!({"db_epoch":binding.realm,"seq":"5"})).unwrap(),
    )
    .unwrap();
    q.begin_probe(&binding, 1, request, target).unwrap();
    q.enqueue_test(
        &binding,
        1,
        request,
        target,
        serde_json::from_value(json!(2)).unwrap(),
    )
    .unwrap();
    let claim = q
        .claim(&binding, 1, serde_json::from_value(json!(2)).unwrap())
        .unwrap()
        .unwrap();
    q.finish(&binding, &claim, State::Unknown).unwrap();
    q.end_probe(request).unwrap();
    let receipt =
        serde_json::to_value(q.test_receipt(&binding, request).unwrap().unwrap()).unwrap();
    let cursor = serde_json::to_value(q.event_cursor().unwrap()).unwrap();
    let attempt = claim.attempt_id.clone();
    drop(q);
    let before = snapshot(d.path());
    let observed = inspect_outbox(d.path()).unwrap();
    let version = observed.version();
    assert_eq!(version.format, rustsimhub_forwarder::OUTBOX_CURRENT_FORMAT);
    drop(observed);
    assert_eq!(snapshot(d.path()), before);
    // A real current outbox, then an actual old-format database; neither comparison migrates it.
    let db = rusqlite::Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    let payload: String = db
        .query_row("SELECT request FROM jobs WHERE id=?", [&claim.id], |r| {
            r.get(0)
        })
        .unwrap();
    db.execute_batch("DROP TABLE history_batches; PRAGMA user_version=2;")
        .unwrap();
    drop(db);
    let before = snapshot(d.path());
    let observed = inspect_outbox(d.path()).unwrap();
    let version = observed.version();
    assert_eq!(version.format, 2);
    let business = tempfile::tempdir().unwrap();
    mode(business.path(), 0o700);
    drop(rustsimhub_storage::Store::open(business.path()).unwrap());
    let db = rusqlite::Connection::open(business.path().join("store.sqlite3")).unwrap();
    let schema: u32 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    drop(db);
    assert!(schema < 99);
    let business_before = snapshot(business.path());
    let input = json!({"format":1,"target_arch":"aarch64","root_mode":"setup_ap","root_contract":rustsimhub_core_launcher::installation_contract().to_string(),
        "data":{"business_schema":schema,"outbox":{"application_id":version.application_id,"format":version.format}},
        "selected":programs(3),"candidate":programs(3),"fallback":programs(2)});
    let report =
        rustsimhub_core_launcher::check_upgrade(&serde_json::to_vec(&input).unwrap()).unwrap();
    assert_eq!(report.exit_code(), 4);
    let report = serde_json::to_value(report).unwrap();
    assert_eq!(report["projected_data"]["outbox"]["format"], 3);
    assert_eq!(
        report["fallback_after_candidate"]["reasons"],
        json!(["unsupported_outbox"])
    );
    assert_eq!(report["authorization_verified"], false);
    drop(observed);
    assert_eq!(snapshot(d.path()), before);
    assert_eq!(snapshot(business.path()), business_before);
    let db = rusqlite::Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    assert_eq!(
        db.query_row::<String, _, _>("SELECT request FROM jobs WHERE id=?", [&claim.id], |r| r
            .get(0))
            .unwrap(),
        payload
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT attempt_id FROM jobs WHERE id=?", [&claim.id], |r| r
            .get(0))
            .unwrap(),
        attempt
    );
    drop(db);
    if rustsimhub_forwarder::email_revision().is_some() {
        // Candidate4 deliberately refuses automatic legacy2 migration. The
        // existing comparison remains read-only and every old receipt survives.
        assert!(matches!(
            Queue::open(d.path(), binding.clone()),
            Err(rustsimhub_forwarder::Error::Forbidden)
        ));
        assert_eq!(snapshot(d.path()), before);
        return;
    }
    let mut reopened = Queue::open(d.path(), binding.clone()).unwrap();
    let db = rusqlite::Connection::open_with_flags(
        d.path().join("outbox.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row::<String, _, _>("SELECT request FROM jobs WHERE id=?", [&claim.id], |r| r
            .get(0))
            .unwrap(),
        payload
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT attempt_id FROM jobs WHERE id=?", [&claim.id], |r| r
            .get(0))
            .unwrap(),
        attempt
    );
    drop(db);
    assert_eq!(
        serde_json::to_value(reopened.test_receipt(&binding, request).unwrap().unwrap()).unwrap(),
        receipt
    );
    assert_eq!(
        serde_json::to_value(reopened.event_cursor().unwrap()).unwrap(),
        cursor
    );
    let mut enabled: Policy = serde_json::from_value(policy_again).unwrap();
    enabled.enabled = true;
    reopened
        .configure(
            &binding,
            1,
            "00000000-0000-4000-8000-000000000006",
            enabled,
            serde_json::from_value(cursor).unwrap(),
        )
        .unwrap();
    assert!(reopened
        .claim(&binding, 2, serde_json::from_value(json!(2)).unwrap())
        .unwrap()
        .is_none());
    assert_eq!(
        reopened
            .test_receipt(&binding, request)
            .unwrap()
            .unwrap()
            .state,
        State::Unknown
    );
}

struct Filler(PathBuf);
impl Drop for Filler {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
#[test]
#[ignore = "requires the private 2MiB tmpfs runner; never fill an ordinary filesystem"]
fn c14_tmpfs_enospc() {
    let root = PathBuf::from(std::env::var_os("C14_TMPFS").unwrap());
    assert_eq!(root.canonicalize().unwrap(), root);
    assert_eq!(root.file_name().unwrap(), "full");
    assert_eq!(
        root.parent().unwrap().parent().unwrap(),
        Path::new("/var/tmp")
    );
    assert!(root
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("rustsimhub-updater-full-"));
    assert_eq!(rustix::process::geteuid().as_raw(), 65534);
    assert_eq!(rustix::fs::statfs(&root).unwrap().f_type, 0x0102_1994);
    let capacity = fs2::statvfs(&root).unwrap().total_space();
    assert_eq!(capacity, 2 * 1024 * 1024);
    let status = fs::read_to_string("/proc/self/status").unwrap();
    for (key, value) in [
        ("CapEff:", "0000000000000000"),
        ("CapPrm:", "0000000000000000"),
        ("CapAmb:", "0000000000000000"),
        ("NoNewPrivs:", "1"),
    ] {
        assert_eq!(
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .unwrap()
                .trim(),
            value
        );
    }
    for format in [1, 2] {
        let f = Fixture::new(format, Some(&root));
        let mut s = open(f.root.path());
        let p = package(format, 2, 6);
        let ledger = f.ledger();
        let original = snapshot(&f.a);
        let filler = Filler(root.join("synthetic-filler"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&filler.0)
            .unwrap();
        let mut written = 0;
        loop {
            assert!(written <= capacity);
            match file.write(&[0; 65536]) {
                Ok(n) => {
                    assert!(n > 0);
                    written += n as u64;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    assert_eq!(e.raw_os_error(), Some(28));
                    break;
                }
            }
        }
        assert_eq!(fs2::statvfs(&root).unwrap().available_space(), 0);
        assert_eq!(
            s.stage(&p.manifest, &p.signature, &p.payloads),
            Err(Error::Io)
        );
        assert_eq!(f.ledger(), ledger);
        assert_eq!(snapshot(&f.a), original);
        assert_eq!(s.floor(), 1);
        drop(s);
        let orphan = if f.target(&p).exists() {
            Some(snapshot(&f.target(&p)))
        } else {
            None
        };
        drop(file);
        drop(filler);
        let mut reopened = open(f.root.path());
        assert_eq!(reopened.choose().unwrap(), f.a);
        assert_eq!(reopened.floor(), 1);
        assert_eq!(f.ledger(), ledger);
        if let Some(original) = orphan {
            assert_eq!(snapshot(&f.target(&p)), original);
        }
        // Separate writable control, not a retry that deletes the failed package.
        let control = Fixture::new(format, Some(&root));
        let mut s = open(control.root.path());
        assert_eq!(
            s.stage(&p.manifest, &p.signature, &p.payloads),
            Ok(p.hash.clone())
        );
        assert_eq!(s.choose().unwrap(), control.target(&p));
        println!("C14_TMPFS_PASSED format={format} capacity={capacity} uid=65534 filler_errno=28 stage_error=Io baseline_preserved=true");
    }
}
