// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use axum::body::Body;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_update::{application_files, manifest_hash, signing_bytes, Entry, Manifest, BOARD};
use tokio::net::UnixListener;
use tower::ServiceExt;

fn request(origin: &str, authorization: &str, body: impl Into<Body>) -> Request {
    Request::builder()
        .method("POST")
        .uri(PATH)
        .header(header::HOST, "localhost:8443")
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-rustsimhub-request", "2")
        .header(header::AUTHORIZATION, authorization)
        .body(body.into())
        .unwrap()
}

fn fixture() -> (Vec<u8>, [u8; 32]) {
    let signing = SigningKey::from_bytes(&[11; 32]);
    let public = signing.verifying_key().to_bytes();
    let mut elf = vec![0; 64];
    elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    elf[16] = 3;
    elf[18] = 183;
    elf[52] = 64;
    let names = application_files(2).unwrap();
    let mut payloads = vec![elf; names.len()];
    *payloads.last_mut().unwrap() = b"License fixture".to_vec();
    let files = names
        .iter()
        .zip(&payloads)
        .map(|(name, bytes)| Entry {
            name: name.to_string(),
            bytes: bytes.len() as u64,
            sha256: manifest_hash(bytes),
        })
        .collect();
    let manifest = serde_json::to_vec(&Manifest {
        format: 2,
        kind: "application".into(),
        board: BOARD.into(),
        arch: "aarch64-linux".into(),
        epoch: 120,
        schema_min: 6,
        schema_max: 7,
        source_sha256: "a".repeat(64),
        development_fixtures: false,
        files,
    })
    .unwrap();
    let signature = signing.sign(&signing_bytes(&manifest).unwrap()).to_bytes();
    (
        bundle::pack(&manifest, &signature, &payloads, &public, 6).unwrap(),
        public,
    )
}

async fn core_reply(socket: &UnixListener, token: &str, allowed: bool) {
    let (mut stream, _) = socket.accept().await.unwrap();
    let len = stream.read_u32().await.unwrap() as usize;
    let mut body = vec![0; len];
    stream.read_exact(&mut body).await.unwrap();
    let request = parse_auth_request(&body).unwrap();
    assert!(
        matches!(request.command, AuthCommand::AuthorizeUpdatePreview { token: sent } if sent == token)
    );
    let result = if allowed {
        serde_json::json!({"update_preview_authorized":true})
    } else {
        serde_json::json!({"ok":false,"error":"denied"})
    };
    let bytes = serde_json::to_vec(&CoreResponse {
        protocol: AUTH_PROTOCOL,
        id: request.id,
        result,
    })
    .unwrap();
    stream.write_u32(bytes.len() as u32).await.unwrap();
    stream.write_all(&bytes).await.unwrap();
}

#[tokio::test]
async fn owner_preview_checks_boundary_before_body_and_rechecks_session_after_verify() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("core.sock");
    let socket = UnixListener::bind(&path).unwrap();
    let app = super::super::tests::app(path);
    let origin = "https://localhost:8443";
    let token = "a".repeat(64);
    let header = format!("Bearer {token}");
    for (source, credentials, expected) in [
        (
            "https://foreign.example",
            header.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (origin, "Bearer invalid", StatusCode::UNAUTHORIZED),
    ] {
        let response = router(app.clone())
            .oneshot(request(source, credentials, Body::empty()))
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let (denial, ()) = tokio::join!(
        router(app.clone()).oneshot(request(origin, &header, Body::empty())),
        core_reply(&socket, &token, false),
    );
    assert_eq!(denial.unwrap().status(), StatusCode::FORBIDDEN);
    let (bytes, key) = fixture();
    let isolated = Router::new()
        .route(
            PATH,
            post(move |req| {
                let app = app.clone();
                async move { preview(&app, req, move || Ok(key)).await }
            }),
        )
        .layer(DefaultBodyLimit::max(MAX_FRAME));

    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    for (body, status) in [
        (changed, StatusCode::UNPROCESSABLE_ENTITY),
        (
            vec![0; bundle::MAX_BUNDLE + 1],
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let (result, ()) = tokio::join!(
            isolated.clone().oneshot(request(origin, &header, body)),
            core_reply(&socket, &token, true),
        );
        assert_eq!(result.unwrap().status(), status);
    }
    for final_auth in [false, true] {
        let (result, ()) = tokio::join!(
            isolated
                .clone()
                .oneshot(request(origin, &header, bytes.clone())),
            async {
                core_reply(&socket, &token, true).await;
                core_reply(&socket, &token, final_auth).await;
            },
        );
        let result = result.unwrap();
        if !final_auth {
            assert_eq!(result.status(), StatusCode::FORBIDDEN);
            continue;
        }
        assert_eq!(result.status(), StatusCode::OK);
        let data = axum::body::to_bytes(result.into_body(), 4096)
            .await
            .unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(reply["epoch"], 120);
        assert_eq!(reply["schema_min"], 6);
        assert_eq!(reply["schema_max"], 7);
    }
}
