// SPDX-License-Identifier: AGPL-3.0-only
//! SmsForwarder-style chat channels. Every token, sender and body is synthetic.
use super::*;

const TARGET: &str = "11111111-1111-4111-8111-111111111111";
const RULE: &str = "22222222-2222-4222-8222-222222222222";
const DELIVERY: &str = "33333333-3333-4333-8333-333333333333";
const MESSAGE: &str = "44444444-4444-4444-8444-444444444444";

fn secret(value: &str) -> Secret {
    Secret::new(value.into())
}
fn target(destination: Destination) -> Target {
    Target {
        id: TARGET.into(),
        destination,
    }
}
fn rule(share: Share) -> Rule {
    Rule {
        id: RULE.into(),
        targets: vec![TARGET.into()],
        sender_equals: None,
        body_contains: None,
        line: None,
        share,
        conditions: None,
    }
}
fn message() -> Message {
    Message {
        id: MESSAGE.into(),
        line: LineEpoch(1),
        sender: "synthetic-sender".into(),
        timestamp: "synthetic-time".into(),
        body: "synthetic body 123456".into(),
        duplicate_candidate: false,
        binary: false,
    }
}
fn prepared(
    destination: Destination,
    share: Share,
) -> (String, serde_json::Value, Acknowledgement) {
    let delivery = prepare(&target(destination), &rule(share), &message(), DELIVERY).unwrap();
    let request = test_https(&delivery);
    assert!(request.bearer.is_none());
    (
        request.url.expose().to_string(),
        serde_json::from_str(request.body.expose()).unwrap(),
        request.acknowledgement,
    )
}
fn all() -> Vec<Destination> {
    vec![
        Destination::Dingtalk {
            access_token: secret("dingtoken"),
            secret: None,
        },
        Destination::Wecom {
            key: secret("wecom-key"),
        },
        Destination::Feishu {
            hook: secret("feishu-hook"),
            secret: None,
        },
        Destination::Serverchan {
            send_key: secret("SCTsynthetic"),
        },
        Destination::Pushplus {
            token: secret("pushplustoken"),
        },
        Destination::Gotify {
            server: "https://gotify.example.invalid/".into(),
            token: secret("A.gotify_token"),
        },
    ]
}
fn acknowledged(acknowledgement: Acknowledgement, status: u16, response: &str) -> bool {
    PreparedRequest {
        url: secret("https://example.invalid/"),
        bearer: None,
        body: secret("{}"),
        acknowledgement,
    }
    .acknowledged(status, response.as_bytes())
}

