// SPDX-License-Identifier: AGPL-3.0-only
//! A fake backend and an app-side signer drive the gateway exactly as the
//! iPhone would. Every number, secret and message is synthetic.
use super::*;
use crate::{seal_registration, sign};
use std::net::{IpAddr, Ipv4Addr};
use x25519_dalek::{PublicKey, StaticSecret};

const NOW_MS: i64 = 1_793_600_000_000;
const NOW: i64 = NOW_MS / 1000;
const PEER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
const PUBLIC: IpAddr = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));

#[derive(Default)]
struct Fake {
    forbidden: bool,
    live: Vec<Call>,
    history: Vec<Call>,
    messages: Vec<Message>,
    log: Vec<String>,
    fail: Option<BackendError>,
}
impl Fake {
    fn check(&self) -> Result<(), BackendError> {
        self.fail.clone().map_or(Ok(()), Err)
    }
}
fn call(id: &str, incoming: bool, phase: Phase) -> Call {
    Call {
        id: id.into(),
        incoming,
        phase,
        number: "10010".into(),
        created_ms: NOW_MS - 5000,
        updated_ms: NOW_MS,
        ended_ms: None,
        missed: false,
    }
}
impl Backend for Fake {
    fn authorize(&mut self, _: &str) -> Result<(), BackendError> {
        if self.forbidden {
            Err(BackendError::Forbidden)
        } else {
            Ok(())
        }
    }
    fn line(&mut self, _: &str) -> Result<Line, BackendError> {
        self.check()?;
        Ok(Line {
            device_name: "UFI003".into(),
            operator: "synthetic-operator".into(),
            registered: true,
            signal_dbm: Some(-90),
            iccid: "synthetic-iccid".into(),
            ..Line::default()
        })
    }
    fn calls(&mut self, _: &str, history: usize) -> Result<(Vec<Call>, Vec<Call>), BackendError> {
        self.check()?;
        Ok((
            self.live.clone(),
            self.history.iter().take(history).cloned().collect(),
        ))
    }
    fn dial(&mut self, _: &str, number: &str, request_id: &str) -> Result<Call, BackendError> {
        self.check()?;
        self.log.push(format!("dial {number} {request_id}"));
        let mut created = call("call-new", false, Phase::Dialing);
        created.number = number.into();
        self.live = vec![created.clone()];
        Ok(created)
    }
    fn call_action(
        &mut self,
        _: &str,
        call_id: &str,
        action: CallAction,
        _: &str,
    ) -> Result<(), BackendError> {
        self.check()?;
        self.log.push(format!("{action:?} {call_id}"));
        Ok(())
    }
    fn messages(
        &mut self,
        _: &str,
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<Message>, BackendError> {
        self.check()?;
        Ok(self
            .messages
            .iter()
            .filter(|m| m.timestamp_ms >= since_ms)
            .take(limit)
            .cloned()
            .collect())
    }
    fn send_message(
        &mut self,
        _: &str,
        phone: &str,
        text: &str,
        _: &str,
    ) -> Result<(), BackendError> {
        self.check()?;
        self.log.push(format!("sms {phone} {text}"));
        Ok(())
    }
}

/// The app's half: holds the VoWLAN secret and signs like VoWLANAuth.swift.
struct App {
    secret: [u8; 32],
    counter: u32,
}
impl App {
    fn headers(
        &mut self,
        method: &str,
        target: &str,
        body: &[u8],
        at: i64,
    ) -> (String, String, String) {
        self.counter += 1;
        let nonce = format!("nonce-{:06}", self.counter);
        let signature = sign(&self.secret, &canonical(method, target, body, at, &nonce));
        (at.to_string(), nonce, signature)
    }
    fn send(
        &mut self,
        g: &mut Gateway,
        b: &mut Fake,
        method: &str,
        target: &str,
        body: &str,
    ) -> Outcome {
        let (timestamp, nonce, signature) = self.headers(method, target, body.as_bytes(), NOW);
        g.control(
            &Request {
                method,
                target,
                version: Some("1"),
                timestamp: Some(&timestamp),
                nonce: Some(&nonce),
                signature: Some(&signature),
                body: body.as_bytes(),
                peer: PEER,
            },
            NOW_MS,
            b,
        )
    }
    fn ok(
        &mut self,
        g: &mut Gateway,
        b: &mut Fake,
        method: &str,
        target: &str,
        body: &str,
    ) -> Value {
        match self.send(g, b, method, target, body) {
            Outcome::Done(Response { status: 200, body }) => body,
            other => panic!("{method} {target}: {other:?}"),
        }
    }
    fn status(
        &mut self,
        g: &mut Gateway,
        b: &mut Fake,
        method: &str,
        target: &str,
        body: &str,
    ) -> (u16, Value) {
        match self.send(g, b, method, target, body) {
            Outcome::Done(r) => (r.status, r.body),
            other => panic!("{method} {target}: {other:?}"),
        }
    }
}

fn plain(
    method: &'static str,
    target: &'static str,
    body: &'static [u8],
    peer: IpAddr,
) -> Request<'static> {
    Request {
        method,
        target,
        version: None,
        timestamp: None,
        nonce: None,
        signature: None,
        body,
        peer,
    }
}

