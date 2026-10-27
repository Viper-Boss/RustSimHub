// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_storage::Error as StorageError;

#[test]
fn wal_backup_captures_committed_data_and_omits_other_backup_domains() {
    let f = Fixture::new();
    let (store, _, _, _) = f.open();
    let context = observe(&store, [4; 32]);
    let message = incoming(&store, context.line, 1);
    let config = f.dir("configuration");
    write(
        &config.join("synthetic-policy.json"),
        b"{\"synthetic\":true}",
    );
    assert!(
        fs::metadata(f.state.join("store.sqlite3-wal"))
            .unwrap()
            .len()
            > 0
    );
    let before = logical(&f.state.join("store.sqlite3"));
    let backup = f.backup(&store, "backup");
    assert!(logical(&backup.join("backup.sqlite3")) == before);
    let files: Vec<_> = fs::read_dir(&backup)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(files, ["backup.sqlite3"]);
    let db = readonly(&backup.join("backup.sqlite3"));
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM inbox WHERE id=?", [&message], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        1
    );
    assert!(logical(&f.state.join("store.sqlite3")) == before);
    assert_eq!(
        store.handle().backup(&backup),
        Err(StorageError::BackupExists)
    );
    let staged = f.dir("interrupted-backup");
    write(
        &staged.join("backup.pending"),
        b"synthetic interrupted stage",
    );
    let evidence = manifest(&staged);
    assert!(store.handle().backup(&staged).is_err());
    assert!(manifest(&staged) == evidence);
    assert!(!staged.join("backup.sqlite3").exists());
}

#[test]
fn matching_identity_and_business_restore_recover_only_the_original_owner() {
    let f = Fixture::new();
    let (store, owner_key, owner, pin) = f.open();
    let client_dir = f.dir("client");
    let client = ClientKey::create(&client_dir).unwrap();
    let approved = store
        .handle()
        .approve_client(owner.clone(), client.public_key(), Kind::Nas, 1)
        .unwrap();
    let seed_backup = f.dir("identity-backup");
    Identity::load(&f.identity)
        .unwrap()
        .backup(&seed_backup)
        .unwrap();
    let backup = f.backup(&store, "backup");
    let target = f.dir("restored");
    let restored = Store::restore(&backup, &target).unwrap();
    let old_epoch = store.handle().cursor().unwrap().db_epoch;
    assert_ne!(restored.handle().cursor().unwrap().db_epoch, old_epoch);
    for p in restored.handle().principals().unwrap() {
        assert!(!p.active);
    }
    let recovered = recover_owner(&restored, &seed_backup, &owner_key, pin);
    assert_eq!(recovered.id, owner.id);
    assert!(recovered.active);
    assert_eq!(recovered.generation, owner.generation + 2);
    let nas = restored
        .handle()
        .principal(approved.public_key)
        .unwrap()
        .unwrap();
    assert_eq!(nas.id, approved.id);
    assert!(!nas.active);
    let mut auth = Auth::open(&seed_backup, restored.handle()).unwrap();
    assert!(auth.login_challenge(client.public_key()).is_err());
    let login = auth.login_challenge(owner_key.public_key()).unwrap();
    let token = auth
        .login_finish(
            &login.id,
            &owner_key
                .prove(&login, pin, ProofExpectation::Login)
                .unwrap(),
        )
        .unwrap();
    assert!(
        auth.authorize(token.expose(), rustsimhub_policy::Scope::StatusRead)
            .is_ok()
    );
    assert!(hash(&f.identity.join("identity.seed")) == hash(&seed_backup.join("identity.seed")));
}

