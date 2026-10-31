use std::cell::Cell;

use rustsimhub_usage_stats::protocol::{
    CheckInIntent, Clock, ClockError, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent,
    ProtocolError, StatsPublicKey, Window, DISCLOSURE_SUMMARY, DISCLOSURE_VERSION,
    MAX_POLICY_BYTES, MAX_PUBLIC_PAYLOAD_BYTES, MAX_SUBSCRIPTION_BLOCKS, PROTOCOL_VERSION,
    RENEW_INTERVAL_SECS,
};
use serde_json::{json, Value};

#[test]
fn named_check_in_result_reports_only_disabled_or_local_preparation() {
    use rustsimhub_usage_stats::protocol::CheckInPreparation;
    let clock = TestClock::at(1_728_000_123);
    let key = StatsPublicKey::from_bytes([7; 32]);
    assert_eq!(
        EnabledPolicy::default().prepare_check_in(Window::Last24Hours, key, &clock),
        Ok(CheckInPreparation::Disabled)
    );
    assert_eq!(clock.calls.get(), 0);
    let prepared = enabled()
        .prepare_check_in(Window::Last24Hours, key, &clock)
        .unwrap();
    assert!(
        matches!(prepared, CheckInPreparation::Prepared { intent } if intent.public_key() == key)
    );
    let dto = serde_json::to_value(prepared).unwrap();
    assert_eq!(dto["status"], "prepared");
    assert!(dto.get("confirmed").is_none());
    assert!(dto.get("seed").is_none());
}

struct TestClock {
    result: Result<u64, ClockError>,
    calls: Cell<usize>,
}

impl TestClock {
    fn at(now: u64) -> Self {
        Self {
            result: Ok(now),
            calls: Cell::new(0),
        }
    }
}

impl Clock for TestClock {
    fn now_secs(&self) -> Result<u64, ClockError> {
        self.calls.set(self.calls.get() + 1);
        self.result
    }
}

fn enabled() -> EnabledPolicy {
    EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
        .unwrap()
}

fn intent(window: Window) -> CheckInIntent {
    enabled()
        .check_in_intent(
            window,
            StatsPublicKey::from_bytes([0xab; 32]),
            &TestClock::at(1_728_000_123),
        )
        .unwrap()
        .unwrap()
}

fn changed(field: &str, value: Value) -> Vec<u8> {
    let mut body = serde_json::to_value(intent(Window::Last24Hours)).unwrap();
    body[field] = value;
    serde_json::to_vec(&body).unwrap()
}

#[test]
fn product_windows_have_fixed_separate_topics_and_bounded_chain_parameters() {
    let expected = [
        ("24h", "rustsimhub-usage-v1-24h", 86_400, 8_640),
        ("30d", "rustsimhub-usage-v1-30d", 2_592_000, 133_920),
        ("90d", "rustsimhub-usage-v1-90d", 7_776_000, 393_120),
    ];
    for (index, window) in Window::ALL.into_iter().enumerate() {
        let (label, topic, seconds, blocks) = expected[index];
        assert_eq!(window.index(), index);
        assert_eq!(window.topic(), topic);
        assert_eq!(window.duration_secs(), seconds);
        assert_eq!(window.subscription_blocks(), blocks);
        assert!(blocks <= MAX_SUBSCRIPTION_BLOCKS);
        let encoded = serde_json::to_string(&window).unwrap();
        assert_eq!(encoded, format!("\"{label}\""));
        assert_eq!(serde_json::from_str::<Window>(&encoded).unwrap(), window);
    }
    for invalid in ["\"7d\"", "\"24H\"", "0", "null", "[]"] {
        assert!(serde_json::from_str::<Window>(invalid).is_err());
    }
}

#[test]
fn public_key_wire_format_is_canonical_and_never_serializes_a_seed() {
    let key = StatsPublicKey::from_bytes([0xab; 32]);
    let encoded = serde_json::to_string(&key).unwrap();
    assert_eq!(encoded, format!("\"{}\"", "ab".repeat(32)));
    assert_eq!(
        serde_json::from_str::<StatsPublicKey>(&encoded).unwrap(),
        key
    );
    assert_eq!(key.as_bytes(), &[0xab; 32]);
    assert_eq!(
        format!("{key:?}"),
        format!("StatsPublicKey(\"{}\")", "ab".repeat(32))
    );

    for invalid in [
        json!("AB".repeat(32)),
        json!("ab".repeat(31)),
        json!("ab".repeat(33)),
        json!("ag".repeat(32)),
        json!(vec![0; 32]),
        json!(null),
        json!(42),
    ] {
        assert!(serde_json::from_value::<StatsPublicKey>(invalid).is_err());
    }
}

