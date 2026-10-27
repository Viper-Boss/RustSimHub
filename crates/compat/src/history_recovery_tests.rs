// SPDX-License-Identifier: AGPL-3.0-only
//! C-06: real importer/SQLite failures, isolated from library lock fixtures.
use rusqlite::{params, Connection};
use rustsimhub_compat::history::{self, Error, HistoryService};
use rustsimhub_domain::{HistoryCursor, HistoryEntity as Entity};
use rustsimhub_policy::{Kind, Principal, ALL_SCOPES};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const REALM: &str = "ce24de2e-57ee-456e-999a-a62899f96a23";
const OTHER_REALM: &str = "3419ebeb-60c4-4088-b96d-366c64fc868c";
const BULK: usize = 3000;
type RecordRow = (i64, String, String, String, String, Vec<u8>);

#[derive(Debug, PartialEq, Eq)]
struct ArchiveState {
    records: Vec<RecordRow>,
    batches: Vec<(String, Vec<u8>)>,
}
fn state(archive: &Path) -> ArchiveState {
    let db = Connection::open(archive.join("history.sqlite")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let records = db
        .prepare(
            "SELECT seq,source,entity,legacy_key,payload,digest FROM history_records ORDER BY seq",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let batches = db
        .prepare(
            "SELECT source,snapshot_digest FROM history_batches ORDER BY source,snapshot_digest",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    ArchiveState { records, batches }
}
fn private_dir(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
struct Fixture {
    root: tempfile::TempDir,
    input: PathBuf,
    archive: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let input = root.path().join("snapshot.sqlite");
        let db = Connection::open(&input).unwrap();
        db.execute_batch(include_str!("history/test-schema.sql"))
            .unwrap();
        drop(db);
        fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
        let archive = root.path().join("archive");
        private_dir(&archive);
        // Adjacent synthetic sentinels, not a replacement Core or live Store.
        let current = Connection::open(root.path().join("current.sqlite")).unwrap();
        current.execute_batch("CREATE TABLE current_line(id TEXT,generation INTEGER); INSERT INTO current_line VALUES('line_synthetic',1); CREATE TABLE sms_operations(id TEXT,status TEXT); INSERT INTO sms_operations VALUES('same-old-request','Unknown'); CREATE TABLE call_operations(id TEXT,status TEXT); INSERT INTO call_operations VALUES('same-old-call','confirmed');").unwrap();
        drop(current);
        fs::write(
            root.path().join("identity.seed"),
            b"synthetic unchanged identity",
        )
        .unwrap();
        fs::write(root.path().join("owner.json"), b"synthetic unchanged owner").unwrap();
        Self {
            root,
            input,
            archive,
        }
    }
    fn access(&self, name: &str) -> PathBuf {
        let p = self.root.path().join(name);
        private_dir(&p);
        p
    }
    fn protected(&self) -> Vec<Vec<u8>> {
        [
            self.input.clone(),
            self.root.path().join("current.sqlite"),
            self.root.path().join("identity.seed"),
            self.root.path().join("owner.json"),
        ]
        .iter()
        .map(|p| fs::read(p).unwrap())
        .collect()
    }
    fn bulk(&self) {
        let mut db = Connection::open(&self.input).unwrap();
        let tx = db.transaction().unwrap();
        for id in 1000..1000 + BULK {
            tx.execute("INSERT INTO sms(id,line_id,content,type,state) VALUES(?1,'line_synthetic',?2,2,'submitting')", params![id as i64, "S".repeat(2048)]).unwrap();
        }
        tx.commit().unwrap();
    }
}
fn principal(key: u8, kind: Kind) -> Principal {
    Principal {
        id: format!("synthetic-{key}"),
        public_key: [key; 32],
        kind,
        scopes: ALL_SCOPES,
        generation: 1,
        active: true,
    }
}
fn flags(r: &history::Report) {
    assert!(r.history_only && !r.credentials_imported);
    assert_eq!(r.hardware_operations, 0);
}

#[test]
fn repeated_packages_keep_exact_ids_payloads_and_one_batch_marker() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    let protected = f.protected();
    let first = history::import(&f.input, A, &f.archive).unwrap();
    flags(&first);
    assert_eq!((first.inserted, first.unchanged), (15, 0));
    let original = state(&f.archive);
    let bytes = fs::read(f.archive.join("history.sqlite")).unwrap();
    for _ in 0..5 {
        let r = history::import(&f.input, A, &f.archive).unwrap();
        flags(&r);
        assert_eq!((r.inserted, r.unchanged), (0, 15));
        assert_eq!(state(&f.archive), original);
        assert_eq!(fs::read(f.archive.join("history.sqlite")).unwrap(), bytes);
    }
    assert_eq!(original.batches.len(), 1);
    assert_eq!(f.protected(), protected);
}

#[test]
fn late_conflict_rolls_back_earlier_new_ids_and_distinct_sources_stay_distinct() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    history::import(&f.input, A, &f.archive).unwrap();
    let before = state(&f.archive);
    let db = Connection::open(&f.input).unwrap();
    db.execute_batch("INSERT INTO sms(id,line_id,content,type) VALUES(1,'line_synthetic','new before conflict',1); UPDATE sms SET content='different same-source content' WHERE id=42;").unwrap();
    drop(db);
    let protected = f.protected();
    assert!(matches!(
        history::import(&f.input, A, &f.archive),
        Err(Error::Conflict)
    ));
    assert_eq!(state(&f.archive), before);
    let r = history::import(&f.input, B, &f.archive).unwrap();
    assert_eq!((r.inserted, r.unchanged), (16, 0));
    let after = state(&f.archive);
    assert_eq!(
        after
            .records
            .iter()
            .filter(|r| r.1 == A)
            .cloned()
            .collect::<Vec<_>>(),
        before.records
    );
    assert_eq!(after.records.len(), 31);
    assert_eq!(after.batches.len(), 2);
    for _ in 0..3 {
        let r = history::import(&f.input, B, &f.archive).unwrap();
        assert_eq!((r.inserted, r.unchanged), (0, 16));
        assert_eq!(state(&f.archive), after);
    }
    assert_eq!(f.protected(), protected);
}

#[test]
fn sqlite_abort_after_three_real_inserts_preserves_original_batch_and_can_rerun() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    history::import(&f.input, A, &f.archive).unwrap();
    let db = Connection::open(f.archive.join("history.sqlite")).unwrap();
    // Condition only fires after this transaction inserted all three B SMS rows.
    db.execute_batch(&format!("CREATE TRIGGER synthetic_write_failure BEFORE INSERT ON history_records WHEN NEW.source='{B}' AND NEW.entity='contacts' AND (SELECT COUNT(*) FROM history_records WHERE source=NEW.source AND entity='sms')=3 BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;")).unwrap();
    drop(db);
    let before = state(&f.archive);
    let protected = f.protected();
    for _ in 0..2 {
        assert!(matches!(
            history::import(&f.input, B, &f.archive),
            Err(Error::Database)
        ));
        assert_eq!(state(&f.archive), before);
    }
    let db = Connection::open(f.archive.join("history.sqlite")).unwrap();
    db.execute_batch("DROP TRIGGER synthetic_write_failure")
        .unwrap();
    drop(db);
    assert_eq!(
        history::import(&f.input, B, &f.archive).unwrap().inserted,
        15
    );
    assert_eq!(
        history::import(&f.input, B, &f.archive).unwrap().unchanged,
        15
    );
    assert_eq!(state(&f.archive).records.len(), 30);
    assert_eq!(state(&f.archive).batches.len(), 2);
    assert_eq!(f.protected(), protected);
}