#[test]
fn sms_and_calls_restore_preserve_ids_unknowns_and_invalidate_old_cursors() {
    let f = Fixture::new();
    let (store, owner_key, owner, pin) = f.open();
    let h = store.handle();
    let b = observe(&store, [4; 32]);
    let sms = incoming(&store, b.line, 1);
    let mut operations = Vec::new();
    let mut calls = Vec::new();
    for (index, state) in [
        SmsState::Prepared,
        SmsState::Submitting,
        SmsState::Unknown,
        SmsState::Submitted,
    ]
    .into_iter()
    .enumerate()
    {
        let op = h
            .prepare_sms(request(&owner.id, &format!("sms-{index}"), b.line))
            .unwrap();
        if state != SmsState::Prepared {
            h.begin_submit(&op.id, &owner.id).unwrap();
            if state == SmsState::Unknown {
                h.mark_unknown(&op.id, &owner.id).unwrap();
            }
            if state == SmsState::Submitted {
                h.mark_submitted(&op.id, &owner.id).unwrap();
            }
        }
        operations.push((op.id, state));
        let call = h
            .prepare_call(
                CallRequest {
                    idempotency_key: format!("call-{index}"),
                    line: b.line,
                    action: CallAction::Dial,
                    call_id: None,
                    payload: PHONE.into(),
                },
                owner.clone(),
                b.clone(),
                || true,
            )
            .unwrap();
        if state != SmsState::Prepared {
            h.begin_call(call.id.clone(), owner.clone(), b.clone(), || true)
                .unwrap();
            if state == SmsState::Unknown {
                h.finish_call(call.id.clone(), owner.id.clone(), SmsState::Unknown)
                    .unwrap();
            }
            if state == SmsState::Submitted {
                let observed = h
                    .observe_call(
                        b.clone(),
                        CallObservation {
                            source: "synthetic-call".into(),
                            number: PHONE.into(),
                            direction: 2,
                            state: 1,
                            reason: 0,
                        },
                        || true,
                    )
                    .unwrap();
                h.permit_call(
                    call.id.clone(),
                    owner.clone(),
                    b.clone(),
                    observed.id,
                    ("/org/freedesktop/ModemManager1/Call/0".into(), 1),
                    || true,
                )
                .unwrap();
                h.confirm_call(call.id.clone(), owner.clone(), b.clone(), || true)
                    .unwrap();
            }
        }
        calls.push((call.id, state));
    }
    let old = h.cursor().unwrap();
    let old_inbox = h.inbox(None, 16).unwrap().cursor;
    let old_calls = h.calls(None, 16).unwrap().cursor;
    let backup = f.backup(&store, "backup");
    let target = f.dir("restored");
    let restored = Store::restore(&backup, &target).unwrap();
    let p = recover_owner(&restored, &f.identity, &owner_key, pin);
    let rb = observe(&restored, [4; 32]);
    assert_eq!(rb.line, b.line);
    let rh = restored.handle();
    let mut permits = 0;
    let before_replay = logical(&target.join("store.sqlite3"));
    for (index, (id, prior)) in operations.into_iter().enumerate() {
        let op = rh.operation(&id, &owner.id).unwrap();
        assert_eq!(op.id, id);
        assert_eq!(op.line, b.line);
        assert_eq!(
            op.state,
            if prior == SmsState::Submitted {
                prior
            } else {
                SmsState::Unknown
            }
        );
        let replay = rh
            .prepare_sms(request(&owner.id, &format!("sms-{index}"), b.line))
            .unwrap();
        assert_eq!(replay.id, id);
        assert_eq!(replay.state, op.state);
        permits += usize::from(rh.begin_submit(&id, &owner.id).is_ok());
    }
    for (index, (id, prior)) in calls.into_iter().enumerate() {
        let op = rh.call_operation(id.clone(), p.clone()).unwrap();
        assert_eq!(op.id, id);
        assert_eq!(op.line, b.line);
        assert_eq!(
            op.state,
            if prior == SmsState::Submitted {
                prior
            } else {
                SmsState::Unknown
            }
        );
        let replay = rh
            .prepare_call(
                CallRequest {
                    idempotency_key: format!("call-{index}"),
                    line: b.line,
                    action: CallAction::Dial,
                    call_id: None,
                    payload: PHONE.into(),
                },
                p.clone(),
                rb.clone(),
                || true,
            )
            .unwrap();
        assert_eq!(replay.id, id);
        assert_eq!(replay.state, op.state);
        permits += usize::from(rh.begin_call(id, p.clone(), rb.clone(), || true).is_ok());
    }
    assert_eq!(permits, 0, "restored operations issued a new begin permit");
    assert!(logical(&target.join("store.sqlite3")) == before_replay);
    assert!(matches!(
        rh.events(old, 16),
        Err(StorageError::ResyncRequired)
    ));
    assert!(matches!(
        rh.inbox(Some(old_inbox), 16),
        Err(StorageError::ResyncRequired)
    ));
    assert!(matches!(
        rh.calls(Some(old_calls), 16),
        Err(StorageError::ResyncRequired)
    ));
    assert!(rh.sms_detail(sms.clone()).unwrap().text == "synthetic inbox 1");
    assert_eq!(rh.inbox(None, 16).unwrap().messages[0].id, sms);
    drop(restored);
    let reopened = Store::open(&target).unwrap();
    assert_eq!(reopened.handle().inbox(None, 16).unwrap().messages.len(), 1);
    assert_eq!(reopened.handle().calls(None, 16).unwrap().calls.len(), 1);
    let db = readonly(&target.join("store.sqlite3"));
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM sms_operations WHERE state='unknown'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM call_operations WHERE state='unknown'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
}

