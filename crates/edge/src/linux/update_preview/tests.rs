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
    let submit_app = app.clone();
    let isolated = Router::new()
        .route(
            SUBMIT,
            post(move |req| {
                let app = submit_app.clone();
                async move { submit_inner(&app, req, move || Ok(key)).await }
            }),
        )
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
    let manifest = bundle::preview(&bytes, &key).unwrap().manifest_sha256;
    let request_id = "aabbccdd-0123-4123-a123-123456789abc";
    let make_submit = |manifest: &str| {
        let mut req = request(origin, &header, bytes.clone());
        *req.uri_mut() = SUBMIT.parse().unwrap();
        req.headers_mut()
            .insert("x-rustsimhub-update-request", request_id.parse().unwrap());
        req.headers_mut()
            .insert("x-rustsimhub-update-manifest", manifest.parse().unwrap());
        req
    };
    let reply = |can_submit| {
        serde_json::json!({
            "available": true,
            "can_submit": can_submit,
            "can_delete_backup": false,
            "receipt": null
        })
    };
    let exchange = |response: serde_json::Value, expected_body: bool| {
        let token = token.clone();
        let manifest = manifest.clone();
        let bytes = bytes.clone();
        let socket = &socket;
        async move {
            let (mut stream, _) = socket.accept().await.unwrap();
            let len = stream.read_u32().await.unwrap() as usize;
            let mut frame = vec![0; len];
            stream.read_exact(&mut frame).await.unwrap();
            let request = parse_auth_request(&frame).unwrap();
            if expected_body {
                let AuthCommand::SubmitUpdateUpload {
                    token: received,
                    upload,
                } = request.command
                else {
                    panic!("submit command")
                };
                assert_eq!(received, token);
                assert_eq!(upload.manifest_sha256, manifest);
                assert_eq!(upload.request_id, request_id);
                assert_eq!(upload.bundle_sha256, manifest_hash(&bytes));
                assert_eq!(upload.epoch, 120);
                let mut delivered = Vec::new();
                stream.read_to_end(&mut delivered).await.unwrap();
                assert_eq!(delivered, bytes);
            } else {
                assert!(
                    matches!(request.command, AuthCommand::UpdateUploadStatus {token:received} if received==token)
                );
            }
            let answer = serde_json::to_vec(&CoreResponse {
                protocol: AUTH_PROTOCOL,
                id: request.id,
                result: response,
            })
            .unwrap();
            stream.write_u32(answer.len() as u32).await.unwrap();
            stream.write_all(&answer).await.unwrap();
        }
    };
    let (result, ()) = tokio::join!(
        isolated.clone().oneshot(make_submit(&manifest)),
        exchange(reply(false), false),
    );
    assert_eq!(result.unwrap().status(), StatusCode::FORBIDDEN);
    let (result, ()) = tokio::join!(
        isolated.clone().oneshot(make_submit(&"b".repeat(64))),
        exchange(reply(true), false),
    );
    assert_eq!(result.unwrap().status(), StatusCode::CONFLICT);
    let (result, ()) = tokio::join!(isolated.oneshot(make_submit(&manifest)), async {
        exchange(
            serde_json::json!({"available":true,"can_submit":true,
                "receipt":{"request_id":"aabbccdd-0123-4123-a123-123456789abb",
                    "epoch":119,"phase":"restored"}}),
            false,
        )
        .await;
        exchange(
            serde_json::json!({"state":"received","request_id":request_id,
                "manifest_sha256":manifest,"epoch":120}),
            true,
        )
        .await;
    },);
    assert_eq!(result.unwrap().status(), StatusCode::ACCEPTED);
}
