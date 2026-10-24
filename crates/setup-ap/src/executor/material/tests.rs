// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
};
use tokio::net::TcpListener;
use tokio_rustls::{
    rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    TlsAcceptor,
};
fn fixture(
    name: &str,
) -> (
    tempfile::TempDir,
    Material,
    rcgen::CertifiedKey<rcgen::KeyPair>,
) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec![name.to_owned()]).unwrap();
    let config = serde_json::json!({"format":1,"realm":"00000000-0000-4000-8000-000000000002","core_uid":65534,"device_public_key":([7u8;32]),"ssid":"RustSimHub-Setup-fixture","passphrase":"a".repeat(48),"uuid":"00000000-0000-4000-8000-000000000003"});
    for (name, bytes) in [
        ("config.json", serde_json::to_vec(&config).unwrap()),
        ("cert.der", cert.cert.der().to_vec()),
    ] {
        fs::write(d.path().join(name), bytes).unwrap();
        fs::set_permissions(d.path().join(name), fs::Permissions::from_mode(0o600)).unwrap();
    }
    let meta = fs::metadata(d.path()).unwrap();
    let m =
        Material::open(Directory::open(d.path(), meta.uid(), meta.gid(), 0o700).unwrap()).unwrap();
    (d, m, cert)
}
async fn server(
    cert: rcgen::CertifiedKey<rcgen::KeyPair>,
    mode: &'static str,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der())),
        )
        .unwrap();
    let task = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let Ok(mut tls) = TlsAcceptor::from(Arc::new(cfg)).accept(tcp).await else {
            return;
        };
        let mut bytes = Vec::new();
        let body = loop {
            let mut headers = [httparse::EMPTY_HEADER; 24];
            let mut r = httparse::Request::new(&mut headers);
            if let httparse::Status::Complete(size) = r.parse(&bytes).unwrap() {
                assert_eq!(r.method, Some("POST"));
                assert_eq!(r.path, Some("/api/v2"));
                let header = |n: &str| {
                    r.headers
                        .iter()
                        .find(|h| h.name.eq_ignore_ascii_case(n))
                        .unwrap()
                        .value
                };
                assert_eq!(header("host"), b"172.31.85.1:8443");
                assert_eq!(header("origin"), b"https://172.31.85.1:8443");
                assert_eq!(header("x-rustsimhub-request"), b"2");
                let length = std::str::from_utf8(header("content-length"))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= size + length {
                    break bytes[size..size + length].to_vec();
                }
            }
            let mut chunk = [0u8; 1024];
            let n = tls.read(&mut chunk).await.unwrap();
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 8192);
        };
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(request["command"]["op"], "identity");
        let key = if mode == "identity" {
            [8u8; 32]
        } else {
            [7u8; 32]
        };
        let mut reply = serde_json::json!({"protocol":2,"id":request["id"],"result":{"public_key":key,"signature_algorithm":"ed25519","protocol":2}});
        if mode == "id" {
            reply["id"] = "stale-probe".into();
        }
        if mode == "error" {
            reply["result"]["error"] = "denied".into();
        }
        let data = serde_json::to_vec(&reply).unwrap();
        let status = if mode == "origin" {
            "403 Forbidden"
        } else {
            "200 OK"
        };
        let header=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len());
        let _ = tls.write_all(header.as_bytes()).await;
        let _ = tls.write_all(&data).await;
        let _ = tls.shutdown().await;
    });
    (address, task)
}
#[tokio::test]
async fn real_tls_ap_san_pin_origin_and_fresh_identity_are_required() {
    for mode in ["good", "identity", "id", "error", "origin", "san", "pin"] {
        let (_d, mut m, cert) = fixture(if mode == "san" {
            "wrong.example"
        } else {
            crate::ADDRESS
        });
        let cert = if mode == "pin" {
            rcgen::generate_simple_self_signed(vec![crate::ADDRESS.into()]).unwrap()
        } else {
            cert
        };
        let (addr, task) = server(cert, mode).await;
        m.endpoint = addr;
        let result = timeout(Duration::from_secs(3), m.check()).await.unwrap();
        assert_eq!(result.is_ok(), mode == "good", "{mode}");
        task.await.unwrap();
    }
}
#[tokio::test]
async fn material_changes_invalidate_even_a_recent_successful_tls_probe() {
    let (d, mut m, cert) = fixture(crate::ADDRESS);
    let (addr, task) = server(cert, "good").await;
    m.endpoint = addr;
    m.check().await.unwrap();
    task.await.unwrap();
    m.check().await.unwrap(); // bounded cache, no second connection
    let path = d.path().join("config.json");
    let mut c: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    c["passphrase"] = "b".repeat(48).into();
    fs::write(&path, serde_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(m.check().await, Err(Error::Changed));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(m.check().await.is_err());
}
#[test]
fn readiness_http_rejects_ambiguous_truncated_or_error_responses() {
    assert_eq!(
        response(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}"
        )
        .unwrap(),
        Some(b"{}".as_slice())
    );
    assert!(response(
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{"
    )
    .unwrap()
    .is_none());
    for input in [
        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}extra",
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}",
    ] {assert!(response(input.as_bytes()).is_err());}
}
