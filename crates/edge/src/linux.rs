// SPDX-License-Identifier: AGPL-3.0-only
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use rustsimhub_wire::{
    check_len, parse_auth_request, AuthCommand, Response as CoreResponse, AUTH_PROTOCOL, MAX_FRAME,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    net::SocketAddr,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UnixStream},
    sync::Semaphore,
    task::JoinSet,
    time::timeout,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    },
    TlsAcceptor,
};
type Result<T> = std::result::Result<T, &'static str>;
mod installation;
mod media;

#[derive(Deserialize, serde::Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Config {
    listen: SocketAddr,
    origin: String,
    core_socket: PathBuf,
    core_uid: u32,
    tls_directory: PathBuf,
    #[serde(default)]
    legacy_compat: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_worker_socket: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    additional_listen_ips: Vec<std::net::IpAddr>,
}
impl Config {
    fn authority(&self) -> Result<String> {
        let uri: axum::http::Uri = self.origin.parse().map_err(|_| "invalid origin")?;
        let authority = uri.authority().ok_or("invalid origin")?;
        if uri.scheme_str() != Some("https")
            || authority.as_str().contains('@')
            || uri.path_and_query().is_some_and(|v| v.as_str() != "/")
            || self.origin != format!("https://{authority}")
        {
            return Err("invalid origin");
        }
        let ip = self.listen.ip();
        let local = match ip {
            std::net::IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
            std::net::IpAddr::V6(v) => {
                v.is_loopback() || v.is_unique_local() || v.is_unicast_link_local()
            }
        };
        if !local || self.listen.port() == 0 || self.origin.len() > 255 {
            return Err("explicit local bind required");
        }
        if self.additional_listen_ips.len() > 3
            || self
                .additional_listen_ips
                .iter()
                .enumerate()
                .any(|(i, ip)| {
                    *ip == self.listen.ip()
                        || self.additional_listen_ips[..i].contains(ip)
                        || !match ip {
                            std::net::IpAddr::V4(v) => {
                                v.is_private() || v.is_loopback() || v.is_link_local()
                            }
                            std::net::IpAddr::V6(v) => {
                                v.is_loopback() || v.is_unique_local() || v.is_unicast_link_local()
                            }
                        }
                })
        {
            return Err("invalid additional listen address");
        }
        if let Some(path) = &self.media_worker_socket {
            if !self.listen.is_ipv4()
                || self.additional_listen_ips.iter().any(|ip| !ip.is_ipv4())
                || !path.is_absolute()
                || path
                    .to_str()
                    .is_none_or(|v| v.len() > 100 || v.split('/').any(|c| matches!(c, "." | "..")))
            {
                return Err("explicit IPv4 media socket required");
            }
        }
        Ok(authority.as_str().to_owned())
    }
}
struct Rate {
    since: Instant,
    remaining: u16,
}
impl Rate {
    fn take(&mut self) -> bool {
        if self.since.elapsed() >= Duration::from_secs(60) {
            self.since = Instant::now();
            self.remaining = 120;
        }
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }
}
#[derive(Clone)]
struct App {
    origin: Arc<str>,
    authority: Arc<str>,
    socket: Arc<PathBuf>,
    uid: u32,
    limit: Arc<Semaphore>,
    rate: Arc<Mutex<Rate>>,
    legacy_compat: bool,
    legacy_rate: Arc<Mutex<Rate>>,
    legacy_limit: Arc<Semaphore>,
    media: Option<Arc<media::Bridge>>,
}
fn socket_safe(path: &Path, uid: u32) -> Result<()> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "core unavailable")?;
    let p = std::fs::symlink_metadata(path.parent().ok_or("invalid socket")?)
        .map_err(|_| "core unavailable")?;
    if !m.file_type().is_socket()
        || m.uid() != uid
        || !p.is_dir()
        || p.uid() != uid
        || p.mode() & 0o022 != 0
    {
        return Err("unsafe core socket");
    }
    Ok(())
}
async fn forward(app: &App, body: &[u8], id: &str) -> Result<Vec<u8>> {
    socket_safe(&app.socket, app.uid)?;
    timeout(Duration::from_secs(15), async {
        let mut stream = UnixStream::connect(app.socket.as_ref())
            .await
            .map_err(|_| "core unavailable")?;
        if stream.peer_cred().map_err(|_| "core unavailable")?.uid() != app.uid {
            return Err("core identity changed");
        }
        stream
            .write_u32(body.len() as u32)
            .await
            .map_err(|_| "core unavailable")?;
        stream
            .write_all(body)
            .await
            .map_err(|_| "core unavailable")?;
        let len = check_len(stream.read_u32().await.map_err(|_| "core unavailable")?)
            .map_err(|_| "invalid core reply")?;
        let mut reply = vec![0; len];
        stream
            .read_exact(&mut reply)
            .await
            .map_err(|_| "core unavailable")?;
        let parsed: CoreResponse =
            serde_json::from_slice(&reply).map_err(|_| "invalid core reply")?;
        if parsed.id != id || parsed.protocol != AUTH_PROTOCOL {
            return Err("invalid core reply");
        }
        Ok(reply)
    })
    .await
    .map_err(|_| "core timeout")?
}
fn denied(status: StatusCode) -> Response {
    (
        status,
        axum::Json(serde_json::json!({"error":"request_denied"})),
    )
        .into_response()
}
async fn api(State(app): State<App>, body: Bytes) -> Response {
    let permit = match app.limit.try_acquire() {
        Ok(v) => v,
        Err(_) => return denied(StatusCode::SERVICE_UNAVAILABLE),
    };
    if !app.rate.lock().is_ok_and(|mut r| r.take()) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let request = match parse_auth_request(&body) {
        Ok(r) => r,
        Err(_) => return denied(StatusCode::BAD_REQUEST),
    };
    // A remote media adapter must prove the actual DTLS peer before issuing a bearer.
    if matches!(
        request.command,
        AuthCommand::IssueMedia { .. }
            | AuthCommand::CheckMediaLease { .. }
            | AuthCommand::CheckMediaPreflight { .. }
    ) {
        return denied(StatusCode::FORBIDDEN);
    }
    let result = forward(&app, &body, &request.id).await;
    drop(permit);
    match result {
        Ok(reply) => ([(header::CONTENT_TYPE, "application/json")], reply).into_response(),
        Err(_) => denied(StatusCode::SERVICE_UNAVAILABLE),
    }
}
async fn boundary(State(app): State<App>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host_ok = headers.get_all(header::HOST).iter().count() == 1
        && headers
            .get(header::HOST)
            .is_some_and(|v| v.as_bytes() == app.authority.as_bytes());
    let api_path = matches!(req.uri().path(), "/api/v2" | "/api/media/v1");
    let media_capabilities = req.uri().path() == "/api/media/capabilities";
    let legacy_path = req.uri().path() == "/v1/session" || req.uri().path() == "/compat/v1/read";
    let origin_ok = headers.get_all(header::ORIGIN).iter().count() == 1
        && headers
            .get(header::ORIGIN)
            .is_some_and(|v| v.as_bytes() == app.origin.as_bytes());
    let content_ok = headers.get_all(header::CONTENT_TYPE).iter().count() == 1
        && headers
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes() == b"application/json");
    let marker_ok = headers
        .get("x-rustsimhub-request")
        .is_some_and(|v| v.as_bytes() == b"2");
    let legacy_ok = app.legacy_compat
        && headers.get_all("x-rsh-legacy-compat").iter().count() == 1
        && headers
            .get("x-rsh-legacy-compat")
            .is_some_and(|v| v.as_bytes() == b"1")
        && (headers.get_all(header::ORIGIN).iter().count() == 0 || origin_ok)
        && content_ok
        && req.uri().query().is_none();
    let mut response = if !host_ok
        || (api_path && (!origin_ok || !content_ok || !marker_ok || req.uri().query().is_some()))
        || (legacy_path && !legacy_ok)
        || (media_capabilities && req.uri().query().is_some())
    {
        denied(StatusCode::FORBIDDEN)
    } else {
        next.run(req).await
    };
    for (name,value) in [
        ("cache-control","no-store"), ("x-content-type-options","nosniff"),
        ("referrer-policy","no-referrer"), ("x-frame-options","DENY"),
        ("content-security-policy","default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"),
        ("permissions-policy","camera=(), microphone=(), geolocation=()"),
    ] { response.headers_mut().insert(name,HeaderValue::from_static(value)); }
    if app.media.is_some() {
        response.headers_mut().insert(
            "permissions-policy",
            HeaderValue::from_static("camera=(), microphone=(self), geolocation=()"),
        );
        response.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; media-src 'self' blob:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"));
    }
    response
}
fn legacy_error(status: StatusCode, code: &str) -> Response {
    (
        status,
        axum::Json(serde_json::json!({"code":code,"retryable":false,"adapter_required":true})),
    )
        .into_response()
}
fn one_header<'a>(headers: &'a axum::http::HeaderMap, name: &str, max: usize) -> Result<&'a str> {
    if headers.get_all(name).iter().count() != 1 {
        return Err("invalid legacy headers");
    }
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= max)
        .ok_or("invalid legacy headers")
}
async fn legacy_forward(
    app: &App,
    headers: &axum::http::HeaderMap,
    command: AuthCommand,
) -> Response {
    if !app.legacy_rate.lock().is_ok_and(|mut r| r.take()) {
        return legacy_error(StatusCode::TOO_MANY_REQUESTS, "legacy_busy");
    }
    let Ok(_permit) = app.legacy_limit.try_acquire() else {
        return legacy_error(StatusCode::SERVICE_UNAVAILABLE, "legacy_busy");
    };
    let id = match one_header(headers, "x-rsh-request-id", 128) {
        Ok(s) => s.to_owned(),
        Err(_) => return legacy_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let request = rustsimhub_wire::AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: id.clone(),
        command,
    };
    let Ok(body) = serde_json::to_vec(&request) else {
        return legacy_error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    if parse_auth_request(&body).is_err() {
        return legacy_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let reply = match forward(app, &body, &id).await {
        Ok(v) => v,
        Err(_) => return legacy_error(StatusCode::SERVICE_UNAVAILABLE, "core_unavailable"),
    };
    let parsed: CoreResponse = match serde_json::from_slice(&reply) {
        Ok(v) => v,
        Err(_) => return legacy_error(StatusCode::SERVICE_UNAVAILABLE, "core_unavailable"),
    };
    if let Some(error) = parsed
        .result
        .get("error")
        .and_then(serde_json::Value::as_str)
    {
        return legacy_error(
            match error {
                "legacy_replay" => StatusCode::CONFLICT,
                "legacy_busy" => StatusCode::TOO_MANY_REQUESTS,
                "legacy_clock_not_ready" => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::UNAUTHORIZED,
            },
            match error {
                "legacy_replay" => "legacy_replay",
                "legacy_busy" => "legacy_busy",
                "legacy_clock_not_ready" => "legacy_clock_not_ready",
                _ => "permission_denied",
            },
        );
    }
    (StatusCode::OK, axum::Json(parsed.result)).into_response()
}
async fn legacy_session(
    State(app): State<App>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    if !app.legacy_compat {
        return legacy_error(StatusCode::NOT_IMPLEMENTED, "legacy_disabled");
    }
    if headers.contains_key("x-nsh-pq-signature") || headers.contains_key("x-nsh-pq-algorithm") {
        return legacy_error(StatusCode::NOT_IMPLEMENTED, "pq_not_supported");
    }
    let fields = (|| -> Result<_> {
        Ok((
            one_header(&headers, "x-nsh-core-id", 128)?.to_owned(),
            one_header(&headers, "x-nsh-device-id", 64)?.to_owned(),
            one_header(&headers, "x-rsh-binding-epoch", 64)?.to_owned(),
            one_header(&headers, "x-nsh-timestamp", 64)?.to_owned(),
            one_header(&headers, "x-nsh-nonce", 128)?.to_owned(),
            one_header(&headers, "x-nsh-signature", 88)?.to_owned(),
            one_header(&headers, "x-rsh-device-proof", 88)?.to_owned(),
        ))
    })();
    let (core_id, device_id, binding_epoch, timestamp, nonce, signature, device_proof) =
        match fields {
            Ok(v) => v,
            Err(_) => return legacy_error(StatusCode::NOT_IMPLEMENTED, "device_binding_required"),
        };
    if body.len() > 8192 {
        return legacy_error(StatusCode::PAYLOAD_TOO_LARGE, "invalid_request");
    }
    legacy_forward(
        &app,
        &headers,
        AuthCommand::LegacySession {
            core_id,
            device_id,
            binding_epoch,
            timestamp,
            nonce,
            signature,
            device_proof,
            body: body.to_vec(),
        },
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyQuery {
    entity: rustsimhub_wire::NasEntity,
    after: Option<rustsimhub_wire::InboxCursor>,
    limit: usize,
    record_id: Option<String>,
}
async fn legacy_read(
    State(app): State<App>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    if !app.legacy_compat {
        return legacy_error(StatusCode::NOT_IMPLEMENTED, "legacy_disabled");
    }
    let token = match one_header(&headers, "authorization", 72)
        .ok()
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        Some(s) if s.len() == 64 => s.to_owned(),
        _ => return legacy_error(StatusCode::UNAUTHORIZED, "permission_denied"),
    };
    let query: LegacyQuery = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return legacy_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    legacy_forward(
        &app,
        &headers,
        AuthCommand::LegacyRead {
            token,
            entity: query.entity,
            after: query.after,
            limit: query.limit,
            record_id: query.record_id,
        },
    )
    .await
}
fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    include_str!("web/index.html"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/app.js"),
                )
            }),
        )
        .route(
            "/indicator.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/indicator.js"),
                )
            }),
        )
        .route(
            "/proof.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/proof.js"),
                )
            }),
        )
        .route(
            "/media.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/media.js"),
                )
            }),
        )
        .route(
            "/call-test.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/call-test.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("web/style.css"),
                )
            }),
        )
        .route("/api/v2", post(api))
        .route("/api/media/v1", post(media::api))
        .route("/api/media/capabilities", get(media::capabilities))
        .route("/v1/session", post(legacy_session))
        .route("/compat/v1/read", post(legacy_read))
        .fallback(|State(app): State<App>, req: Request| async move {
            if app.legacy_compat && req.uri().path().starts_with("/v1/") {
                legacy_error(
                    StatusCode::NOT_IMPLEMENTED,
                    "legacy_operation_not_supported",
                )
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        })
        .layer(DefaultBodyLimit::max(MAX_FRAME))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app)
}
fn create_tls(dir: &Path, hostname: &str) -> Result<String> {
    if hostname.is_empty()
        || hostname.len() > 253
        || hostname
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || b".:-".contains(&b)))
    {
        return Err("invalid TLS name");
    }
    std::fs::create_dir(dir).map_err(|_| "new private TLS directory required")?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "private TLS directory unavailable")?;
    let key = rcgen::generate_simple_self_signed(vec![hostname.to_owned()])
        .map_err(|_| "TLS generation failed")?;
    rustsimhub_auth::write_private_new(dir.join("cert.der").as_path(), key.cert.der())
        .map_err(|_| "TLS write failed")?;
    rustsimhub_auth::write_private_new(dir.join("cert.pem").as_path(), key.cert.pem().as_bytes())
        .map_err(|_| "TLS write failed")?;
    rustsimhub_auth::write_private_new(
        dir.join("key.der").as_path(),
        &key.signing_key.serialize_der(),
    )
    .map_err(|_| "TLS write failed")?;
    let digest = Sha256::digest(key.cert.der());
    let pin = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(pin)
}
fn init_tls(dir: &Path, hostname: &str) -> Result<()> {
    let pin = create_tls(dir, hostname)?;
    println!(
        "{}",
        serde_json::json!({"tls_created":true,"certificate_sha256":pin,"self_signed":true})
    );
    Ok(())
}
fn tls(dir: &Path) -> Result<TlsAcceptor> {
    let cert = rustsimhub_auth::read_private(&dir.join("cert.der"), 8192)
        .map_err(|_| "private TLS certificate unavailable")?;
    let key = rustsimhub_auth::read_private(&dir.join("key.der"), 8192)
        .map_err(|_| "private TLS key unavailable")?;
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert.to_vec())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())),
        )
        .map_err(|_| "TLS configuration failed")?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}