// Only the parent fixtures invoke this ignored test in another OS process.
// The worker calls the production importer; no alternate migration exists.
#[test]
#[ignore = "bounded subprocess worker invoked by C-06 parent tests"]
fn import_worker() {
    let input = std::env::var_os("RSH_C06_INPUT").expect("parent input");
    let archive = std::env::var_os("RSH_C06_ARCHIVE").expect("parent archive");
    match history::import(Path::new(&input), B, Path::new(&archive)) {
        Ok(r) => {
            flags(&r);
            if std::env::var_os("RSH_C06_LOSE_REPLY").is_some() {
                // Committed successfully, but the caller receives no report.
                std::process::exit(72);
            }
            println!("C06_IMPORT_OK:{}:{}", r.inserted, r.unchanged);
        }
        Err(error) => panic!("C06_IMPORT_DENIED:{error:?}"),
    }
}
struct Worker(Option<Child>);
impl Worker {
    fn spawn(f: &Fixture, file_limit: Option<u64>, lose_reply: bool) -> Self {
        let executable = std::env::current_exe().unwrap();
        let mut cmd = if let Some(limit) = file_limit {
            // Safe OS resource setup, then exec the very same Rust test binary.
            // Ignoring SIGXFSZ makes the actual write return EFBIG to SQLite.
            let mut c = Command::new("python3");
            c.arg("-c").arg("import os,resource,signal,sys; n=int(sys.argv[1]); resource.setrlimit(resource.RLIMIT_CORE,(0,0)); resource.setrlimit(resource.RLIMIT_FSIZE,(n,n)); signal.signal(signal.SIGXFSZ,signal.SIG_IGN); os.execv(sys.argv[2],sys.argv[2:])").arg(limit.to_string()).arg(&executable);
            c
        } else {
            Command::new(&executable)
        };
        cmd.args([
            "--exact",
            "history_recovery_tests::import_worker",
            "--ignored",
            "--nocapture",
        ])
        .env("RSH_C06_INPUT", &f.input)
        .env("RSH_C06_ARCHIVE", &f.archive)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
        if lose_reply {
            cmd.env("RSH_C06_LOSE_REPLY", "1");
        } else {
            cmd.env_remove("RSH_C06_LOSE_REPLY");
        }
        Self(Some(cmd.spawn().unwrap()))
    }
    fn finish(&mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "bounded importer worker timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn killed_import_with_real_journal_reopens_without_partial_records_and_reruns() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    history::import(&f.input, A, &f.archive).unwrap();
    let before = state(&f.archive);
    f.bulk();
    let protected = f.protected();
    let path = f.archive.join("history.sqlite");
    let blocker = Connection::open(&path).unwrap();
    blocker
        .execute_batch("BEGIN; SELECT COUNT(*) FROM history_records;")
        .unwrap();
    let mut worker = Worker::spawn(&f, None, false);
    let journal = f.archive.join("history.sqlite-journal");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            worker.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "worker exited before interruption"
        );
        if fs::metadata(&journal).is_ok_and(|m| m.len() > 4096) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real SQLite journal was not observed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    // Reader lock prevents commit; the nonempty journal proves write work began.
    worker.0.as_mut().unwrap().kill().unwrap();
    let output = worker.finish();
    assert_eq!(output.status.signal(), Some(9));
    blocker.execute_batch("ROLLBACK").unwrap();
    drop(blocker);
    assert_eq!(state(&f.archive), before); // real SQLite reopen/recovery
    let r = history::import(&f.input, B, &f.archive).unwrap();
    assert_eq!((r.inserted, r.unchanged), (BULK + 15, 0));
    let committed = state(&f.archive);
    assert_eq!(committed.records.len(), BULK + 30);
    assert_eq!(committed.batches.len(), 2);
    let again = history::import(&f.input, B, &f.archive).unwrap();
    assert_eq!((again.inserted, again.unchanged), (0, BULK + 15));
    assert_eq!(state(&f.archive), committed);
    assert_eq!(f.protected(), protected);
}

