// SPDX-License-Identifier: AGPL-3.0-only
//! Replay an explicit bounded corpus through real Store entry points.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    format: u32,
    seed: String,
    max_cases: usize,
    sms: Vec<SmsCase>,
    calls: Vec<CallCase>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SmsCase {
    id: String,
    text: String,
    repeat: usize,
    data_len: usize,
    source_len: Option<usize>,
    sender_len: Option<usize>,
    timestamp_len: Option<usize>,
    accepted: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallCase {
    id: String,
    direction: i32,
    state: i32,
    reason: i32,
    accepted: bool,
}
fn context(h: &Handle, key: u8) -> SendContext {
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-boundary".into(),
            fingerprint: Some([key; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    SendContext {
        line,
        backend: "synthetic-boundary".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [key; 32],
    }
}
fn sms(c: &SmsCase, line: LineEpoch) -> IncomingSms {
    assert!(c.text.len() * c.repeat <= 16_384 && c.data_len <= 8192);
    for n in [c.source_len, c.sender_len, c.timestamp_len]
        .into_iter()
        .flatten()
    {
        assert!(n <= 512);
    }
    IncomingSms {
        line,
        source: c
            .source_len
            .map_or_else(|| format!("synthetic/{}", c.id), |n| "s".repeat(n)),
        sender: c
            .sender_len
            .map_or_else(|| "synthetic".into(), |n| "n".repeat(n)),
        timestamp: c
            .timestamp_len
            .map_or_else(|| "synthetic-time".into(), |n| "t".repeat(n)),
        text: c.text.repeat(c.repeat),
        data: vec![0xa5; c.data_len],
    }
}
fn call(c: &CallCase) -> CallObservation {
    CallObservation {
        source: format!("synthetic/{}", c.id),
        number: "+12025550123".into(),
        direction: c.direction,
        state: c.state,
        reason: c.reason,
    }
}
fn controls(h: &Handle) -> i64 {
    h.call(|db| {
        Ok(db.connection.query_row(
            "SELECT (SELECT count(*) FROM sms_operations)+(SELECT count(*) FROM call_operations)",
            [],
            |r| r.get(0),
        )?)
    })
    .unwrap()
}

#[test]
fn deterministic_corpus_rejects_without_advancing_versions_and_replays_without_new_rows() {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../../../tools/input-corpus/sms-calls/v1.json"
    ))
    .unwrap();
    assert_eq!(corpus.format, 1);
    assert_eq!(corpus.seed, "rsh-d15-v1");
    assert_eq!(corpus.max_cases, 64);
    assert!(corpus.sms.len() + corpus.calls.len() <= corpus.max_cases);
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 88);
    let mut sms_ids = vec![];
    let mut call_ids = vec![];
    for c in &corpus.sms {
        let before = h.indicator_version().unwrap();
        let result = h.receive_sms(sms(c, b.line));
        if c.accepted {
            sms_ids.push((c.id.clone(), result.unwrap().id));
        } else {
            assert!(matches!(result, Err(Error::InvalidInput)), "{}", c.id);
            assert_eq!(h.indicator_version().unwrap(), before);
        }
    }
    for c in &corpus.calls {
        let before = h.indicator_version().unwrap();
        let result = h.observe_call(b.clone(), call(c), || true);
        if c.accepted {
            call_ids.push((c.id.clone(), result.unwrap().id));
        } else {
            assert!(matches!(result, Err(Error::InvalidInput)), "{}", c.id);
            assert_eq!(h.indicator_version().unwrap(), before);
        }
    }
    let version = h.indicator_version().unwrap();
    for c in corpus.sms.iter().rev().filter(|c| c.accepted) {
        assert_eq!(
            h.receive_sms(sms(c, b.line)).unwrap().id,
            sms_ids.iter().find(|r| r.0 == c.id).unwrap().1
        );
    }
    for c in corpus.calls.iter().rev().filter(|c| c.accepted) {
        assert_eq!(
            h.observe_call(b.clone(), call(c), || true).unwrap().id,
            call_ids.iter().find(|r| r.0 == c.id).unwrap().1
        );
    }
    assert_eq!(h.indicator_version().unwrap(), version);
    assert_eq!(controls(&h), 0);
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 88);
    for c in corpus.sms.iter().filter(|c| c.accepted) {
        assert_eq!(
            h.receive_sms(sms(c, b.line)).unwrap().id,
            sms_ids.iter().find(|r| r.0 == c.id).unwrap().1
        );
    }
    for c in corpus.calls.iter().filter(|c| c.accepted) {
        assert_eq!(
            h.observe_call(b.clone(), call(c), || true).unwrap().id,
            call_ids.iter().find(|r| r.0 == c.id).unwrap().1
        );
    }
    assert_eq!(h.inbox(None, 32).unwrap().messages.len(), sms_ids.len());
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), call_ids.len());
    assert_eq!(controls(&h), 0);
}

#[test]
fn current_line_projection_removes_old_sim_and_deleted_or_unavailable_records() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([89; 32]).unwrap();
    let first = context(&h, 89);
    let input = || IncomingSms {
        line: first.line,
        source: "synthetic/current-sms".into(),
        sender: "synthetic".into(),
        timestamp: "synthetic-time".into(),
        text: "synthetic indicator".into(),
        data: vec![],
    };
    let msg = h.receive_sms(input()).unwrap();
    let c = h
        .observe_call(
            first.clone(),
            CallObservation {
                source: "synthetic/current-call".into(),
                number: "+12025550123".into(),
                direction: 1,
                state: 3,
                reason: 0,
            },
            || true,
        )
        .unwrap();
    let initial = h.indicator_snapshot().unwrap();
    assert_eq!(initial.sms.as_slice(), std::slice::from_ref(&msg.id));
    assert_eq!(initial.calls, [(c.id.clone(), 1, 3)]);
    h.set_sms_deleted(owner.clone(), msg.id.clone(), true)
        .unwrap();
    assert!(h.indicator_snapshot().unwrap().sms.is_empty());
    h.set_sms_deleted(owner, msg.id.clone(), false).unwrap();
    assert_eq!(h.indicator_snapshot().unwrap().sms, [msg.id]);
    h.calls_unavailable(vec![c.id.clone()]).unwrap();
    assert!(h.indicator_snapshot().unwrap().calls.is_empty());
    let second = context(&h, 90);
    assert_ne!(second.line, first.line);
    let current = h.indicator_snapshot().unwrap();
    assert!(current.sms.is_empty() && current.calls.is_empty());
    assert!(matches!(h.receive_sms(input()), Err(Error::StaleLine)));
    assert_eq!(h.call_detail(c.id).unwrap().modem_state, 3);
    assert_eq!(controls(&h), 0);
}