pub fn run(args: Vec<String>) -> Result<()> {
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":rustsimhub_wire::build_source_sha256(),"local_installation_preparation_supported":true,"media_installation_preparation_supported":true})
        );
        return Ok(());
    }
    if let [command, rest @ ..] = args.as_slice() {
        if command == "--prepare-local-media" || command == "--check-local-media" {
            let preparing = command == "--prepare-local-media";
            let required = if preparing { 3 } else { 2 };
            if rest.len() < required || rest.len() > required + 3 {
                return Err("invalid media installation arguments");
            }
            let offset = usize::from(preparing);
            let listen = rest[offset]
                .parse()
                .map_err(|_| "invalid local listen address")?;
            let name = &rest[offset + 1];
            let additional = rest[required..]
                .iter()
                .map(|s| s.parse().map_err(|_| "invalid additional address"))
                .collect::<Result<Vec<std::net::IpAddr>>>()?;
            let result = if preparing {
                installation::prepare_profile(Path::new(&rest[0]), listen, name, Some(&additional))?
            } else {
                installation::check_media(listen, name, &additional)?
            };
            println!("{result}");
            return Ok(());
        }
    }
    if let [command, directory, listen, name] = args.as_slice() {
        if command == "--prepare-local" {
            let listen = listen.parse().map_err(|_| "invalid local listen address")?;
            println!(
                "{}",
                installation::prepare(Path::new(directory), listen, name)?
            );
            return Ok(());
        }
    }
    if let [command, dir, name] = args.as_slice() {
        if command == "--check-local" {
            println!(
                "{}",
                installation::check(
                    dir.parse().map_err(|_| "invalid local listen address")?,
                    name
                )?
            );
            return Ok(());
        }
        if command == "--init-tls" {
            return init_tls(Path::new(dir), name);
        }
    }
    let [command, path] = args.as_slice() else {
        return Err("usage: --init-tls NEW_DIR DNS_NAME | --serve PRIVATE_CONFIG");
    };
    if command != "--serve" {
        return Err("invalid mode");
    }
    let config: Config = serde_json::from_slice(
        &rustsimhub_auth::read_private(Path::new(path), 4096)
            .map_err(|_| "private config required")?,
    )
    .map_err(|_| "invalid config")?;
    let authority = config.authority()?;
    let _media_owner = if config.media_worker_socket.is_some() {
        Some(
            rustsimhub_auth::lock_private_directory(&config.tls_directory)
                .map_err(|_| "media Edge instance already owns TLS directory")?,
        )
    } else {
        None
    };
    let acceptor = tls(&config.tls_directory)?;
    socket_safe(&config.core_socket, config.core_uid)?;
    let mut configured_addresses = vec![config.listen];
    configured_addresses.extend(
        config
            .additional_listen_ips
            .iter()
            .map(|ip| SocketAddr::new(*ip, config.listen.port())),
    );
    let media = config
        .media_worker_socket
        .map(|p| Arc::new(media::Bridge::new(p)));
    let app = App {
        origin: config.origin.into(),
        authority: authority.into(),
        socket: Arc::new(config.core_socket),
        uid: config.core_uid,
        limit: Arc::new(Semaphore::new(8)),
        rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
        legacy_compat: config.legacy_compat,
        legacy_rate: Arc::new(Mutex::new(Rate {
            since: Instant::now(),
            remaining: 120,
        })),
        legacy_limit: Arc::new(Semaphore::new(2)),
        media,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime unavailable")?;
    runtime.block_on(async {
        let accept_one=|listener:TcpListener|async move{let accepted=listener.accept().await;(listener,accepted)};
        let mut acceptors=JoinSet::new();let mut listening=Vec::new();
        for address in &configured_addresses {match TcpListener::bind(address).await {Ok(listener)=>{acceptors.spawn(accept_one(listener));listening.push(*address);},Err(e) if e.kind()==std::io::ErrorKind::AddrNotAvailable=>{},Err(_)=>return Err("listener unavailable"),}}
        if let Some(bridge)=&app.media {bridge.takeover().await?;}
        let mut retry_bind=tokio::time::interval(Duration::from_secs(5));retry_bind.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let slots=Arc::new(Semaphore::new(24)); let mut tasks=JoinSet::new();
        let mut term=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|_|"signal unavailable")?;
        let mut interrupt=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).map_err(|_|"signal unavailable")?;
        let routes=router(app.clone());
        let mut reaper=tokio::time::interval(Duration::from_millis(500));reaper.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        println!("edge ready: https only; bounded connections; core authority required");
        let served=async {
        loop {
            tokio::select! {
                biased;
                _=term.recv()=>break,
                _=interrupt.recv()=>break,
                _=tasks.join_next(),if !tasks.is_empty()=>{},
                _=reaper.tick(),if app.media.is_some()=>{if let Some(bridge)=&app.media {bridge.reap().await;}},
                _=retry_bind.tick()=>{for address in &configured_addresses {if !listening.contains(address) {match TcpListener::bind(address).await {Ok(listener)=>{acceptors.spawn(accept_one(listener));listening.push(*address);},Err(e) if e.kind()==std::io::ErrorKind::AddrNotAvailable=>{},Err(_)=>return Err("additional listener unavailable"),}}}},
                incoming=acceptors.join_next(),if !acceptors.is_empty()=>{
                    let (listener,incoming)=incoming.ok_or("listeners stopped")?.map_err(|_|"listener failed")?;
                    acceptors.spawn(accept_one(listener));
                    let (stream,peer)=incoming.map_err(|_|"listener failed")?;
                    let local=stream.local_addr().map_err(|_|"listener address unavailable")?;
                    let Ok(slot)=slots.clone().try_acquire_owned() else {continue;};
                    let acceptor=acceptor.clone();let service=TowerToHyperService::new(routes.clone().layer(axum::Extension(media::Connection{peer:peer.ip(),local:local.ip()})));
                    tasks.spawn(async move {
                        let _slot=slot;
                        let Ok(Ok(stream))=timeout(Duration::from_secs(3),acceptor.accept(stream)).await else {return;};
                        let mut http=hyper::server::conn::http1::Builder::new();
                        http.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(3)).max_headers(32).max_buf_size(16_384);
                        let _=timeout(Duration::from_secs(90),http.serve_connection(TokioIo::new(stream),service)).await;
                    });
                }
            }
        }
        Ok::<(),&'static str>(())
        }.await;
        // No submitted core mutation is retried. Core durable operations resolve uncertainty.
        acceptors.abort_all();while acceptors.join_next().await.is_some() {}
        tasks.abort_all();while tasks.join_next().await.is_some() {}
        if let Some(bridge)=&app.media {bridge.takeover().await?;}
        served
    })
}

#[cfg(test)]
mod tests;
