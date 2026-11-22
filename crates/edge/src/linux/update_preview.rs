// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use axum::{
    body::{Body, HttpBody},
    http::HeaderMap,
};
use rustsimhub_update::{bundle, store::read_root_release_key};
use std::{future::poll_fn, pin::Pin};

pub(super) const PATH: &str = "/api/update/v1/preview";
static UPLOAD: Semaphore = Semaphore::const_new(1);

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let token = one_header(headers, "authorization", 71)
        .ok()?
        .strip_prefix("Bearer ")?;
    (token.len() == 64
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(token)
}

async fn authorize(app: &App, token: &str) -> std::result::Result<(), StatusCode> {
    let _permit = app
        .limit
        .try_acquire()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let request = rustsimhub_wire::AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: "update-preview".to_owned(),
        command: AuthCommand::AuthorizeUpdatePreview {
            token: token.into(),
        },
    };
    let body = serde_json::to_vec(&request).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let reply = forward(app, &body, &request.id)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let reply: CoreResponse =
        serde_json::from_slice(&reply).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if reply.result == serde_json::json!({"update_preview_authorized":true}) {
        Ok(())
    } else if reply.result["error"] == "busy" {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub(super) async fn api(State(app): State<App>, request: Request) -> Response {
    preview(&app, request, read_root_release_key).await
}

async fn read_upload(mut body: Body) -> std::result::Result<Vec<u8>, StatusCode> {
    let size = body.size_hint();
    if size.lower() > bundle::MAX_BUNDLE as u64 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    // One buffer avoids retaining a body of chunks alongside a coalesced copy.
    let mut bytes = Vec::with_capacity(size.exact().unwrap_or(bundle::MAX_BUNDLE as u64) as usize);
    while let Some(frame) = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
        let chunk = frame
            .map_err(|_| StatusCode::BAD_REQUEST)?
            .into_data()
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if chunk.len() > bundle::MAX_BUNDLE - bytes.len() {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn preview(
    app: &App,
    request: Request,
    trust: impl FnOnce() -> std::result::Result<[u8; 32], rustsimhub_update::Error> + Send + 'static,
) -> Response {
    let (parts, body) = request.into_parts();
    let Some(token) = bearer(&parts.headers) else {
        return denied(StatusCode::UNAUTHORIZED);
    };
    if !app.rate.lock().is_ok_and(|mut rate| rate.take()) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let Ok(permit) = UPLOAD.try_acquire() else {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    };
    if let Err(status) = authorize(app, token).await {
        return denied(status);
    }
    let body = match timeout(Duration::from_secs(45), read_upload(body)).await {
        Ok(Ok(body)) => body,
        Ok(Err(status)) => return denied(status),
        Err(_) => return denied(StatusCode::REQUEST_TIMEOUT),
    };
    let verified = tokio::task::spawn_blocking(move || {
        // Hold the permit even if the client closes during verification.
        let _permit = permit;
        trust().map(|key| bundle::preview(&body, &key))
    })
    .await;
    match verified {
        Ok(Ok(Ok(value))) => match authorize(app, token).await {
            Ok(()) => axum::Json(value).into_response(),
            Err(status) => denied(status),
        },
        Ok(Ok(Err(_))) => denied(StatusCode::UNPROCESSABLE_ENTITY),
        _ => denied(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[cfg(test)]
mod tests;
