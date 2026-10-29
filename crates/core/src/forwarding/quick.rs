// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit finite probe: no persistent switch or policy mutation, no SMS capture.
use super::*;
use std::time::Duration;

async fn wait_for_receipt<F, R>(mut observe: F) -> Result<()>
where
    F: FnMut() -> R,
    R: std::future::Future<Output = Result<Value>>,
{
    for _ in 0..30 {
        match observe().await {
            // Admission was refused before any mutation. A competing read
            // must not retire a worker while its HTTPS reply is outstanding.
            // Only this passive read is retried, within the original budget.
            Err(Error::Busy) => {}
            Err(error) => return Err(error),
            Ok(value) => {
                if value["known"] != true {
                    return Err(Error::Forbidden);
                }
                if !matches!(
                    value["receipt"]["state"].as_str(),
                    Some("queued" | "in_flight")
                ) {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(Error::Busy)
}

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
                // `request` refuses with Busy before any dispatch or Root call when a page's
                // poll holds the Auth/state lock at that instant; a bounded retry keeps such a
                // collision from silently skipping the start (backlog #24).
                let mut attempt = 0;
                let value = loop {
                    match service
                        .request(
                            &controller,
                            auth.clone(),
                            AuthCommand::ForwardingStatus {
                                token: token.clone(),
                            },
                            service::Action::Reconcile,
                        )
                        .await
                    {
                        Err(Error::Busy) if attempt < 5 => {
                            attempt += 1;
                            tokio::time::sleep(Duration::from_millis(20 << attempt)).await;
                        }
                        result => break result?,
                    }
                };
                if value["state"] != "running" || value["process_state_verified"] != true {
                    return Err(Error::Forbidden);
                }
                wait_for_receipt(|| {
                    controller.passive_request(
                        auth.clone(),
                        AuthCommand::ForwardingTestRequest {
                            token: token.clone(),
                            request_id: id.clone(),
                        },
                    )
                })
                .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forwarding::tests::{Fixture, REQUEST};

    #[tokio::test]
    async fn receipt_lock_contention_does_not_cancel_an_in_flight_probe() {
        let f = Fixture::new();
        let token = match f.configure(0, REQUEST, false) {
            AuthCommand::ForwardingConfigure { token, .. } => token,
            _ => unreachable!(),
        };
        // Exercise the real nonblocking Auth mutex, not an invented error.
        let auth = f.auth.clone();
        let (ready, held) = std::sync::mpsc::sync_channel(0);
        let (release, done) = std::sync::mpsc::sync_channel(0);
        let holder = std::thread::spawn(move || {
            let _guard = auth.lock().unwrap();
            ready.send(()).unwrap();
            done.recv().unwrap();
        });
        held.recv().unwrap();
        let busy = f
            .control
            .passive_request(
                f.auth.clone(),
                AuthCommand::ForwardingTestRequest {
                    token,
                    request_id: REQUEST.into(),
                },
            )
            .await;
        release.send(()).unwrap();
        holder.join().unwrap();
        assert!(matches!(busy, Err(Error::Busy)));
        let mut observations = [
            busy,
            Ok(json!({"known":true,"receipt":{"state":"in_flight"}})),
            Ok(json!({"known":true,"receipt":{"state":"confirmed"}})),
        ]
        .into_iter();
        wait_for_receipt(|| std::future::ready(observations.next().unwrap()))
            .await
            .unwrap();
        assert!(observations.next().is_none());
    }

    #[tokio::test]
    async fn receipt_denial_and_missing_receipt_still_end_the_probe_immediately() {
        for value in [Err(Error::Forbidden), Ok(json!({"known":false}))] {
            let mut value = Some(value);
            assert!(matches!(
                wait_for_receipt(|| std::future::ready(value.take().unwrap())).await,
                Err(Error::Forbidden)
            ));
            assert!(value.is_none());
        }
    }
}