#[test]
fn actual_file_size_write_failure_recovers_new_and_existing_archives() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    const LIMIT: u64 = 128 * 1024;
    for seeded in [false, true] {
        let f = Fixture::new();
        let before = if seeded {
            history::import(&f.input, A, &f.archive).unwrap();
            Some(state(&f.archive))
        } else {
            None
        };
        f.bulk();
        let protected = f.protected();
        let output = Worker::spawn(&f, Some(LIMIT), false).finish();
        assert!(!output.status.success());
        assert_eq!(
            output.status.signal(),
            None,
            "must return write failure, not die from SIGXFSZ"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("C06_IMPORT_DENIED:Database"));
        let path = f.archive.join("history.sqlite");
        assert!(fs::metadata(&path).unwrap().len() <= LIMIT);
        let recovered = state(&f.archive);
        if let Some(before) = &before {
            assert_eq!(&recovered, before);
        } else {
            assert!(recovered.records.is_empty() && recovered.batches.is_empty());
        }
        let r = history::import(&f.input, B, &f.archive).unwrap();
        assert_eq!((r.inserted, r.unchanged), (BULK + 15, 0));
        flags(&r);
        assert!(fs::metadata(&path).unwrap().len() > LIMIT);
        assert_eq!(
            history::import(&f.input, B, &f.archive).unwrap().unchanged,
            BULK + 15
        );
        assert_eq!(state(&f.archive).batches.len(), if seeded { 2 } else { 1 });
        assert_eq!(f.protected(), protected);
    }
}

#[test]
fn committed_import_with_lost_report_retries_without_reassigning_ids() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    let protected = f.protected();
    let output = Worker::spawn(&f, None, true).finish();
    assert_eq!(output.status.code(), Some(72));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("C06_IMPORT_OK"));
    let committed = state(&f.archive);
    assert_eq!(committed.records.len(), 15);
    assert_eq!(committed.batches.len(), 1);
    for _ in 0..3 {
        let r = history::import(&f.input, B, &f.archive).unwrap();
        assert_eq!((r.inserted, r.unchanged), (0, 15));
        assert_eq!(state(&f.archive), committed);
    }
    assert_eq!(f.protected(), protected);
}