#[test]
fn repeated_restore_rotates_realms_and_never_overwrites_existing_targets() {
    let f = Fixture::new();
    let (store, _, owner, _) = f.open();
    let op = store
        .handle()
        .prepare_sms(request(&owner.id, "once", LineEpoch(1)))
        .unwrap();
    let backup = f.backup(&store, "backup");
    let before = manifest(&backup);
    let mut epochs = Vec::new();
    for n in 0..3 {
        let path = f.dir(&format!("restore-{n}"));
        let r = Store::restore(&backup, &path).unwrap();
        let current = r.handle().cursor().unwrap().db_epoch;
        assert!(!epochs.contains(&current));
        epochs.push(current);
        assert_eq!(
            r.handle().operation(&op.id, &owner.id).unwrap().state,
            SmsState::Unknown
        );
        assert!(matches!(
            Store::restore(&backup, &path),
            Err(StorageError::AlreadyOpen)
        ));
        drop(r);
        let evidence = manifest(&path);
        assert!(matches!(
            Store::restore(&backup, &path),
            Err(StorageError::BackupExists)
        ));
        assert!(manifest(&path) == evidence);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.handle().cursor().unwrap().db_epoch,
            *epochs.last().unwrap()
        );
    }
    assert!(manifest(&backup) == before);
}

#[test]
fn corrupt_missing_and_newer_database_backups_preserve_original_state() {
    let f = Fixture::new();
    let (store, _, owner, _) = f.open();
    store
        .handle()
        .prepare_sms(request(&owner.id, "original", LineEpoch(1)))
        .unwrap();
    let backup = f.backup(&store, "backup");
    let source = logical(&f.state.join("store.sqlite3"));
    let occupied = f.dir("occupied");
    let other = Store::open(&occupied).unwrap();
    other
        .handle()
        .prepare_sms(request("sentinel", "keep", LineEpoch(1)))
        .unwrap();
    drop(other);
    let occupied_before = manifest(&occupied);
    for (n, fault) in ["missing", "truncated", "newer", "foreign", "empty"]
        .into_iter()
        .enumerate()
    {
        let bad = f.dir(&format!("bad-{n}"));
        let path = bad.join("backup.sqlite3");
        if fault != "missing" {
            fs::copy(backup.join("backup.sqlite3"), &path).unwrap();
        }
        match fault {
            "truncated" => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(100)
                    .unwrap();
            }
            "empty" => {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(0)
                    .unwrap();
            }
            "newer" | "foreign" => {
                let db = Connection::open(&path).unwrap();
                db.execute_batch(if fault == "newer" {
                    "PRAGMA user_version=7"
                } else {
                    "PRAGMA application_id=0"
                })
                .unwrap();
            }
            _ => {}
        }
        let before = manifest(&bad);
        let destination = f.dir(&format!("rejected-{n}"));
        assert!(
            Store::restore(&bad, &destination).is_err(),
            "bad database was accepted"
        );
        if destination.join(".restore-pending").exists() {
            assert!(Store::open(&destination).is_err());
        }
        assert!(manifest(&bad) == before);
        assert!(Store::restore(&bad, &occupied).is_err());
        assert!(manifest(&occupied) == occupied_before);
    }
    assert!(logical(&f.state.join("store.sqlite3")) == source);
}

