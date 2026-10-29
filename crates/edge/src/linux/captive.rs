// SPDX-License-Identifier: AGPL-3.0-only
//! Plain-HTTP first-setup page on the open setup AP (user decision 2026-10-28).
//! Serves only the setup page and an allowlisted set of first-setup commands; every other
//! request is redirected to the page so phones show it as a captive portal. Root's AP
//! firewall forwards port 80 here only while the open (first-setup) hotspot is up.
use super::*;

pub(super) const PAGE: &str = "http://192.168.100.1/";
pub(super) const LISTEN: SocketAddr = SocketAddr::V4(std::net::SocketAddrV4::new(
    std::net::Ipv4Addr::new(192, 168, 100, 1),
    8080,
));
const HOST: &[u8] = b"192.168.100.1";
const ORIGIN: &[u8] = b"http://192.168.100.1";
const MAX_BODY: usize = 16 * 1024;

/// Commands a first-time user needs on the hotspot; nothing about SMS, calls, forwarding,
/// history, pairing or device settings. Core still authorizes each one.
pub(super) fn allowed(command: &AuthCommand) -> bool {
    matches!(
        command,
        AuthCommand::Identity { .. }
            | AuthCommand::SetupOpenChallenge { .. }
            | AuthCommand::SetupOpenFinish { .. }
            | AuthCommand::Challenge { .. }
            | AuthCommand::Authenticate { .. }
            | AuthCommand::SessionChallenge { .. }
            | AuthCommand::RenewSession { .. }
            | AuthCommand::WifiNetworks { .. }
            // The current Owner adopts the already-open first-setup round via
            // this command before station trial. Core still checks the real
            // Owner and exact live bootstrap binding; this does not claim one.
            | AuthCommand::SetupApStart { .. }
            | AuthCommand::SetupApStatus { .. }
            | AuthCommand::SetupApOperationStatus { .. }
            | AuthCommand::SetupApCancel { .. }
            | AuthCommand::SetupApWifiBegin { .. }
            | AuthCommand::SetupApWifiStatus { .. }
            | AuthCommand::SetupApWifiCancel { .. }
            | AuthCommand::SetupApWifiConfirm { .. }
            | AuthCommand::SetupApWifiCommitStatus { .. }
    )
}
fn redirect() -> Response {
    (StatusCode::FOUND, [(header::LOCATION, PAGE)]).into_response()
}
fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, content_type)], body).into_response()
}
async fn api(State(app): State<App>, body: Bytes) -> Response {
    if body.len() > MAX_BODY {
        return denied(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let request = match parse_auth_request(&body) {
        Ok(r) => r,
        Err(_) => return denied(StatusCode::BAD_REQUEST),
    };
    if !allowed(&request.command) {
        return denied(StatusCode::NOT_FOUND);
    }
    let permit = match app.limit.try_acquire() {
        Ok(v) => v,
        Err(_) => return denied(StatusCode::SERVICE_UNAVAILABLE),
    };
    if !app.rate.lock().is_ok_and(|mut r| r.take()) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let result = forward(&app, &body, &request.id).await;
    drop(permit);
    match result {
        Ok(reply) => ([(header::CONTENT_TYPE, "application/json")], reply).into_response(),
        Err(_) => denied(StatusCode::SERVICE_UNAVAILABLE),
    }
}
/// Exact Host for everything served; API calls also need the http Origin, JSON and the marker.
async fn boundary(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host_ok = headers.get_all(header::HOST).iter().count() == 1
        && headers
            .get(header::HOST)
            .is_some_and(|v| v.as_bytes() == HOST);
    let is_api = req.uri().path() == "/api/v2";
    let api_ok = req.method() == axum::http::Method::POST
        && req.uri().query().is_none()
        && headers.get_all(header::ORIGIN).iter().count() == 1
        && headers
            .get(header::ORIGIN)
            .is_some_and(|v| v.as_bytes() == ORIGIN)
        && headers.get_all(header::CONTENT_TYPE).iter().count() == 1
        && headers
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes() == b"application/json")
        && headers
            .get("x-rustsimhub-request")
            .is_some_and(|v| v.as_bytes() == b"2");
    let mut response = if is_api && !(host_ok && api_ok) {
        denied(StatusCode::FORBIDDEN)
    } else if !is_api && !host_ok {
        // Connectivity probes (any name, via the AP's wildcard DNS) land on the page.
        redirect()
    } else {
        next.run(req).await
    };
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("content-security-policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}
pub(super) fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async {
                asset(
                    "text/html; charset=utf-8",
                    include_str!("../web/setup.html"),
                )
            }),
        )
        .route(
            "/setup.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/setup.js"),
                )
            }),
        )
        .route(
            "/password.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/password.js"),
                )
            }),
        )
        .route(
            "/ap-wifi.js",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/ap-wifi.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async { asset("text/css; charset=utf-8", include_str!("../web/style.css")) }),
        )
        .route("/api/v2", post(api))
        .fallback(|| async { redirect() })
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .layer(axum::middleware::from_fn(boundary))
        .with_state(app)
}
/// Serves until the process ends; the AP address exists only while the hotspot is up.
pub(super) async fn serve(addr: SocketAddr, app: App) {
    // Own rate and concurrency budget: the phone's page polling must never starve Root's
    // readiness probe on the https listener (field test 2026-10-29).
    let app = App {
        rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
        limit: Arc::new(Semaphore::new(4)),
        ..app
    };
    loop {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                let _ = axum::serve(listener, router(app.clone())).await;
            }
            Err(_) => tokio::time::sleep(Duration::from_secs(5)).await,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parse(op: serde_json::Value) -> AuthCommand {
        parse_auth_request(
            &serde_json::to_vec(&serde_json::json!({"protocol":2,"id":"t","command":op})).unwrap(),
        )
        .unwrap()
        .command
    }
    #[test]
    fn only_first_setup_commands_pass_the_captive_allowlist() {
        let t = "a".repeat(64);
        for op in [
            serde_json::json!({"op":"identity"}),
            serde_json::json!({"op":"setup_open_challenge","public_key":vec![1;32]}),
            serde_json::json!({"op":"wifi_networks","token":t}),
            serde_json::json!({"op":"setup_ap_status","token":t}),
            serde_json::json!({"op":"setup_ap_start","token":t,"idempotency_key":"once"}),
        ] {
            assert!(allowed(&parse(op.clone())), "{op}");
        }
        for op in [
            serde_json::json!({"op":"inbox","token":t,"after":null,"limit":10}),
            serde_json::json!({"op":"setup_challenge","setup_code":"0".repeat(64),"public_key":vec![1;32]}),
            serde_json::json!({"op":"open_pairing","token":t}),
            serde_json::json!({"op":"line_status","token":t}),
        ] {
            if let Ok(r) = parse_auth_request(
                &serde_json::to_vec(&serde_json::json!({"protocol":2,"id":"t","command":op}))
                    .unwrap(),
            ) {
                assert!(!allowed(&r.command), "{op}");
            }
        }
    }
    #[tokio::test]
    async fn owner_adoption_start_reaches_core_on_the_real_captive_router() {
        let body=serde_json::json!({"protocol":2,"id":"adopt","command":{"op":"setup_ap_start","token":"a".repeat(64),"idempotency_key":"01010101-0101-4101-8101-010101010101"}}).to_string();
        // No Core is installed in this test: a forwarded request must fail 503.
        // 404 means the captive route blocked the UI's required adoption command.
        let response = send(post("http://192.168.100.1", &body)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
    async fn send(req: axum::http::Request<axum::body::Body>) -> Response {
        use tower::ServiceExt;
        let app = App {
            origins: Arc::from(vec![]),
            socket: Arc::new(PathBuf::from("/nonexistent/core.sock")),
            uid: 1,
            limit: Arc::new(Semaphore::new(4)),
            rate: Arc::new(Mutex::new(Rate {
                since: Instant::now(),
                remaining: 120,
            })),
            legacy_compat: false,
            legacy_rate: Arc::new(Mutex::new(Rate {
                since: Instant::now(),
                remaining: 1,
            })),
            legacy_limit: Arc::new(Semaphore::new(1)),
            media: None,
        };
        router(app).oneshot(req).await.unwrap()
    }
    fn get(host: &str, path: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::get(path)
            .header(header::HOST, host)
            .body(axum::body::Body::empty())
            .unwrap()
    }
    fn post(origin: &str, body: &str) -> axum::http::Request<axum::body::Body> {
        axum::http::Request::post("/api/v2")
            .header(header::HOST, "192.168.100.1")
            .header(header::ORIGIN, origin)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-rustsimhub-request", "2")
            .body(axum::body::Body::from(body.to_owned()))
            .unwrap()
    }
    #[tokio::test]
    async fn probes_redirect_page_is_served_and_api_needs_exact_origin_and_allowlist() {
        for (host, path) in [
            ("connectivitycheck.gstatic.com", "/generate_204"),
            ("captive.apple.com", "/hotspot-detect.html"),
            ("www.msftconnecttest.com", "/connecttest.txt"),
            ("192.168.100.1", "/anything"),
            ("evil.example", "/"),
        ] {
            let r = send(get(host, path)).await;
            assert_eq!(r.status(), StatusCode::FOUND, "{host}{path}");
            assert_eq!(r.headers()[header::LOCATION], PAGE);
        }
        let page = send(get("192.168.100.1", "/")).await;
        assert_eq!(page.status(), StatusCode::OK);
        assert!(page.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("default-src 'none'"));
        let inbox = r#"{"protocol":2,"id":"t","command":{"op":"inbox","token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","after":null,"limit":10}}"#;
        let identity = r#"{"protocol":2,"id":"t","command":{"op":"identity"}}"#;
        assert_eq!(
            send(post("http://192.168.100.1", inbox)).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(post("https://192.168.100.1:8443", identity))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(post("http://evil.example", identity)).await.status(),
            StatusCode::FORBIDDEN
        );
        // Allowed and well-formed: reaches the Core forwarder (absent here -> 503, not 4xx).
        assert_eq!(
            send(post("http://192.168.100.1", identity)).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        let big = format!(
            r#"{{"protocol":2,"id":"{}","command":{{"op":"identity"}}}}"#,
            "x".repeat(MAX_BODY)
        );
        assert!(send(post("http://192.168.100.1", &big))
            .await
            .status()
            .is_client_error());
    }
}