#[test]
fn public_intents_round_trip_with_only_the_disclosed_fields() {
    for window in Window::ALL {
        let expected = intent(window);
        let encoded = expected.to_json().unwrap();
        assert!(encoded.len() <= MAX_PUBLIC_PAYLOAD_BYTES);
        assert_eq!(CheckInIntent::parse(&encoded).unwrap(), expected);
        assert_eq!(
            serde_json::from_slice::<CheckInIntent>(&encoded).unwrap(),
            expected
        );
        assert_eq!(expected.version(), PROTOCOL_VERSION);
        assert_eq!(expected.window(), window);
        assert_eq!(expected.topic(), window.topic());
        assert_eq!(
            expected.public_key(),
            StatsPublicKey::from_bytes([0xab; 32])
        );
        assert_eq!(expected.subscription_blocks(), window.subscription_blocks());
        assert_eq!(expected.renewal_period(), 20_000);

        let actual: Value = serde_json::from_slice(&encoded).unwrap();
        let fields = actual.as_object().unwrap();
        assert_eq!(fields.len(), 6);
        for field in [
            "version",
            "public_key",
            "topic",
            "window",
            "subscription_blocks",
            "renewal_period",
        ] {
            assert!(fields.contains_key(field));
        }
        for forbidden in [
            "seed", "phone", "sms", "imei", "imsi", "iccid", "ssid", "owner", "identity", "bytes",
        ] {
            assert!(!fields.contains_key(forbidden));
        }
    }
}

#[test]
fn parser_rejects_unsupported_versions_and_cross_product_or_window_topics() {
    for version in [0, 2, u16::MAX] {
        assert_eq!(
            CheckInIntent::parse(&changed("version", json!(version))),
            Err(ProtocolError::UnsupportedVersion)
        );
    }
    for topic in [
        "nknguard.usage.24h",
        "nassimhub-usage-v1-24h",
        "rustsimhub-usage-v1-30d",
        "",
        "https://rpc.invalid/",
    ] {
        assert_eq!(
            CheckInIntent::parse(&changed("topic", json!(topic))),
            Err(ProtocolError::WrongTopic)
        );
    }
}

#[test]
fn parser_rejects_invalid_windows_durations_periods_and_numeric_overflow() {
    for window in [json!("7d"), json!("24H"), json!(null), json!(1)] {
        assert_eq!(
            CheckInIntent::parse(&changed("window", window)),
            Err(ProtocolError::Malformed)
        );
    }
    for blocks in [json!(0), json!(1), json!(133_920), json!(400_001)] {
        assert_eq!(
            CheckInIntent::parse(&changed("subscription_blocks", blocks)),
            Err(ProtocolError::InvalidPeriod)
        );
    }
    for (field, value) in [
        ("version", json!(65_536_u64)),
        ("subscription_blocks", json!(u64::MAX)),
        ("subscription_blocks", json!(-1)),
        ("renewal_period", json!(-1)),
        ("renewal_period", json!(0.5)),
    ] {
        assert_eq!(
            CheckInIntent::parse(&changed(field, value)),
            Err(ProtocolError::Malformed)
        );
    }
    for period in [u64::MAX, u64::MAX / RENEW_INTERVAL_SECS] {
        assert_eq!(
            CheckInIntent::parse(&changed("renewal_period", json!(period))),
            Err(ProtocolError::InvalidPeriod)
        );
    }
    let largest_valid_period = u64::MAX / RENEW_INTERVAL_SECS - 1;
    assert_eq!(
        CheckInIntent::parse(&changed("renewal_period", json!(largest_valid_period)))
            .unwrap()
            .renewal_period(),
        largest_valid_period
    );
}

