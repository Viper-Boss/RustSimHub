// SPDX-License-Identifier: AGPL-3.0-only
use crate::*;
use rustsimhub_domain::LineEpoch;
use serde_json::{json, Value};

const ID: &str = "11111111-1111-4111-8111-111111111111";
const TARGET: &str = "22222222-2222-4222-8222-222222222222";
fn message() -> Message {
    Message {
        id: ID.into(),
        line: LineEpoch(4),
        sender: "10010".into(),
        timestamp: "2026-10-27T09:30:00+08:00".into(),
        body: "验证码ABC123，请勿分享；全角１２３，数字𝟙𝟚𝟛。".into(),
        duplicate_candidate: false,
        binary: false,
    }
}
fn rule(share: &str) -> Rule {
    serde_json::from_value(json!({"id":ID,"targets":[TARGET],"sender_equals":null,
        "body_contains":null,"line":null,"share":share}))
    .unwrap()
}
fn previous() -> Policy {
    serde_json::from_value(json!({"enabled":false,"targets":[{"id":TARGET,
        "destination":{"kind":"webhook","url":"https://example.invalid/","bearer":null}}],
        "rules":[serde_json::to_value(rule("notice")).unwrap()]}))
    .unwrap()
}
fn conditions() -> RuleConditions {
    RuleConditions {
        body_regex: Some(BodyPattern::new("验证码[A-Z0-9]{6}".into()).unwrap()),
        time_window: Some(TimeWindow {
            start_minute: 540,
            end_minute: 600,
            utc_offset_minutes: 480,
        }),
    }
}

#[test]
fn bounded_pattern_rejects_invalid_unsupported_and_large_programs() {
    for bad in [
        "".to_string(),
        "x".repeat(257),
        "(".into(),
        "(?=secret)".into(),
        "(a)\\1".into(),
        "a{1000000}".into(),
        "(".repeat(32) + "a" + &")".repeat(32),
        "x\0y".into(),
    ] {
        assert!(BodyPattern::new(bad.clone()).is_err());
        let error = serde_json::from_value::<BodyPattern>(json!(bad))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("invalid_rule_pattern"));
        assert!(!error.contains("secret"));
    }
    let pattern = BodyPattern::new("(?i)通知.*[0-9]{6}$".into()).unwrap();
    assert!(pattern.is_match("通知 code 123456"));
    assert!(!pattern.is_match("通知 code 12345"));
    assert!(!pattern.is_match(&"a".repeat(4097)));
    assert_eq!(
        serde_json::to_value(&pattern).unwrap(),
        "(?i)通知.*[0-9]{6}$"
    );
}

#[test]
fn windows_use_sms_time_with_offsets_and_exact_midnight_boundaries() {
    let w = TimeWindow {
        start_minute: 540,
        end_minute: 600,
        utc_offset_minutes: 480,
    };
    assert!(w.matches("2026-10-27T01:00:00Z"));
    assert!(w.matches("2026-10-27T09:59:59.9+08:00"));
    assert!(!w.matches("2026-10-27T10:00:00+08:00"));
    assert!(!w.matches("2026-10-27T08:59:59+08:00"));
    for bad in [
        "",
        "09:30",
        "2026-02-30T09:30:00+08:00",
        "2026-10-27T09:30:00",
    ] {
        assert!(!w.matches(bad));
    }
    let night = TimeWindow {
        start_minute: 1320,
        end_minute: 360,
        utc_offset_minutes: -300,
    };
    assert!(night.matches("2026-10-27T03:00:00Z")); // previous day 22:00
    assert!(night.matches("2026-10-27T10:59:59Z"));
    assert!(!night.matches("2026-10-27T11:00:00Z"));
    for bad in [
        TimeWindow {
            start_minute: 1440,
            end_minute: 2,
            utc_offset_minutes: 0,
        },
        TimeWindow {
            start_minute: 1,
            end_minute: 1,
            utc_offset_minutes: 0,
        },
        TimeWindow {
            start_minute: 1,
            end_minute: 2,
            utc_offset_minutes: 841,
        },
    ] {
        assert!(bad.validate().is_err());
        assert!(!bad.matches(&message().timestamp));
    }
    // Valid remote timestamps cannot panic when a configured offset crosses the year bound.
    assert!(!TimeWindow {
        start_minute: 0,
        end_minute: 1,
        utc_offset_minutes: 840
    }
    .matches("9999-12-31T23:59:59Z"));
}

