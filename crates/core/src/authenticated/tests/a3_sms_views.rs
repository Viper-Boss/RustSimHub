// SPDX-License-Identifier: AGPL-3.0-only
//! Original IPC/Auth, two delegated readers, logical trash and durable epochs.
use super::*;
use rustsimhub_policy::Kind;
fn login(a: &mut Auth, key: &SigningKey) -> String {
    let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
async fn ids(auth: Arc<Mutex<Auth>>, token: &str, limit: usize) -> Vec<String> {
    let mut after = None;
    let mut result = vec![];
    for _ in 0..40 {
        let page = round_trip(
            AuthCommand::Inbox {
                token: token.into(),
                after,
                limit,
            },
            auth.clone(),
        )
        .await;
        let messages = page["messages"].as_array().expect("authorized inbox");
        if messages.is_empty() {
            return result;
        }
        result.extend(
            messages
                .iter()
                .map(|m| m["id"].as_str().unwrap().to_owned()),
        );
        after = Some(serde_json::from_value(page["cursor"].clone()).unwrap());
    }
    panic!("bounded inbox pagination did not terminate")
}
#[tokio::test]
async fn two_real_delegated_ipc_readers_keep_cursor_trash_revocation_and_restart_boundaries() {
    let d = directory();
    let identity = directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let mut a = Auth::new(Identity::open(identity.path()).unwrap(), h.clone()).unwrap();
    let key = SigningKey::from_bytes(&[91; 32]);
    let c = a
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    let owner = a
        .bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let owner_token = login(&mut a, &key);
    let keys = [
        SigningKey::from_bytes(&[92; 32]),
        SigningKey::from_bytes(&[93; 32]),
    ];
    let mut tokens = vec![];
    for key in &keys {
        h.approve_client(
            owner.clone(),
            key.verifying_key().to_bytes(),
            Kind::Client,
            rustsimhub_policy::mask(&[Scope::SmsRead]).unwrap(),
        )
        .unwrap();
        tokens.push(login(&mut a, key));
    }
    let line = h
        .observe_line(rustsimhub_storage::LineObservation {
            backend: "synthetic-a3-views".into(),
            fingerprint: Some([41; 32]),
            status: rustsimhub_storage::LineStatus::Ready,
        })
        .unwrap()
        .line;
    let mut expected = vec![];
    for n in 0..17 {
        expected.push(
            h.receive_sms(rustsimhub_storage::IncomingSms {
                line,
                source: format!("/org/freedesktop/ModemManager1/SMS/{n}#1"),
                sender: "synthetic".into(),
                timestamp: "synthetic-time".into(),
                text: if n == 1 {
                    String::new()
                } else if n == 0 {
                    "中文👩\u{200d}💻 e\u{301}".into()
                } else {
                    "合成长短信🙂".repeat(100)
                },
                data: if n == 1 { vec![0, 255, 128] } else { vec![] },
            })
            .unwrap()
            .id,
        );
    }
    let auth = Arc::new(Mutex::new(a));
    assert_eq!(ids(auth.clone(), &tokens[0], 3).await, expected);
    assert_eq!(ids(auth.clone(), &tokens[1], 7).await, expected);
    let text = round_trip(
        AuthCommand::SmsDetail {
            token: tokens[1].clone(),
            sms_id: expected[0].clone(),
        },
        auth.clone(),
    )
    .await;
    assert_eq!(text["text"], "中文👩\u{200d}💻 e\u{301}");
    for command in [
        AuthCommand::DeleteSms {
            token: tokens[0].clone(),
            sms_id: expected[0].clone(),
        },
        AuthCommand::DeletedInbox {
            token: tokens[0].clone(),
            after: None,
            limit: 8,
        },
    ] {
        assert!(round_trip(command, auth.clone()).await["error"].is_string());
    }
    assert_eq!(
        round_trip(
            AuthCommand::DeleteSms {
                token: owner_token.clone(),
                sms_id: expected[0].clone()
            },
            auth.clone()
        )
        .await["deleted"],
        true
    );
    assert!(round_trip(
        AuthCommand::SmsDetail {
            token: tokens[1].clone(),
            sms_id: expected[0].clone()
        },
        auth.clone()
    )
    .await["error"]
        .is_string());
    assert_eq!(ids(auth.clone(), &tokens[1], 4).await, expected[1..]);
    assert_eq!(
        round_trip(
            AuthCommand::RestoreSms {
                token: owner_token,
                sms_id: expected[0].clone()
            },
            auth.clone()
        )
        .await["deleted"],
        false
    );
    let next = h
        .observe_line(rustsimhub_storage::LineObservation {
            backend: "synthetic-a3-views".into(),
            fingerprint: Some([42; 32]),
            status: rustsimhub_storage::LineStatus::Ready,
        })
        .unwrap()
        .line;
    assert_ne!(line, next);
    let new = h
        .receive_sms(rustsimhub_storage::IncomingSms {
            line: next,
            source: "/org/freedesktop/ModemManager1/SMS/0#1".into(),
            sender: "synthetic".into(),
            timestamp: "synthetic-time".into(),
            text: "new SIM incarnation".into(),
            data: vec![],
        })
        .unwrap();
    assert_ne!(new.id, expected[0]);
    expected.push(new.id);
    h.revoke_client(owner, keys[0].verifying_key().to_bytes())
        .unwrap();
    assert!(round_trip(
        AuthCommand::Inbox {
            token: tokens[0].clone(),
            after: None,
            limit: 8
        },
        auth.clone()
    )
    .await["error"]
        .is_string());
    assert_eq!(ids(auth.clone(), &tokens[1], 5).await, expected);
    drop(auth);
    drop(store);
    let reopened = Store::open(d.path()).unwrap();
    let mut a = Auth::new(Identity::open(identity.path()).unwrap(), reopened.handle()).unwrap();
    let second = login(&mut a, &keys[1]);
    assert!(a
        .login_challenge(keys[0].verifying_key().to_bytes())
        .is_err());
    let auth = Arc::new(Mutex::new(a));
    assert!(round_trip(
        AuthCommand::Inbox {
            token: tokens[1].clone(),
            after: None,
            limit: 8
        },
        auth.clone()
    )
    .await["error"]
        .is_string());
    assert_eq!(ids(auth, &second, 7).await, expected);
}