#[test]
fn parser_rejects_unknown_missing_duplicate_fields_and_trailing_input() {
    let valid = intent(Window::Last24Hours).to_json().unwrap();
    let valid_text = String::from_utf8(valid).unwrap();
    for field in [
        "version",
        "public_key",
        "topic",
        "window",
        "subscription_blocks",
        "renewal_period",
    ] {
        let mut body: Value = serde_json::from_str(&valid_text).unwrap();
        let duplicate = serde_json::to_string(&body[field]).unwrap();
        let text = format!(
            "{},\"{field}\":{duplicate}}}",
            valid_text.trim_end_matches('}')
        );
        assert_eq!(
            CheckInIntent::parse(text.as_bytes()),
            Err(ProtocolError::Malformed)
        );
        body.as_object_mut().unwrap().remove(field);
        assert_eq!(
            CheckInIntent::parse(&serde_json::to_vec(&body).unwrap()),
            Err(ProtocolError::Malformed)
        );
    }
    assert_eq!(
        CheckInIntent::parse(&changed("extra", json!(true))),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        CheckInIntent::parse(format!("{valid_text} {{}}").as_bytes()),
        Err(ProtocolError::Malformed)
    );
    for text in ["", "{", "[]", "null", "{\"version\":"] {
        assert_eq!(
            CheckInIntent::parse(text.as_bytes()),
            Err(ProtocolError::Malformed)
        );
    }
}

#[test]
fn byte_limits_are_enforced_before_parsing_and_topic_is_also_bounded() {
    assert_eq!(
        CheckInIntent::parse(&vec![b' '; MAX_PUBLIC_PAYLOAD_BYTES + 1]),
        Err(ProtocolError::InputTooLarge)
    );
    assert_eq!(
        EnabledPolicy::parse(&vec![b' '; MAX_POLICY_BYTES + 1]),
        Err(ProtocolError::InputTooLarge)
    );
    assert_eq!(
        CheckInIntent::parse(&changed("topic", json!("x".repeat(65)))),
        Err(ProtocolError::Malformed)
    );
    let mut exactly_bounded = intent(Window::Last24Hours).to_json().unwrap();
    exactly_bounded.resize(MAX_PUBLIC_PAYLOAD_BYTES, b' ');
    assert!(CheckInIntent::parse(&exactly_bounded).is_ok());
    exactly_bounded.push(b' ');
    assert_eq!(
        CheckInIntent::parse(&exactly_bounded),
        Err(ProtocolError::InputTooLarge)
    );
}