fn gateway() -> Gateway {
    let mut g = Gateway::new(State::new().unwrap(), 7592, 7593).unwrap();
    g.set_enabled(true);
    g
}

/// Runs the app's manual pairing against an open window.
fn pair_with(g: &mut Gateway, code: &str, secret: [u8; 32]) -> Response {
    let info = g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), NOW_MS);
    assert_eq!(info.status, 200, "{info:?}");
    let session = info.body["session_id"].as_str().unwrap().to_owned();
    let server = decode_base64url(info.body["server_public_key"].as_str().unwrap()).unwrap();
    let client = StaticSecret::from([5u8; 32]);
    let registration = json!({"vowlan_secret": base64url(&secret), "device_id": "synthetic-device-AB12",
        "apns_token": "must-not-be-kept"});
    let sealed = seal_registration(
        &client,
        &server,
        registration.to_string().as_bytes(),
        &session,
        code,
        [3; 12],
    )
    .unwrap();
    let claim = json!({"version": 1, "session_id": session,
        "client_public_key": base64url(PublicKey::from(&client).as_bytes()),
        "sealed_registration": base64url(&sealed), "want_direct_route": true})
    .to_string();
    let response = g.pairing(
        &Request {
            method: "POST",
            target: "/pair/v1/claim",
            version: None,
            timestamp: None,
            nonce: None,
            signature: None,
            body: claim.as_bytes(),
            peer: PEER,
        },
        NOW_MS,
    );
    if response.status == 200 {
        assert_eq!(
            response.body["route_proof"],
            direct_route_proof(
                &secret,
                &session,
                &base64url(PublicKey::from(&client).as_bytes()),
                g.agent_id(),
                7592,
                7593
            )
        );
    }
    response
}
fn paired() -> (Gateway, App) {
    let mut g = gateway();
    let (code, _) = g.open_window(NOW).unwrap();
    let secret = [0x42; 32];
    assert_eq!(pair_with(&mut g, &code, secret).status, 200);
    (g, App { secret, counter: 0 })
}