#[test]
fn extensions_are_anded_with_sender_contains_line_and_duplicate_filters() {
    let mut r = rule("notice");
    r.conditions = Some(conditions());
    r.sender_equals = Some("10010".into());
    r.body_contains = Some("请勿".into());
    r.line = Some(LineEpoch(4));
    assert!(r.matches(&message()));
    let mut m = message();
    m.sender = "10086".into();
    assert!(!r.matches(&m));
    let mut m = message();
    m.line = LineEpoch(5);
    assert!(!r.matches(&m));
    let mut m = message();
    m.body = "验证码ABC123".into();
    assert!(!r.matches(&m));
    let mut m = message();
    m.body = "请勿分享验证码XYZ12".into();
    assert!(!r.matches(&m));
    let mut m = message();
    m.timestamp = "2026-10-27T11:30:00+08:00".into();
    assert!(!r.matches(&m));
    let mut m = message();
    m.binary = true;
    assert!(!r.matches(&m));
    let mut m = message();
    m.duplicate_candidate = true;
    assert!(!r.matches(&m));
    assert!(RuleConditions {
        body_regex: None,
        time_window: None
    }
    .validate()
    .is_err());
}

#[test]
fn notice_masked_and_explicit_full_text_use_the_actual_three_provider_encoders() {
    let destinations = [
        json!({"kind":"webhook","url":"https://example.invalid/","bearer":null}),
        json!({"kind":"bark","server":"https://example.invalid/","key":"synthetic-key"}),
        json!({"kind":"telegram","bot_token":"1:synthetic-only","chat_id":"synthetic-chat"}),
    ];
    for destination in destinations {
        let target: Target =
            serde_json::from_value(json!({"id":TARGET,"destination":destination})).unwrap();
        let m = message();
        for share in ["notice", "masked_text", "full_text"] {
            let request = prepare(&target, &rule(share), &m, ID).unwrap();
            let body: Value =
                serde_json::from_str(crate::model::test_https(&request).body.expose()).unwrap();
            let output = crate::model::test_https(&request).body.expose();
            if share == "full_text" {
                assert!(output.contains("ABC123"));
                assert!(output.contains("10010"));
            } else {
                assert!(!output.contains("ABC123"));
                assert!(!output.contains("10010"));
                assert!(!output.contains("１２３"));
                assert!(!output.contains("𝟙𝟚𝟛"));
            }
            if share == "masked_text" {
                assert!(output.contains("验证码******"));
                if destination["kind"] == "webhook" {
                    assert_eq!(body["redacted"], true);
                    assert!(body.get("sender").is_none());
                    assert!(body.get("timestamp").is_none());
                }
            }
            if share == "notice" {
                assert!(!output.contains("验证码"));
            }
        }
        let mut restricted = rule("full_text");
        restricted.targets.clear();
        assert!(prepare(&target, &restricted, &m, ID).is_err());
        let mut large = message();
        large.body = "A1".repeat(2048);
        let masked = prepare(&target, &rule("masked_text"), &large, ID).unwrap();
        assert!(!crate::model::test_https(&masked)
            .body
            .expose()
            .contains("A1"));
    }
}

#[test]
fn legacy_rules_stay_unchanged_and_stale_clients_cannot_erase_extensions() {
    let legacy = serde_json::to_value(rule("notice")).unwrap();
    assert!(legacy.get("conditions").is_none());
    let edit = |rules: Value, revision: Option<u32>| {
        let mut v = json!({"enabled":false,"targets":[{"id":TARGET,"destination":{"kind":"keep"}}],"rules":rules});
        if let Some(revision) = revision {
            v["rules_revision"] = revision.into();
        }
        serde_json::from_value::<PolicyEdit>(v).unwrap()
    };
    assert!(edit(json!([legacy.clone()]), None)
        .resolve(previous())
        .is_ok());
    let mut advanced = rule("masked_text");
    advanced.conditions = Some(conditions());
    let advanced = serde_json::to_value(advanced).unwrap();
    assert!(edit(json!([advanced.clone()]), None)
        .resolve(previous())
        .is_err());
    assert!(edit(json!([advanced.clone()]), Some(99))
        .resolve(previous())
        .is_err());
    let mut p = previous();
    p.rules = vec![serde_json::from_value(advanced.clone()).unwrap()];
    assert!(edit(json!([legacy.clone()]), None).resolve(p).is_err());
    let p = edit(json!([advanced]), Some(2))
        .resolve(previous())
        .unwrap();
    let p = edit(json!([legacy]), Some(2)).resolve(p).unwrap();
    assert!(!p.rules[0].extended());
}
