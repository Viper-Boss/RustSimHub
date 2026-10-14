// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use axum::body::Body;
use tokio::net::UnixListener;
use tower::ServiceExt;
fn app(path: PathBuf) -> App {
    let uid = std::fs::metadata(path.parent().unwrap()).unwrap().uid();
    App {
        origin: "https://localhost:8443".into(),
        authority: "localhost:8443".into(),
        socket: Arc::new(path),
        uid,
        legacy_compat: false,
        legacy_limit: Arc::new(Semaphore::new(2)),
        legacy_rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
        limit: Arc::new(Semaphore::new(8)),
        rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
    }
}
fn request(origin: &str, host: &str, body: Vec<u8>) -> Request {
    Request::builder()
        .method("POST")
        .uri("/api/v2")
        .header(header::HOST, host)
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-rustsimhub-request", "2")
        .body(Body::from(body))
        .unwrap()
}
#[tokio::test]
async fn host_origin_content_marker_size_and_role_fail_before_core() {
    let dir = tempfile::tempdir().unwrap();
    let a = app(dir.path().join("absent.sock"));
    let body = br#"{"protocol":2,"id":"a","command":{"op":"identity"}}"#.to_vec();
    for (origin, host) in [
        ("https://evil.invalid", "localhost:8443"),
        ("null", "localhost:8443"),
        ("https://localhost:8443", "evil.invalid"),
    ] {
        let response = router(a.clone())
            .oneshot(request(origin, host, body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    let mut missing = request("https://localhost:8443", "localhost:8443", body.clone());
    missing.headers_mut().remove("x-rustsimhub-request");
    assert_eq!(
        router(a.clone()).oneshot(missing).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut duplicate = request("https://localhost:8443", "localhost:8443", body.clone());
    duplicate.headers_mut().append(
        header::ORIGIN,
        HeaderValue::from_static("https://localhost:8443"),
    );
    assert_eq!(
        router(a.clone()).oneshot(duplicate).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut query = request("https://localhost:8443", "localhost:8443", body);
    *query.uri_mut() = "/api/v2?token=forbidden".parse().unwrap();
    assert_eq!(
        router(a.clone()).oneshot(query).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let injection =
        br#"{"protocol":2,"id":"a","command":{"op":"identity","role":"owner"}}"#.to_vec();
    assert_eq!(
        router(a.clone())
            .oneshot(request(
                "https://localhost:8443",
                "localhost:8443",
                injection
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        router(a)
            .oneshot(request(
                "https://localhost:8443",
                "localhost:8443",
                vec![b'x'; MAX_FRAME + 1]
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}
#[tokio::test]
async fn valid_core_response_is_forwarded_but_wrong_id_version_and_size_are_not() {
    for case in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let a = app(path);
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let len = stream.read_u32().await.unwrap() as usize;
            let mut bytes = vec![0; len];
            stream.read_exact(&mut bytes).await.unwrap();
            assert_eq!(parse_auth_request(&bytes).unwrap().id, "a");
            if case == 3 {
                stream.write_u32(MAX_FRAME as u32 + 1).await.unwrap();
                return;
            }
            let response=serde_json::to_vec(&serde_json::json!({"protocol":if case==2 {1}else{2},"id":if case==1 {"wrong"}else{"a"},"result":{"ok":true}})).unwrap();
            stream.write_u32(response.len() as u32).await.unwrap();
            stream.write_all(&response).await.unwrap();
        });
        let response = router(a)
            .oneshot(request(
                "https://localhost:8443",
                "localhost:8443",
                br#"{"protocol":2,"id":"a","command":{"op":"identity"}}"#.to_vec(),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if case == 0 {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        );
        task.await.unwrap();
    }
}
#[tokio::test]
async fn changed_core_uid_or_writable_parent_is_rejected_without_connecting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("core.sock");
    let _listener = UnixListener::bind(&path).unwrap();
    let mut a = app(path);
    a.uid = a.uid.wrapping_add(1);
    assert!(forward(&a, b"synthetic", "id").await.is_err());
    a.uid = std::fs::metadata(dir.path()).unwrap().uid();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(forward(&a, b"synthetic", "id").await.is_err());
}
#[test]
fn config_rejects_unrestricted_binds_credentials_and_noncanonical_origins() {
    let mut c = Config {
        listen: "127.0.0.1:8443".parse().unwrap(),
        origin: "https://localhost:8443".into(),
        core_socket: "/tmp/synthetic.sock".into(),
        core_uid: 0,
        tls_directory: "/tmp/synthetic".into(),
        legacy_compat: false,
    };
    assert_eq!(c.authority().unwrap(), "localhost:8443");
    for origin in [
        "http://localhost:8443",
        "https://localhost:8443/",
        "https://u:p@localhost:8443",
        "https://localhost:8443/?x=1",
    ] {
        c.origin = origin.into();
        assert!(c.authority().is_err());
    }
    c.origin = "https://localhost:8443".into();
    for address in ["0.0.0.0:8443", "8.8.8.8:8443", "127.0.0.1:0"] {
        c.listen = address.parse().unwrap();
        assert!(c.authority().is_err());
    }
    c.listen = "[fd00::1]:8443".parse().unwrap();
    c.origin = "https://[fd00::1]:8443".into();
    assert!(c.authority().is_ok());
}
#[test]
fn tls_material_is_private_and_never_overwritten() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("tls");
    init_tls(&dir, "localhost").unwrap();
    let key = std::fs::read(dir.join("key.der")).unwrap();
    assert!(tls(&dir).is_ok());
    assert!(init_tls(&dir, "localhost").is_err());
    assert_eq!(std::fs::read(dir.join("key.der")).unwrap(), key);
    std::fs::set_permissions(dir.join("key.der"), std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(tls(&dir).is_err());
}
#[test]
fn admission_rate_is_bounded_and_recovers_at_next_window() {
    let mut rate = Rate {
        since: Instant::now(),
        remaining: 2,
    };
    assert!(rate.take());
    assert!(rate.take());
    assert!(!rate.take());
    rate.since = Instant::now() - Duration::from_secs(61);
    assert!(rate.take());
    assert_eq!(rate.remaining, 119);
}
#[tokio::test]
async fn legacy_is_explicit_origin_checked_and_missing_device_proof_never_reaches_core() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("not-listening.sock");
    fn legacy_request() -> Request {
        Request::builder()
            .method("POST")
            .uri("/v1/session")
            .header(header::HOST, "localhost:8443")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-rsh-legacy-compat", "1")
            .body(Body::from("{}".to_owned()))
            .unwrap()
    }
    let response = router(app(path.clone()))
        .oneshot(legacy_request())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let mut a = app(path);
    a.legacy_compat = true;
    let response = router(a.clone()).oneshot(legacy_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let mut r = legacy_request();
    r.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("https://evil.example"),
    );
    assert_eq!(
        router(a.clone()).oneshot(r).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut r = legacy_request();
    r.headers_mut()
        .insert("x-nsh-pq-signature", HeaderValue::from_static("synthetic"));
    assert_eq!(
        router(a).oneshot(r).await.unwrap().status(),
        StatusCode::NOT_IMPLEMENTED
    );
}
#[tokio::test]
async fn legacy_capacity_does_not_consume_the_normal_api_budget() {
    let d = tempfile::tempdir().unwrap();
    let mut a = app(d.path().join("absent.sock"));
    a.legacy_compat = true;
    let _p1 = a.legacy_limit.acquire().await.unwrap();
    let _p2 = a.legacy_limit.acquire().await.unwrap();
    assert_eq!(a.limit.available_permits(), 8);
    a.legacy_rate.lock().unwrap().remaining = 0;
    let headers = axum::http::HeaderMap::new();
    let response = legacy_forward(&a, &headers, AuthCommand::Identity {}).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(a.rate.lock().unwrap().remaining, 120);
}