#[test]
fn pairing_needs_an_open_window_the_right_code_and_a_private_peer() {
    let mut g = gateway();
    assert_eq!(
        g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), NOW_MS)
            .status,
        410,
        "no window yet"
    );
    let (code, expires) = g.open_window(NOW).unwrap();
    assert_eq!(expires, NOW + 120);
    assert_eq!(
        g.pairing(&plain("GET", "/pair/v1/info", b"", PUBLIC), NOW_MS)
            .status,
        403
    );
    assert_eq!(
        g.pairing(&plain("GET", "/pair/v1/other", b"", PEER), NOW_MS)
            .status,
        404
    );
    let info = g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), NOW_MS);
    assert_eq!(info.body["product"], "airsim-pair");
    assert_eq!(info.body["expires_at"], (NOW + 120) * 1000);
    assert!(
        !info.body.to_string().contains(&code),
        "the code is never sent to the app"
    );

    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(pair_with(&mut g, wrong, [0x42; 32]).status, 401);
    assert!(g.pairings().is_empty());
    assert_eq!(
        g.pairing(&plain("POST", "/pair/v1/claim", b"{}", PEER), NOW_MS)
            .status,
        400
    );
    assert_eq!(
        g.pairing(&plain("POST", "/pair/v1/claim", b"not json", PEER), NOW_MS)
            .status,
        400
    );
    assert!(g.take_dirty().is_none(), "failed attempts store nothing");

    let done = pair_with(&mut g, &code, [0x42; 32]);
    assert_eq!(done.status, 200, "a wrong code did not consume the window");
    assert_eq!(done.body["device_id_hint"], "AB12");
    assert_eq!(done.body["control_port"], 7592);
    assert_eq!(done.body["pcm_port"], 7593);
    let stored = serde_json::to_string(g.take_dirty().expect("pairing is persisted")).unwrap();
    assert!(
        !stored.contains("must-not-be-kept"),
        "push tokens are not kept"
    );
    assert_eq!(g.pairings().len(), 1);
    assert_eq!(g.pairings()[0].last_peer, "192.168.1.20");
    assert!(!serde_json::to_string(&g.pairings())
        .unwrap()
        .contains("secret"));
    assert_eq!(
        g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), NOW_MS)
            .status,
        410,
        "the window is single use"
    );
}

#[test]
fn a_window_expires_and_short_secrets_or_a_full_store_are_refused() {
    let mut g = gateway();
    g.open_window(NOW).unwrap();
    let late = (NOW + 121) * 1000;
    assert_eq!(
        g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), late)
            .status,
        410
    );
    assert_eq!(g.window_remaining(NOW + 121), None);

    let (code, _) = g.open_window(NOW).unwrap();
    let info = g.pairing(&plain("GET", "/pair/v1/info", b"", PEER), NOW_MS);
    let session = info.body["session_id"].as_str().unwrap().to_owned();
    let server = decode_base64url(info.body["server_public_key"].as_str().unwrap()).unwrap();
    let client = StaticSecret::from([5u8; 32]);
    let sealed = seal_registration(
        &client,
        &server,
        json!({"vowlan_secret": base64url(&[1; 31])})
            .to_string()
            .as_bytes(),
        &session,
        &code,
        [3; 12],
    )
    .unwrap();
    let claim = json!({"version": 1, "session_id": session, "client_public_key": base64url(PublicKey::from(&client).as_bytes()), "sealed_registration": base64url(&sealed)}).to_string();
    let mut request = plain("POST", "/pair/v1/claim", b"", PEER);
    request.body = claim.as_bytes();
    assert_eq!(
        g.pairing(&request, NOW_MS).body["error"],
        "vowlan_secret_required"
    );

    for n in 0..MAX_PAIRINGS {
        let (code, _) = g.open_window(NOW).unwrap();
        assert_eq!(pair_with(&mut g, &code, [n as u8 + 1; 32]).status, 200);
    }
    let (code, _) = g.open_window(NOW).unwrap();
    assert_eq!(
        pair_with(&mut g, &code, [0xee; 32]).body["error"],
        "too_many_paired_devices"
    );
    assert_eq!(g.pairings().len(), MAX_PAIRINGS);

    g.set_enabled(false);
    assert!(
        g.open_window(NOW).is_err(),
        "a disabled gateway opens no window"
    );
}