#[test]
fn wrong_owner_proof_never_reactivates_another_principal() {
    let f = Fixture::new();
    let (store, key, owner, pin) = f.open();
    let wrong_dir = f.dir("wrong-owner");
    let wrong = ClientKey::create(&wrong_dir).unwrap();
    let client = store
        .handle()
        .approve_client(owner.clone(), wrong.public_key(), Kind::Client, 1)
        .unwrap();
    let backup = f.backup(&store, "backup");
    let destination = f.dir("restored");
    let restored = Store::restore(&backup, &destination).unwrap();
    let mut auth = Auth::open(&f.identity, restored.handle()).unwrap();
    let before = logical(&destination.join("store.sqlite3"));
    assert!(auth.recovery_challenge(wrong.public_key()).is_err());
    let challenge = auth.recovery_challenge(key.public_key()).unwrap();
    assert!(auth.recovery_finish(&challenge.id, &[0; 64]).is_err());
    assert!(logical(&destination.join("store.sqlite3")) == before);
    let challenge = auth.recovery_challenge(key.public_key()).unwrap();
    let recovered = auth
        .recovery_finish(
            &challenge.id,
            &key.prove(&challenge, pin, ProofExpectation::RecoverOwner)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(recovered.id, owner.id);
    assert!(
        !restored
            .handle()
            .principal(client.public_key)
            .unwrap()
            .unwrap()
            .active
    );
}

#[test]
fn missing_outbox_and_configuration_are_not_reconstructed_by_store_restore() {
    let f = Fixture::new();
    let (store, _, _, _) = f.open();
    let cfg = f.dir("configuration");
    write(&cfg.join("synthetic-settings.json"), b"{\"fixture\":true}");
    let configuration = manifest(&cfg);
    let identity = manifest(&f.identity);
    let backup = f.backup(&store, "backup");
    let dest = f.dir("restored");
    let restored = Store::restore(&backup, &dest).unwrap();
    for missing in [
        "identity.seed",
        "outbox.sqlite3",
        "synthetic-settings.json",
        "bundle-manifest.json",
    ] {
        assert!(!backup.join(missing).exists());
        assert!(!dest.join(missing).exists());
    }
    assert!(manifest(&cfg) == configuration);
    assert!(manifest(&f.identity) == identity);
    assert!(restored.handle().identity_public_key().unwrap().is_some());
}

#[test]
fn database_backup_and_restore_path_failures_keep_existing_data() {
    let f = Fixture::new();
    let (store, _, _, _) = f.open();
    let backup = f.backup(&store, "backup");
    let original = manifest(&backup);
    let wrong_file = f.root.join("not-directory");
    write(&wrong_file, b"preserve path evidence");
    assert!(Store::restore(&backup, &wrong_file).is_err());
    assert!(Store::restore(&backup, f.root.join("missing-target")).is_err());
    let linked_source = f.root.join("linked-directory");
    symlink(&backup, &linked_source).unwrap();
    let destination = f.dir("destination");
    assert!(Store::restore(&linked_source, &destination).is_err());
    let linked_backup = f.dir("linked-file");
    symlink(
        backup.join("backup.sqlite3"),
        linked_backup.join("backup.sqlite3"),
    )
    .unwrap();
    assert!(Store::restore(&linked_backup, f.dir("target-link-file")).is_err());
    let linked_target = f.root.join("linked-target");
    symlink(&destination, &linked_target).unwrap();
    assert!(Store::restore(&backup, &linked_target).is_err());
    let pending_only = f.dir("pending-only");
    write(
        &pending_only.join("backup.pending"),
        b"synthetic incomplete backup",
    );
    let pending_before = manifest(&pending_only);
    assert!(Store::restore(&pending_only, f.dir("pending-target")).is_err());
    assert!(manifest(&pending_only) == pending_before);
    fs::set_permissions(
        backup.join("backup.sqlite3"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(matches!(
        Store::restore(&backup, f.dir("public-file-target")),
        Err(StorageError::Forbidden)
    ));
    assert!(manifest(&backup) == original);
    assert!(fs::read(wrong_file).unwrap() == b"preserve path evidence");
}

#[test]
fn restored_unknown_operations_cannot_become_new_current_line_actions() {
    let f = Fixture::new();
    let (store, key, owner, pin) = f.open();
    let before_line = observe(&store, [4; 32]);
    let sms = incoming(&store, before_line.line, 1);
    let op = store
        .handle()
        .prepare_sms(request(&owner.id, "same-key", before_line.line))
        .unwrap();
    store.handle().begin_submit(&op.id, &owner.id).unwrap();
    let backup = f.backup(&store, "backup");
    let destination = f.dir("restored");
    let restored = Store::restore(&backup, &destination).unwrap();
    recover_owner(&restored, &f.identity, &key, pin);
    let h = restored.handle();
    let before_replay = logical(&destination.join("store.sqlite3"));
    for _ in 0..3 {
        let replay = h
            .prepare_sms(request(&owner.id, "same-key", before_line.line))
            .unwrap();
        assert_eq!(replay.id, op.id);
        assert_eq!(replay.state, SmsState::Unknown);
    }
    assert!(logical(&destination.join("store.sqlite3")) == before_replay);
    let next = observe(&restored, [9; 32]);
    assert!(next.line.0 > before_line.line.0);
    let before = logical(&destination.join("store.sqlite3"));
    assert_eq!(
        h.prepare_sms(request(&owner.id, "same-key", next.line)),
        Err(StorageError::Conflict)
    );
    assert!(h.begin_submit(&op.id, &owner.id).is_err());
    let saved = h.operation(&op.id, &owner.id).unwrap();
    assert_eq!(saved.id, op.id);
    assert_eq!(saved.state, SmsState::Unknown);
    assert_eq!(saved.line, before_line.line);
    assert_eq!(h.sms_detail(sms).unwrap().summary.line, before_line.line);
    assert!(logical(&destination.join("store.sqlite3")) == before);
}