#[test]
fn realm_generation_and_archive_changes_never_reuse_old_authority_or_live_operations() {
    let _guard = crate::OFFLINE_CLI_TEST_LOCK.lock().unwrap();
    let f = Fixture::new();
    let db = Connection::open(&f.input).unwrap();
    db.execute_batch("UPDATE sms SET request_id='same-old-request',state='submitting' WHERE id=44; UPDATE call_history SET request_id='same-old-call',phase='dialing';").unwrap();
    drop(db);
    history::import(&f.input, A, &f.archive).unwrap();
    let access = f.access("access");
    let owner = principal(1, Kind::Owner);
    let mut nas = principal(2, Kind::Nas);
    let mut service = HistoryService::open(&f.archive, &access, [77; 32], REALM).unwrap();
    service
        .grant(&owner, &nas, A, None, &[Entity::Sms, Entity::Calls])
        .unwrap();
    let page = service.page(&nas, A, Entity::Sms, None, 1).unwrap();
    let cursor: HistoryCursor = serde_json::from_value(page["cursor"].clone()).unwrap();
    let old_id = page["records"][0]["id"].as_str().unwrap().to_owned();
    drop(service);
    // Reused legacy line name and changed synthetic current SIM generation.
    let db = Connection::open(f.root.path().join("current.sqlite")).unwrap();
    db.execute_batch("UPDATE current_line SET generation=2")
        .unwrap();
    drop(db);
    let protected = f.protected();
    let mut service = HistoryService::open(&f.archive, &access, [77; 32], REALM).unwrap();
    assert_eq!(
        service
            .page(&nas, A, Entity::Sms, Some(&cursor), 1)
            .unwrap()["archived"],
        true
    );
    nas.generation = 2;
    assert!(matches!(
        service.page(&nas, A, Entity::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    service
        .grant(&owner, &nas, A, None, &[Entity::Sms])
        .unwrap();
    assert!(matches!(
        service.page(&nas, A, Entity::Sms, Some(&cursor), 1),
        Err(Error::Forbidden)
    ));
    let generation_cursor: HistoryCursor = serde_json::from_value(
        service.page(&nas, A, Entity::Sms, None, 1).unwrap()["cursor"].clone(),
    )
    .unwrap();
    drop(service);
    assert!(matches!(
        HistoryService::open(&f.archive, &access, [77; 32], OTHER_REALM),
        Err(Error::Forbidden)
    ));
    let fresh = f.access("fresh-realm-access");
    let service = HistoryService::open(&f.archive, &fresh, [77; 32], OTHER_REALM).unwrap();
    assert!(service.sources(&nas).unwrap().is_empty());
    assert!(matches!(
        service.page(&nas, A, Entity::Sms, Some(&generation_cursor), 1),
        Err(Error::Forbidden)
    ));
    drop(service);
    let mapping = state(&f.archive).records;
    history::import(&f.input, B, &f.archive).unwrap();
    assert!(matches!(
        HistoryService::open(&f.archive, &access, [77; 32], REALM),
        Err(Error::Forbidden)
    ));
    let new_access = f.access("new-archive-access");
    let mut service = HistoryService::open(&f.archive, &new_access, [77; 32], REALM).unwrap();
    service
        .grant(&owner, &nas, A, None, &[Entity::Sms, Entity::Calls])
        .unwrap();
    service
        .grant(&owner, &nas, B, None, &[Entity::Sms])
        .unwrap();
    assert!(matches!(
        service.page(&nas, A, Entity::Sms, Some(&generation_cursor), 1),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        service.detail(&nas, B, Entity::Sms, &old_id),
        Err(Error::Forbidden)
    ));
    for entity in [Entity::Sms, Entity::Calls] {
        let page = service.page(&nas, A, entity, None, 16).unwrap();
        assert_eq!(page["archived"], true);
        for r in page["records"].as_array().unwrap() {
            assert_eq!(r["archived"], true);
            let detail = service
                .detail(&nas, A, entity, r["id"].as_str().unwrap())
                .unwrap();
            assert_eq!(detail["archived"], true);
        }
    }
    drop(service);
    assert_eq!(
        state(&f.archive)
            .records
            .into_iter()
            .filter(|r| r.1 == A)
            .collect::<Vec<_>>(),
        mapping
    );
    let db = Connection::open(f.archive.join("history.sqlite")).unwrap();
    // PRAGMA optimize may maintain sqlite_stat1/stat4; they are internal
    // statistics, not imported live business or operation tables.
    let names = db
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT GLOB 'sqlite_*' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(names, ["history_batches", "history_records"]);
    assert_eq!(f.protected(), protected);
}