#[test]
fn control_requests_must_be_signed_fresh_unreplayed_and_private() {
    let (mut g, mut app) = paired();
    let mut b = Fake::default();
    let health = app.ok(&mut g, &mut b, "GET", "/v1/health", "");
    assert_eq!(health["agent_id"], g.agent_id());
    assert_eq!(health["transport"], "vowlan");
    assert_eq!(health["sim_slots"][0]["carrier_name"], "synthetic-operator");

    let target = "/api/status";
    let (timestamp, nonce, signature) = app.headers("GET", target, b"", NOW);
    let mut request = Request {
        method: "GET",
        target,
        version: Some("1"),
        timestamp: Some(&timestamp),
        nonce: Some(&nonce),
        signature: Some(&signature),
        body: b"",
        peer: PEER,
    };
    let run = |g: &mut Gateway, b: &mut Fake, r: &Request<'_>| match g.control(r, NOW_MS, b) {
        Outcome::Done(r) => r.status,
        other => panic!("{other:?}"),
    };
    request.peer = PUBLIC;
    assert_eq!(run(&mut g, &mut b, &request), 403);
    request.peer = PEER;
    request.version = Some("2");
    assert_eq!(run(&mut g, &mut b, &request), 401);
    request.version = Some("1");
    request.target = "/api/status?x=1";
    assert_eq!(
        run(&mut g, &mut b, &request),
        401,
        "the signature covers the exact target"
    );
    request.target = target;
    request.body = b"{}";
    assert_eq!(run(&mut g, &mut b, &request), 401, "and the body");
    request.body = b"";
    assert_eq!(
        run(&mut g, &mut b, &request),
        200,
        "failed attempts did not burn the nonce"
    );
    assert_eq!(run(&mut g, &mut b, &request), 401, "a replay is refused");

    for at in [NOW - 31, NOW + 31] {
        let (timestamp, nonce, signature) = app.headers("GET", target, b"", at);
        let stale = Request {
            method: "GET",
            target,
            version: Some("1"),
            timestamp: Some(&timestamp),
            nonce: Some(&nonce),
            signature: Some(&signature),
            body: b"",
            peer: PEER,
        };
        assert_eq!(run(&mut g, &mut b, &stale), 401, "stale timestamp {at}");
    }
    let mut stranger = App {
        secret: [0x43; 32],
        counter: 100,
    };
    assert_eq!(stranger.status(&mut g, &mut b, "GET", target, "").0, 401);
    assert_eq!(
        app.status(&mut g, &mut b, "GET", "/api/at", "").0,
        404,
        "unknown routes are not forwarded"
    );
    assert_eq!(app.status(&mut g, &mut b, "DELETE", "/api/sms", "").0, 404);
    assert_eq!(app.status(&mut g, &mut b, "GET", "/api/debug", "").0, 404);
    assert_eq!(
        app.status(&mut g, &mut b, "POST", "/api/calls/dial", "{not json")
            .0,
        400
    );
    assert!(
        b.log.is_empty(),
        "nothing above reached the backend's actions"
    );
}

#[test]
fn revoked_disabled_or_unauthorised_pairings_stop_working_at_once() {
    let (mut g, mut app) = paired();
    let mut b = Fake::default();
    let id = g.pairings()[0].id.clone();
    g.set_enabled(false);
    assert_eq!(app.status(&mut g, &mut b, "GET", "/api/health", "").0, 503);
    g.set_enabled(true);
    assert_eq!(app.status(&mut g, &mut b, "GET", "/api/health", "").0, 200);

    b.forbidden = true;
    g.take_dirty();
    assert_eq!(app.status(&mut g, &mut b, "GET", "/api/health", "").0, 401);
    assert!(
        g.pairings().is_empty(),
        "a pairing whose authority is gone is removed"
    );
    assert!(g.take_dirty().is_some());
    b.forbidden = false;
    assert_eq!(app.status(&mut g, &mut b, "GET", "/api/health", "").0, 401);
    assert!(!g.revoke(&id), "already gone");

    let (mut g, mut app) = paired();
    let id = g.pairings()[0].id.clone();
    assert!(g.revoke(&id));
    assert_eq!(
        app.status(
            &mut g,
            &mut b,
            "POST",
            "/api/calls/dial",
            r#"{"number":"10010"}"#
        )
        .0,
        401
    );
    assert!(b.log.is_empty());
}

