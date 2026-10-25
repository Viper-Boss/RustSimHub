// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit finite probe: no persistent switch or policy mutation, no SMS capture.
use super::*;
use std::time::Duration;

pub(super) async fn start(
    controller: &Controller,
    auth: Arc<Mutex<Auth>>,
    token: String,
    generation: u64,
    id: String,
    target: String,
) -> Result<Value> {
    let service = controller.1.clone().ok_or(Error::Forbidden)?;
    let state = controller.0.clone();
    let a = auth.clone();
    let t = token.clone();
    let request_id = id.clone();
    let (response, launch) = tokio::task::spawn_blocking(move || {
        let mut a = a.try_lock().map_err(|_| Error::Busy)?;
        let binding = denied(Binding::from_recent_owner(&mut a, &t))?;
        let mut s = state.try_lock().map_err(|_| Error::Busy)?;
        let reader = s.reader.clone();
        let q = s.existing(binding.clone())?.ok_or(Error::Forbidden)?;
        if let Some(receipt) = denied(q.test_receipt(&binding, &request_id))? {
            if receipt.generation != generation || receipt.target_id != target {
                return Err(Error::Forbidden);
            }
            return Ok((json!({"request_id":request_id,"accepted":true,"synthetic":true,"quick_test":true,"receipt":receipt}), false));
        }
        if s.quick.is_some() || s.worker.is_some() {
            return Err(Error::Busy);
        }
        let q = s.existing(binding)?.ok_or(Error::Forbidden)?;
        let receipt = denied(q.probe_owner(&mut a, &reader, &t, generation, &request_id, &target))?;
        s.quick = Some(request_id.clone());
        s.quick_stop = None;
        Ok((json!({"request_id":request_id,"accepted":true,"synthetic":true,"quick_test":true,"temporary_seconds":45,"receipt":receipt}), true))
    }).await.map_err(|_| Error::Forbidden)??;
    if launch {
        let controller = controller.clone();
        tokio::spawn(async move {
            // This task belongs to Core, not the HTTP connection or browser tab.
            let run = async {
                let value = service
                    .request(
                        &controller,
                        auth.clone(),
                        AuthCommand::ForwardingStatus {
                            token: token.clone(),
                        },
                        service::Action::Reconcile,
                    )
                    .await?;
                if value["state"] != "running" || value["process_state_verified"] != true {
                    return Err(Error::Forbidden);
                }
                for _ in 0..30 {
                    let value = controller
                        .passive_request(
                            auth.clone(),
                            AuthCommand::ForwardingTestRequest {
                                token: token.clone(),
                                request_id: id.clone(),
                            },
                        )
                        .await?;
                    if value["known"] != true {
                        return Err(Error::Forbidden);
                    }
                    if !matches!(
                        value["receipt"]["state"].as_str(),
                        Some("queued" | "in_flight")
                    ) {
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Err(Error::Busy)
            };
            let _ = tokio::time::timeout(Duration::from_secs(30), run).await;
            let state = controller.0.clone();
            let request_id = id.clone();
            let ended = tokio::task::spawn_blocking(move || {
                let mut s = state.lock().map_err(|_| Error::Busy)?;
                if s.quick.as_deref() != Some(request_id.as_str()) {
                    return Ok(false);
                }
                let _ = denied(
                    s.queue
                        .as_mut()
                        .ok_or(Error::Forbidden)?
                        .end_probe(&request_id),
                );
                s.retire();
                Ok::<_, Error>(true)
            })
            .await;
            // Cleanup is an internal fixed reconcile operation. A browser logout
            // cannot stop it; Root still derives the actual current OFF intent.
            let stopped = if matches!(ended, Ok(Ok(true))) {
                service
                    .cleanup_probe(&controller, auth, &id)
                    .await
                    .unwrap_or(false)
            } else {
                false
            };
            let state = controller.0.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(mut s) = state.lock() {
                    if s.quick.as_deref() == Some(id.as_str()) {
                        s.quick = None;
                        s.quick_stop = Some((id, stopped));
                    }
                }
            })
            .await;
        });
    }
    Ok(response)
}
