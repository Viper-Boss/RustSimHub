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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    listen: SocketAddr,
    origin: String,
    core_socket: PathBuf,
    core_uid: u32,
    tls_directory: PathBuf,
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
    if matches!(request.command, AuthCommand::IssueMedia { .. }) {
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
    let api_path = req.uri().path() == "/api/v2";
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
    let mut response = if !host_ok
        || (api_path && (!origin_ok || !content_ok || !marker_ok || req.uri().query().is_some()))
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
    response
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
            "/proof.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("web/proof.js"),
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
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(DefaultBodyLimit::max(MAX_FRAME))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app)
}
fn init_tls(dir: &Path, hostname: &str) -> Result<()> {
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
            serde_json::json!({"source_archive_sha256":rustsimhub_wire::build_source_sha256()})
        );
        return Ok(());
    }
    if let [command, dir, name] = args.as_slice() {
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
    let acceptor = tls(&config.tls_directory)?;
    socket_safe(&config.core_socket, config.core_uid)?;
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
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime unavailable")?;
    runtime.block_on(async {
        let listener=TcpListener::bind(config.listen).await.map_err(|_|"listen unavailable")?;
        let slots=Arc::new(Semaphore::new(24)); let mut tasks=JoinSet::new();
        let mut term=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|_|"signal unavailable")?;
        let mut interrupt=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).map_err(|_|"signal unavailable")?;
        let routes=router(app);
        println!("edge ready: https only; bounded connections; core authority required");
        loop {
            tokio::select! {
                biased;
                _=term.recv()=>break,
                _=interrupt.recv()=>break,
                _=tasks.join_next(),if !tasks.is_empty()=>{},
                incoming=listener.accept()=>{
                    let (stream,_)=incoming.map_err(|_|"listener failed")?;
                    let Ok(slot)=slots.clone().try_acquire_owned() else {continue;};
                    let acceptor=acceptor.clone();let service=TowerToHyperService::new(routes.clone());
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
        // No submitted core mutation is retried. Core durable operations resolve uncertainty.
        tasks.abort_all();while tasks.join_next().await.is_some() {}
        Ok(())
    })
}

#[cfg(test)]
mod tests;