#[test]
fn dialling_and_call_control_reach_the_backend_with_bounded_input() {
    let (mut g, mut app) = paired();
    let mut b = Fake::default();
    for bad in [r#"{"number":""}"#, r#"{"number":"   "}"#, "{}"] {
        assert_eq!(
            app.status(&mut g, &mut b, "POST", "/api/calls/dial", bad).1["error"],
            "number_required"
        );
    }
    let long = json!({"number": "1".repeat(65)}).to_string();
    assert_eq!(
        app.status(&mut g, &mut b, "POST", "/api/calls/dial", &long)
            .0,
        400
    );

    let dialled = app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/calls/dial",
        r#"{"number":" 10010 ","operation_id":"op-1"}"#,
    );
    assert_eq!(dialled["call_id"], "call-new");
    assert_eq!(dialled["android_id"], "call-new");
    assert_eq!(
        dialled["operation_proof"],
        dial_proof(
            &app.secret,
            &format!("nonce-{:06}", app.counter),
            "op-1",
            "call-new",
            "call-new"
        )
    );
    assert!(b.log[0].starts_with("dial 10010 airsim-dial-"));
    let first = b.log[0].clone();
    app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/calls/dial",
        r#"{"number":"10010","operation_id":"op-1"}"#,
    );
    assert_eq!(
        b.log[1], first,
        "the same operation maps to the same backend request"
    );
    let plain = app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/calls/dial",
        r#"{"number":"10010"}"#,
    );
    assert!(plain.get("operation_proof").is_none());

    b.log.clear();
    app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/calls/dtmf",
        r##"{"digit":"#"}"##,
    );
    for bad in [r#"{"digit":"12"}"#, r#"{"digit":"x"}"#, r#"{"digit":""}"#] {
        assert_eq!(
            app.status(&mut g, &mut b, "POST", "/api/calls/dtmf", bad).1["error"],
            "invalid_digit"
        );
    }
    assert_eq!(
        app.status(
            &mut g,
            &mut b,
            "POST",
            "/api/calls/answer",
            r#"{"call_id":"other"}"#
        )
        .0,
        404
    );
    assert_eq!(
        app.ok(&mut g, &mut b, "POST", "/api/calls/reject", "")["rejected"],
        true
    );
    assert_eq!(
        b.log,
        ["Dtmf('#') call-new", "Hangup call-new"],
        "rejecting a non-ringing call hangs it up"
    );

    b.live = vec![call("call-in", true, Phase::Ringing)];
    b.log.clear();
    app.ok(&mut g, &mut b, "POST", "/api/calls/reject", "");
    app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/calls/answer",
        r#"{"call_id":"call-in"}"#,
    );
    assert_eq!(b.log, ["Reject call-in", "Answer call-in"]);

    b.live.clear();
    b.log.clear();
    assert_eq!(
        app.ok(&mut g, &mut b, "POST", "/api/calls/hangup", "")["ok"],
        true
    );
    assert_eq!(
        app.status(&mut g, &mut b, "POST", "/api/calls/answer", "")
            .1["error"],
        "no_active_call"
    );
    assert!(
        b.log.is_empty(),
        "nothing is sent for a call that is already gone"
    );

    b.fail = Some(BackendError::Message(409, "线路正在使用".into()));
    assert_eq!(
        app.status(
            &mut g,
            &mut b,
            "POST",
            "/api/calls/dial",
            r#"{"number":"10010"}"#
        ),
        (409, json!({"error": "线路正在使用"}))
    );
    b.fail = Some(BackendError::Other);
    assert_eq!(
        app.status(
            &mut g,
            &mut b,
            "POST",
            "/api/calls/dial",
            r#"{"number":"10010"}"#
        )
        .0,
        502
    );
}