#[test]
fn hmac_matches_rfc4231_and_long_keys_are_hashed() {
    let hex = |b: [u8; 32]| b.iter().map(|v| format!("{v:02x}")).collect::<String>();
    assert_eq!(
        hex(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    // RFC 4231 test case 6: 131-byte key, longer than the block size.
    assert_eq!(
        hex(hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        )),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn each_channel_uses_its_fixed_provider_endpoint_and_body_shape() {
    let (url, body, ack) = prepared(all().remove(0), Share::FullText);
    assert_eq!(
        url,
        "https://oapi.dingtalk.com/robot/send?access_token=dingtoken"
    );
    assert_eq!(body["msgtype"], "text");
    let text = body["text"]["content"].as_str().unwrap();
    assert!(text.contains("synthetic-sender") && text.contains("synthetic body 123456"));
    assert!(matches!(ack, Acknowledgement::ErrcodeZero));

    let (url, body, ack) = prepared(all().remove(1), Share::FullText);
    assert_eq!(
        url,
        "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=wecom-key"
    );
    assert!(body["text"]["content"]
        .as_str()
        .unwrap()
        .contains("synthetic body 123456"));
    assert!(matches!(ack, Acknowledgement::ErrcodeZero));

    let (url, body, ack) = prepared(all().remove(2), Share::FullText);
    assert_eq!(
        url,
        "https://open.feishu.cn/open-apis/bot/v2/hook/feishu-hook"
    );
    assert_eq!(body["msg_type"], "text");
    assert!(body["content"]["text"]
        .as_str()
        .unwrap()
        .contains("synthetic body 123456"));
    assert!(body.get("sign").is_none() && body.get("timestamp").is_none());
    assert!(matches!(ack, Acknowledgement::CodeZero));

    let (url, body, ack) = prepared(all().remove(3), Share::FullText);
    assert_eq!(url, "https://sctapi.ftqq.com/SCTsynthetic.send");
    assert!(body["desp"]
        .as_str()
        .unwrap()
        .contains("synthetic body 123456"));
    assert!(matches!(ack, Acknowledgement::CodeZero));

    let (url, body, ack) = prepared(all().remove(4), Share::FullText);
    assert_eq!(url, "https://www.pushplus.plus/send");
    assert_eq!(body["token"], "pushplustoken");
    assert!(body["content"]
        .as_str()
        .unwrap()
        .contains("synthetic body 123456"));
    assert!(matches!(ack, Acknowledgement::Bark));

    let (url, body, ack) = prepared(all().remove(5), Share::FullText);
    assert_eq!(
        url,
        "https://gotify.example.invalid/message?token=A.gotify_token"
    );
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("synthetic body 123456"));
    assert!(matches!(ack, Acknowledgement::Http2xx));
}

#[test]
fn notice_and_masked_shares_never_leak_sender_or_digits() {
    for share in [Share::Notice, Share::MaskedText] {
        for destination in all() {
            let (_, body, _) = prepared(destination, share);
            let raw = body.to_string();
            assert!(!raw.contains("synthetic-sender"), "{raw}");
            assert!(!raw.contains("123456"), "{raw}");
            assert!(!raw.contains("synthetic-time"), "{raw}");
        }
    }
}

#[test]
fn dingtalk_and_feishu_signatures_match_the_documented_construction() {
    let (url, _, _) = prepared(
        Destination::Dingtalk {
            access_token: secret("dingtoken"),
            secret: Some(secret("SECsynthetic")),
        },
        Share::Notice,
    );
    let query = url.split_once('?').unwrap().1;
    let field = |name: &str| {
        query
            .split('&')
            .find_map(|p| p.strip_prefix(&format!("{name}=")))
            .unwrap()
            .to_string()
    };
    let timestamp: u128 = field("timestamp").parse().unwrap();
    assert!(timestamp > 1_700_000_000_000);
    assert_eq!(field("sign"), dingtalk_sign("SECsynthetic", timestamp));
    assert!(
        !field("sign").contains(['+', '/', '=']),
        "sign is URL-encoded"
    );
    assert!(
        !url.contains("SECsynthetic"),
        "the secret itself is never sent"
    );
    // Fixed vector: independent of the clock.
    assert_eq!(
        dingtalk_sign("secret", 1),
        base64(&hmac_sha256(b"secret", b"1\nsecret"))
            .replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D")
    );

    let (_, body, _) = prepared(
        Destination::Feishu {
            hook: secret("feishu-hook"),
            secret: Some(secret("feishusecret")),
        },
        Share::Notice,
    );
    let timestamp: u64 = body["timestamp"].as_str().unwrap().parse().unwrap();
    assert_eq!(body["sign"], feishu_sign("feishusecret", timestamp));
    assert_eq!(
        feishu_sign("secret", 1),
        base64(&hmac_sha256(b"1\nsecret", b""))
    );
    assert!(!body.to_string().contains("feishusecret"));
}

#[test]
fn tokens_that_could_alter_the_url_are_refused() {
    for bad in [
        "",
        "a/b",
        "a?b",
        "a&b=c",
        "a b",
        "a#b",
        "a%2F",
        "é",
        &"a".repeat(129),
    ] {
        let cases = vec![
            Destination::Dingtalk {
                access_token: secret(bad),
                secret: None,
            },
            Destination::Dingtalk {
                access_token: secret("ok"),
                secret: Some(secret(bad)),
            },
            Destination::Wecom { key: secret(bad) },
            Destination::Feishu {
                hook: secret(bad),
                secret: None,
            },
            Destination::Feishu {
                hook: secret("ok"),
                secret: Some(secret(bad)),
            },
            Destination::Serverchan {
                send_key: secret(bad),
            },
            Destination::Pushplus { token: secret(bad) },
            Destination::Gotify {
                server: "https://gotify.example.invalid/".into(),
                token: secret(bad),
            },
        ];
        for destination in cases {
            assert!(destination.validate().is_err(), "{bad:?}");
            assert!(prepare(
                &target(destination),
                &rule(Share::Notice),
                &message(),
                DELIVERY
            )
            .is_err());
        }
    }
    for server in [
        "http://gotify.example.invalid/",
        "https://gotify.example.invalid/path",
        "https://gotify.example.invalid/?x=1",
        "https://user@gotify.example.invalid/",
    ] {
        assert!(Destination::Gotify {
            server: server.into(),
            token: secret("ok"),
        }
        .validate()
        .is_err());
    }
    for destination in all() {
        assert!(destination.validate().is_ok());
    }
}

#[test]
fn provider_error_bodies_are_not_treated_as_delivered() {
    use Acknowledgement::{CodeZero, ErrcodeZero};
    assert!(acknowledged(
        ErrcodeZero,
        200,
        r#"{"errcode":0,"errmsg":"ok"}"#
    ));
    // DingTalk and WeCom answer HTTP 200 for a wrong token or keyword mismatch.
    assert!(!acknowledged(
        ErrcodeZero,
        200,
        r#"{"errcode":310000,"errmsg":"sign not match"}"#
    ));
    assert!(!acknowledged(ErrcodeZero, 200, r#"{"errcode":"0"}"#));
    assert!(!acknowledged(ErrcodeZero, 200, "ok"));
    assert!(!acknowledged(ErrcodeZero, 200, "{}"));
    assert!(!acknowledged(ErrcodeZero, 500, r#"{"errcode":0}"#));
    assert!(acknowledged(CodeZero, 200, r#"{"code":0,"msg":"success"}"#));
    assert!(acknowledged(
        CodeZero,
        200,
        r#"{"StatusCode":0,"StatusMessage":"success"}"#
    ));
    assert!(!acknowledged(
        CodeZero,
        200,
        r#"{"code":19021,"msg":"sign match fail"}"#
    ));
    assert!(!acknowledged(CodeZero, 200, r#"{"code":40001}"#));
    assert!(!acknowledged(CodeZero, 200, ""));
    // PushPlus shares Bark's `code == 200` rule.
    assert!(acknowledged(Acknowledgement::Bark, 200, r#"{"code":200}"#));
    assert!(!acknowledged(
        Acknowledgement::Bark,
        200,
        r#"{"code":903,"msg":"invalid token"}"#
    ));
    let large = format!(r#"{{"errcode":0,"pad":"{}"}}"#, "x".repeat(9000));
    assert!(
        !acknowledged(ErrcodeZero, 200, &large),
        "oversized replies are not trusted"
    );
}

#[test]
fn views_name_the_kind_and_never_return_credentials() {
    let policy = Policy {
        enabled: false,
        targets: all()
            .into_iter()
            .enumerate()
            .map(|(i, destination)| Target {
                id: format!("11111111-1111-4111-8111-11111111111{i}"),
                destination,
            })
            .collect(),
        rules: vec![],
    };
    policy.validate().unwrap();
    let view = serde_json::to_value(policy.view()).unwrap();
    let kinds: Vec<_> = view["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        kinds,
        [
            "dingtalk",
            "wecom",
            "feishu",
            "serverchan",
            "pushplus",
            "gotify"
        ]
    );
    let raw = view.to_string();
    for secret in [
        "dingtoken",
        "wecom-key",
        "feishu-hook",
        "SCTsynthetic",
        "pushplustoken",
        "A.gotify_token",
    ] {
        assert!(!raw.contains(secret), "{secret}");
    }
}

#[test]
fn owner_edits_accept_each_kind_and_refuse_unknown_fields() {
    for (kind, fields) in [
        ("dingtalk", r#""access_token":"t","secret":null"#),
        ("dingtalk", r#""access_token":"t","secret":"SECx""#),
        ("wecom", r#""key":"k""#),
        ("feishu", r#""hook":"h","secret":null"#),
        ("serverchan", r#""send_key":"SCTx""#),
        ("pushplus", r#""token":"t""#),
        (
            "gotify",
            r#""server":"https://gotify.example.invalid/","token":"t""#,
        ),
    ] {
        let edit: crate::PolicyEdit = serde_json::from_str(&format!(
            r#"{{"rules_revision":null,"enabled":false,"targets":[{{"id":"{TARGET}","destination":{{"kind":"{kind}",{fields}}}}}],"rules":[]}}"#
        ))
        .unwrap();
        let policy = edit
            .resolve(Policy::default())
            .unwrap_or_else(|_| panic!("{kind}"));
        assert_eq!(policy.view().targets[0].kind, kind);
        assert!(serde_json::from_str::<crate::PolicyEdit>(&format!(
            r#"{{"rules_revision":null,"enabled":false,"targets":[{{"id":"{TARGET}","destination":{{"kind":"{kind}",{fields},"url":"https://example.invalid/"}}}}],"rules":[]}}"#
        ))
        .is_err());
    }
}

#[test]
fn synthetic_tests_carry_only_the_fixed_notice() {
    for destination in all() {
        let delivery = prepare_test(&target(destination), DELIVERY, LineEpoch(1)).unwrap();
        let raw = test_https(&delivery).body.expose().to_string();
        assert!(raw.contains("合成测试"), "{raw}");
        assert!(!raw.contains("synthetic body"), "{raw}");
    }
}