#[test]
fn missing_policy_is_off_and_enabled_requires_explicit_current_disclosure() {
    assert!(!EnabledPolicy::default().is_enabled());
    assert!(!EnabledPolicy::parse(b"{}").unwrap().is_enabled());
    assert!(!EnabledPolicy::parse(br#"{"enabled":false}"#)
        .unwrap()
        .is_enabled());
    assert!(!serde_json::from_str::<EnabledPolicy>("{}")
        .unwrap()
        .is_enabled());
    assert_eq!(
        EnabledPolicy::from_owner_consent(
            OwnerConsent::Granted,
            DisclosureAcknowledgement::NotAcknowledged
        ),
        Err(ProtocolError::DisclosureRequired)
    );
    assert!(!EnabledPolicy::from_owner_consent(
        OwnerConsent::Denied,
        DisclosureAcknowledgement::Current
    )
    .unwrap()
    .is_enabled());
    assert!(enabled().is_enabled());
    assert!(DISCLOSURE_SUMMARY.contains("links this installation across windows and time"));
    assert!(DISCLOSURE_SUMMARY.contains("not people"));

    let persisted = serde_json::to_vec(&enabled()).unwrap();
    assert!(EnabledPolicy::parse(&persisted).unwrap().is_enabled());
    assert_eq!(
        serde_json::from_slice::<Value>(&persisted).unwrap(),
        json!({"enabled":true,"owner_consent":true,"disclosure_version":DISCLOSURE_VERSION})
    );
    for rejected in [
        br#"{"enabled":true}"#.as_slice(),
        br#"{"enabled":true,"owner_consent":false,"disclosure_version":1}"#.as_slice(),
        br#"{"enabled":true,"owner_consent":true}"#.as_slice(),
        br#"{"enabled":true,"owner_consent":true,"disclosure_version":0}"#.as_slice(),
        br#"{"enabled":true,"owner_consent":true,"disclosure_version":2}"#.as_slice(),
        br#"{"enabled":false,"owner_consent":true,"disclosure_version":1}"#.as_slice(),
        br#"{"enabled":false,"extra":false}"#.as_slice(),
        br#"{"enabled":false,"enabled":true}"#.as_slice(),
    ] {
        assert!(EnabledPolicy::parse(rejected).is_err());
        assert!(serde_json::from_slice::<EnabledPolicy>(rejected).is_err());
    }
}

#[test]
fn off_creates_no_intent_and_does_not_even_read_the_clock() {
    let clock = TestClock {
        result: Err(ClockError::Unavailable),
        calls: Cell::new(0),
    };
    for policy in [
        EnabledPolicy::default(),
        EnabledPolicy::disabled(),
        EnabledPolicy::parse(b"{}").unwrap(),
    ] {
        for window in Window::ALL {
            assert_eq!(
                policy.check_in_intent(window, StatsPublicKey::from_bytes([1; 32]), &clock),
                Ok(None)
            );
        }
    }
    assert_eq!(clock.calls.get(), 0);
}

#[test]
fn consented_preparation_uses_injected_coarse_time_without_submit_semantics() {
    let key = StatsPublicKey::from_bytes([1; 32]);
    let earlier = TestClock::at(2 * RENEW_INTERVAL_SECS);
    let later = TestClock::at(3 * RENEW_INTERVAL_SECS - 1);
    let next = TestClock::at(3 * RENEW_INTERVAL_SECS);
    let first = enabled()
        .check_in_intent(Window::Last30Days, key, &earlier)
        .unwrap()
        .unwrap();
    let same = enabled()
        .check_in_intent(Window::Last30Days, key, &later)
        .unwrap()
        .unwrap();
    let different = enabled()
        .check_in_intent(Window::Last30Days, key, &next)
        .unwrap()
        .unwrap();
    assert_eq!(first, same);
    assert_eq!(first.renewal_period(), 2);
    assert_eq!(different.renewal_period(), 3);
    assert_eq!(earlier.calls.get(), 1);
    assert_eq!(later.calls.get(), 1);
    assert_eq!(next.calls.get(), 1);
}

#[test]
fn clock_failure_and_time_overflow_fail_closed() {
    for failure in [ClockError::BeforeEpoch, ClockError::Unavailable] {
        let clock = TestClock {
            result: Err(failure),
            calls: Cell::new(0),
        };
        assert_eq!(
            enabled().check_in_intent(
                Window::Last90Days,
                StatsPublicKey::from_bytes([1; 32]),
                &clock
            ),
            Err(ProtocolError::Clock(failure))
        );
        assert_eq!(clock.calls.get(), 1);
    }
    assert_eq!(
        enabled().check_in_intent(
            Window::Last24Hours,
            StatsPublicKey::from_bytes([1; 32]),
            &TestClock::at(u64::MAX)
        ),
        Err(ProtocolError::InvalidPeriod)
    );
}

#[test]
fn parser_errors_do_not_echo_restricted_input() {
    const SYNTHETIC_SECRET: &str = "SYNTHETIC_PRIVATE_SEED_DO_NOT_ECHO";
    for malicious in [
        changed("window", json!(SYNTHETIC_SECRET)),
        changed("public_key", json!(SYNTHETIC_SECRET)),
        changed(SYNTHETIC_SECRET, json!(SYNTHETIC_SECRET)),
        changed("topic", json!(SYNTHETIC_SECRET)),
    ] {
        let error = CheckInIntent::parse(&malicious).unwrap_err();
        assert!(!format!("{error}: {error:?}").contains(SYNTHETIC_SECRET));
        let serde_error = serde_json::from_slice::<CheckInIntent>(&malicious).unwrap_err();
        assert!(!serde_error.to_string().contains(SYNTHETIC_SECRET));
    }
    let policy = format!(r#"{{"{SYNTHETIC_SECRET}":true}}"#);
    let error = serde_json::from_str::<EnabledPolicy>(&policy).unwrap_err();
    assert!(!error.to_string().contains(SYNTHETIC_SECRET));
    let error = serde_json::from_value::<Window>(json!(SYNTHETIC_SECRET)).unwrap_err();
    assert!(!error.to_string().contains(SYNTHETIC_SECRET));
}