#[test]
fn messages_are_listed_after_pairing_acknowledged_once_and_sent_bounded() {
    let (mut g, mut app) = paired();
    let mut b = Fake::default();
    let message = |id: &str, content: &str, at: i64, outgoing: bool| Message {
        delivery_id: id.into(),
        sender: "synthetic-sender".into(),
        content: content.into(),
        outgoing,
        timestamp_ms: at,
    };
    b.messages = vec![
        message("m3", "您的验证码是 482916，请勿泄露", NOW_MS + 3, false),
        message("m2", "hello", NOW_MS + 2, false),
        message("out", "sent by us", NOW_MS + 2, true),
        message("", "no id", NOW_MS + 2, false),
        message("old", "before pairing", NOW_MS - 1, false),
    ];
    let list = app.ok(&mut g, &mut b, "GET", "/api/sms", "");
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["delivery_id"], "m3");
    assert_eq!(list[0]["code"], "482916");
    assert!(list[1].get("code").is_none());
    assert_eq!(list[0]["timestamp"], "2026-11-02T06:13:20.003Z");
    assert_eq!(
        app.ok(&mut g, &mut b, "GET", "/api/sms/status", "")["count"],
        2
    );

    g.take_dirty();
    assert_eq!(
        app.ok(
            &mut g,
            &mut b,
            "POST",
            "/api/sms/ack",
            r#"{"ids":["m3","m3",7,""]}"#
        )["acknowledged"],
        2
    );
    assert!(
        g.take_dirty().is_some(),
        "acknowledgements survive a restart"
    );
    let list = app.ok(&mut g, &mut b, "GET", "/api/sms", "");
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["delivery_id"], "m2");

    let sent = app.ok(
        &mut g,
        &mut b,
        "POST",
        "/api/sms/send",
        r#"{"phone":" 10010 ","message":"CXLL"}"#,
    );
    assert_eq!(sent, json!({"sent": true, "segments": 1}));
    assert_eq!(b.log, ["sms 10010 CXLL"]);
    let too_long = json!({"phone": "10010", "message": "字".repeat(1601)}).to_string();
    for bad in [
        r#"{"phone":"","message":"x"}"#,
        r#"{"phone":"10010","message":""}"#,
        too_long.as_str(),
    ] {
        assert_eq!(
            app.status(&mut g, &mut b, "POST", "/api/sms/send", bad).1["error"],
            "invalid_message"
        );
    }
    assert_eq!(b.log.len(), 1);
}

#[test]
fn long_polls_wait_for_a_change_and_end_on_timeout_or_revocation() {
    let (mut g, mut app) = paired();
    let mut b = Fake::default();
    let first = app.ok(&mut g, &mut b, "GET", "/api/events", "");
    let revision = first["revision"].as_u64().unwrap();
    assert!(revision > 0);
    assert_eq!(first["call"]["active"], Value::Null);
    assert_eq!(first["call"]["event_driven"], true);

    let target = format!("/api/events?after={revision}&timeout_ms=99999");
    let Outcome::Wait(poll) = app.send(&mut g, &mut b, "GET", &target, "") else {
        panic!("nothing changed: the gateway must hold the request");
    };
    assert_eq!(poll.timeout_ms, MAX_LONG_POLL_MS, "the wait is capped");
    assert_eq!(g.poll(&poll, NOW_MS + 400, &mut b, false), None);

    b.live = vec![call("call-in", true, Phase::Ringing)];
    let changed = g
        .poll(&poll, NOW_MS + 800, &mut b, false)
        .expect("a ringing call ends the wait");
    assert_eq!(changed.body["call"]["active"]["state"], "incoming");
    assert_eq!(changed.body["call"]["active"]["direction"], "incoming");
    let next = changed.body["revision"].as_u64().unwrap();
    assert_ne!(next, revision);

    let again = Poll {
        after: next,
        ..poll.clone()
    };
    assert_eq!(g.poll(&again, NOW_MS + 1200, &mut b, false), None);
    let timed_out = g
        .poll(&again, NOW_MS + 30_000, &mut b, true)
        .expect("the deadline answers");
    assert_eq!(timed_out.body["revision"], next);

    let calls_only = format!("/api/calls/events?after={next}&timeout_ms=0");
    let now = app.ok(&mut g, &mut b, "GET", &calls_only, "");
    assert_eq!(
        now["active"]["id"], "call-in",
        "timeout 0 answers at once with the call view"
    );

    b.fail = Some(BackendError::Unavailable);
    assert_eq!(g.poll(&again, NOW_MS, &mut b, false).unwrap().status, 503);
    b.fail = None;
    let id = g.pairings()[0].id.clone();
    g.revoke(&id);
    assert_eq!(
        g.poll(&again, NOW_MS, &mut b, false).unwrap().status,
        401,
        "a revoked phone's held poll ends"
    );
}

