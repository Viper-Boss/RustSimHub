// SPDX-License-Identifier: AGPL-3.0-only
//! Actual finite tmpfs exhaustion; never fill the host filesystem.
use super::*;
use std::io::Write;

fn mount() -> std::path::PathBuf {
    assert_eq!(std::env::var("RSH_A3_ENOSPC").unwrap(), "1");
    let path = std::path::PathBuf::from(std::env::var_os("RSH_A3_SMS_FS").unwrap());
    assert!(path.is_absolute() && path.to_str().unwrap().starts_with("/var/tmp/rsh-a3-sms-"));
    assert_eq!(path.canonicalize().unwrap(), path);
    assert_ne!(
        fs::read_link("/proc/self/ns/mnt").unwrap(),
        std::path::PathBuf::from(std::env::var_os("RSH_A3_PARENT_MNT").unwrap())
    );
    let mounts = fs::read_to_string("/proc/self/mountinfo").unwrap();
    assert!(
        mounts.lines().any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.get(4) == Some(&path.to_str().unwrap())
                && line.contains(" - tmpfs ")
                && line.contains("size=16384k")
        }),
        "requires exact disposable 16MiB tmpfs"
    );
    path
}
fn fill(directory: &std::path::Path) -> std::fs::File {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("owned-filler"))
        .unwrap();
    let block = [0x5a; 65536];
    for _ in 0..512 {
        match file.write_all(&block) {
            Ok(()) => {}
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(28));
                return file;
            }
        }
    }
    panic!("tmpfs did not reach real ENOSPC within 32MiB bound");
}

#[test]
#[ignore = "explicit private mount namespace with disposable 16MiB tmpfs only"]
fn actual_enospc_at_five_send_boundaries_preserves_original_id_and_permits() {
    let root = mount();
    for phase in 0..5 {
        let directory = tempfile::tempdir_in(&root).unwrap();
        let store = Store::open(directory.path()).unwrap();
        let h = store.handle();
        let p = h.bootstrap_owner([61; 32]).unwrap();
        let b = binding(&h);
        let op = if phase > 0 {
            Some(prepare(&h, &p, &b, "a3-full"))
        } else {
            None
        };
        if phase > 1 {
            h.begin_authorized(
                op.as_ref().unwrap().id.clone(),
                p.clone(),
                b.clone(),
                || true,
            )
            .unwrap();
        }
        if phase > 2 {
            bind(&h, &p, &b, &op.as_ref().unwrap().id);
        }
        if phase > 3 {
            h.permit_send(
                op.as_ref().unwrap().id.clone(),
                p.clone(),
                b.clone(),
                || true,
            )
            .unwrap();
        }
        let head = h.cursor().unwrap();
        h.call(|db| {
            db.connection
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            Ok(())
        })
        .unwrap();
        let filler = fill(directory.path());
        let result = match phase {
            0 => h
                .prepare_authorized(draft(&p, &b, "a3-full"), p.clone(), b.clone(), || true)
                .map(|_| ()),
            1 => h
                .begin_authorized(
                    op.as_ref().unwrap().id.clone(),
                    p.clone(),
                    b.clone(),
                    || true,
                )
                .map(|_| ()),
            2 => h.bind_created(
                op.as_ref().unwrap().id.clone(),
                p.clone(),
                b.clone(),
                "/org/freedesktop/ModemManager1/SMS/6100".into(),
                7,
                || true,
            ),
            3 => h.permit_send(
                op.as_ref().unwrap().id.clone(),
                p.clone(),
                b.clone(),
                || true,
            ),
            4 => h
                .resolve_sent(
                    op.as_ref().unwrap().id.clone(),
                    p.clone(),
                    b.clone(),
                    || true,
                )
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert_eq!(
            result,
            Err(Error::Full),
            "phase {phase}: real SQLite disk-full required"
        );
        assert_eq!(h.cursor().unwrap(), head);
        // This fixture invokes no modem. A permit is never returned on failed
        // persistence; phase4 already consumed its one durable permit.
        filler.set_len(0).unwrap();
        drop(filler);
        drop(store);
        let reopened = Store::open(directory.path()).unwrap();
        let h = reopened.handle();
        let b = binding(&h);
        let same = h
            .prepare_authorized(draft(&p, &b, "a3-full"), p.clone(), b.clone(), || true)
            .unwrap();
        let expected = if phase <= 1 {
            SmsState::Prepared
        } else {
            SmsState::Unknown
        };
        assert_eq!(same.state, expected);
        if let Some(op) = op {
            assert_eq!(same.id, op.id);
        }
        if phase >= 2 {
            assert!(h
                .begin_authorized(same.id.clone(), p.clone(), b.clone(), || true)
                .is_err());
            assert!(h
                .permit_send(same.id.clone(), p.clone(), b.clone(), || true)
                .is_err());
        }
        if phase == 4 {
            assert_eq!(
                h.send_attempt(same.id.clone(), p.clone()).unwrap().phase,
                "sending"
            );
            assert_eq!(
                h.resolve_sent(same.id.clone(), p.clone(), b, || true)
                    .unwrap()
                    .state,
                SmsState::Submitted
            );
        }
        println!("A3 actual ENOSPC phase={phase}, original ID/state/permit fence verified");
    }
}