#[test]
fn pcm_prefaces_are_matched_to_a_pairing_once() {
    let (mut g, app) = paired();
    let id = g.pairings()[0].id.clone();
    let line = |secret: &[u8], at: i64, nonce: &str| {
        format!(
            "AIRSIMVWL1 {at} {nonce} {}\n",
            sign(secret, &canonical("PCM", "/v1/pcm", b"", at, nonce))
        )
    };
    let good = line(&app.secret, NOW, "nonce-pcm-1");
    assert_eq!(g.authenticate_pcm(&good, PUBLIC, NOW), None);
    assert_eq!(
        g.authenticate_pcm(&line(&[9; 32], NOW, "nonce-pcm-2"), PEER, NOW),
        None
    );
    assert_eq!(
        g.authenticate_pcm(&line(&app.secret, NOW - 31, "nonce-pcm-3"), PEER, NOW),
        None
    );
    assert_eq!(g.authenticate_pcm(&good, PEER, NOW), Some(id));
    assert_eq!(
        g.authenticate_pcm(&good, PEER, NOW),
        None,
        "a preface cannot be replayed"
    );
    g.set_enabled(false);
    assert_eq!(
        g.authenticate_pcm(&line(&app.secret, NOW, "nonce-pcm-4"), PEER, NOW),
        None
    );
}

#[test]
fn state_round_trips_and_damaged_state_is_refused() {
    let (mut g, mut app) = paired();
    let stored = serde_json::to_string(g.take_dirty().unwrap()).unwrap();
    let mut restored = Gateway::new(serde_json::from_str(&stored).unwrap(), 7592, 7593).unwrap();
    restored.set_enabled(true);
    let mut b = Fake::default();
    assert_eq!(
        app.status(&mut restored, &mut b, "GET", "/api/health", "")
            .0,
        200,
        "a restart keeps the pairing"
    );

    let mut value: Value = serde_json::from_str(&stored).unwrap();
    for damage in [
        |v: &mut Value| v["format"] = 2.into(),
        |v: &mut Value| v["agent_id"] = "bad id".into(),
        |v: &mut Value| v["pairings"][0]["secret"] = "c2hvcnQ".into(),
        |v: &mut Value| v["pairings"][0]["id"] = "x".into(),
    ] {
        let mut broken = value.clone();
        damage(&mut broken);
        assert!(Gateway::new(serde_json::from_value(broken).unwrap(), 7592, 7593).is_err());
    }
    value["unknown"] = 1.into();
    assert!(serde_json::from_value::<State>(value).is_err());
}

#[test]
fn helpers_follow_the_reference_behaviour() {
    assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00.000Z");
    assert_eq!(iso(1_793_600_000_123), "2026-11-02T06:13:20.123Z");
    assert_eq!(iso(-5), "1970-01-01T00:00:00.000Z");
    assert_eq!(sms_segments(&"a".repeat(160)), 1);
    assert_eq!(sms_segments(&"a".repeat(161)), 2);
    assert_eq!(sms_segments(&"字".repeat(70)), 1);
    assert_eq!(sms_segments(&"字".repeat(71)), 2);
    assert_eq!(verification_code("Your code is 123456."), Some("123456"));
    assert_eq!(verification_code("验证码1234"), Some("1234"));
    assert_eq!(verification_code("验证码 123 和 123456789"), None);
    assert_eq!(
        verification_code("余额 123456 元"),
        None,
        "no code wording, no code"
    );
    for (address, allowed) in [
        ("127.0.0.1", true),
        ("10.1.2.3", true),
        ("172.16.0.1", true),
        ("172.32.0.1", false),
        ("192.168.120.167", true),
        ("169.254.1.1", true),
        ("100.64.0.1", true),
        ("100.128.0.1", false),
        ("1.1.1.1", false),
        ("::1", true),
        ("fd00::1", true),
        ("fe80::1", true),
        ("2001:db8::1", false),
        ("::ffff:192.168.1.1", true),
        ("::ffff:8.8.8.8", false),
    ] {
        assert_eq!(private_peer(address.parse().unwrap()), allowed, "{address}");
    }
    let ended = Call {
        phase: Phase::Ended,
        ended_ms: None,
        ..call("c", false, Phase::Ended)
    };
    assert_eq!(
        record(&ended, 1)["ended_at"],
        iso(NOW_MS),
        "falls back to the last update"
    );
    assert_eq!(airsim_state(&call("c", true, Phase::Connecting)), "active");
    assert_eq!(
        airsim_state(&call("c", false, Phase::Connecting)),
        "alerting"
    );
}
