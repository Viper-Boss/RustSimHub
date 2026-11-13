// SPDX-License-Identifier: AGPL-3.0-only
//! Volatile Edge-only PCM capabilities. Pairing secrets/bearers stay in Core;
//! media admission still goes through the ordinary active-call controller.
use super::*;
use std::time::Instant;

const LIFETIME: Duration = Duration::from_secs(2);

/// Fixed local diagnostics only; no phone/lease/key/preface content is logged.
fn denied(stage: &'static str) -> Error {
    eprintln!("airsim_pcm_denied stage={stage}");
    Error::Forbidden
}
pub(super) struct State {
    ticket: String,
    pairing: String,
    call: String,
    rate: u32,
    until: Instant,
    session: String,
    issued: bool,
    activate_until: Instant,
}
#[cfg(test)]
impl State {
    pub(super) fn expire_for_test(&mut self) {
        self.until = Instant::now() - Duration::from_millis(1);
    }
    pub(super) fn expire_activation_for_test(&mut self) {
        self.activate_until = Instant::now() - Duration::from_millis(1);
    }
}

fn context<'a>(
    controller: &'a Controller,
    inner: &'a mut Inner,
    auth: &'a Arc<Mutex<Auth>>,
    runtime: &'a tokio::runtime::Handle,
) -> Core<'a> {
    Core {
        auth,
        seed: inner.seed.as_ref(),
        session: &mut inner.session,
        baselines: &inner.baselines,
        sender: None,
        calls: controller.calls.as_deref(),
        book: &mut inner.call_book,
        runtime,
    }
}

pub(super) fn process(
    controller: &Controller,
    inner: &mut Inner,
    auth: &Arc<Mutex<Auth>>,
    runtime: &tokio::runtime::Handle,
    command: AuthCommand,
) -> Result<Value, Error> {
    if inner
        .pcm
        .as_ref()
        .is_some_and(|s| Instant::now() >= s.until)
    {
        inner.pcm = None;
    }
    match command {
        AuthCommand::AirsimPcmOpen { preface, peer } => {
            if inner.pcm.is_some() {
                eprintln!("airsim_pcm_denied stage=occupied");
                return Err(Error::Busy);
            }
            let pairing = inner
                .gateway
                .authenticate_pcm(
                    &preface,
                    peer.parse().map_err(|_| Error::InvalidInput)?,
                    now_ms().div_euclid(1000),
                )
                .ok_or_else(|| denied("authentication"))?;
            let Some((owner, lease, _)) = inner.call_book.lease.clone() else {
                return Err(denied("no_owned_call"));
            };
            if owner != pairing {
                return Err(denied("foreign_owner"));
            }
            let mut core = context(controller, inner, auth, runtime);
            let token = core.token().map_err(|_| denied("gateway_authority"))?;
            observed_state(&mut core, &token, &lease.call_id)
                .map_err(|_| denied("call_observation"))?;
            let ticket = hex(&random::<16>()?);
            let session = hex(&random::<16>()?);
            let rate = inner.pcm_rate;
            inner.pcm = Some(State {
                ticket: ticket.clone(),
                pairing,
                call: lease.call_id,
                rate,
                until: Instant::now() + LIFETIME,
                session: session.clone(),
                issued: false,
                activate_until: Instant::now() + Duration::from_secs(90),
            });
            Ok(json!({"ticket": ticket, "session": session, "pcm_rate": rate, "waiting": true}))
        }
        AuthCommand::AirsimPcmPulse { ticket, close } => {
            let state = inner
                .pcm
                .as_ref()
                .filter(|s| s.ticket == ticket)
                .ok_or_else(|| denied("link_ticket"))?;
            if close {
                inner.pcm = None;
                return Ok(json!({"active": false}));
            }
            let pairing = state.pairing.clone();
            let call = state.call.clone();
            let issued = state.issued;
            let activate_until = state.activate_until;
            if !issued && Instant::now() >= activate_until {
                inner.pcm = None;
                return Err(denied("activation_deadline"));
            }
            let valid = inner.enabled
                && inner.pcm_rate == state.rate
                && inner.gateway.pairings().iter().any(|p| p.id == pairing);
            let lease = inner
                .call_book
                .lease
                .as_ref()
                .filter(|(p, l, _)| *p == pairing && l.call_id == call)
                .map(|(_, l, _)| l.clone());
            if !valid || lease.is_none() {
                inner.pcm = None;
                return Err(denied("pairing_or_call_lost"));
            }
            let mut core = context(controller, inner, auth, runtime);
            // Audio traffic is proof of this phone's continued presence. Renew
            // only its own call, then re-run the active/media/scopes check.
            core.keep(&pairing, std::slice::from_ref(&call), now_ms());
            let token = core.token().map_err(|_| denied("gateway_authority"))?;
            if !issued {
                let state = observed_state(&mut core, &token, &call)
                    .map_err(|_| denied("call_observation"))?;
                if state != 4 {
                    if Instant::now() >= activate_until {
                        inner.pcm = None;
                        return Err(denied("activation_deadline"));
                    }
                    inner.pcm.as_mut().ok_or(Error::Forbidden)?.until = Instant::now() + LIFETIME;
                    return Ok(json!({"active": false, "waiting": true}));
                }
            }
            let lease = core
                .book
                .lease
                .as_ref()
                .map(|(_, l, _)| l.clone())
                .ok_or_else(|| denied("lease_lost"))?;
            // Once activated, this admission reads the selected MM call and
            // validates ACTIVE, owner, lease, binding and current scopes itself.
            // A second CallStatus would rescan every historical call at 4 Hz.
            let checked = core.call(AuthCommand::CheckMediaLease {
                token: token.to_string(),
                lease: lease.clone(),
            });
            if !checked.is_ok_and(|v| v["media_lease_valid"] == true) {
                inner.pcm = None;
                return Err(denied("media_lease"));
            }
            let credentials = if !issued {
                let peer_key = random::<32>()?;
                let value = core
                    .call(AuthCommand::IssueMedia {
                        token: token.to_string(),
                        lease,
                        peer_key,
                    })
                    .map_err(|_| denied("media_grant"))?;
                let credentials: rustsimhub_media::protocol::Credentials =
                    serde_json::from_value(value).map_err(|_| denied("credentials_shape"))?;
                if credentials.peer_key != peer_key {
                    return Err(denied("peer_binding"));
                }
                Some(credentials)
            } else {
                None
            };
            let muted = inner.gateway.muted(&pairing);
            let slot = inner.pcm.as_mut().ok_or(Error::Forbidden)?;
            slot.until = Instant::now() + LIFETIME;
            slot.issued = true;
            Ok(
                json!({"active": true, "muted": muted, "credentials":credentials,"session":slot.session,"pcm_rate":slot.rate}),
            )
        }
        _ => Err(Error::Forbidden),
    }
}

fn observed_state(core: &mut Core<'_>, token: &str, call: &str) -> Result<i64, Error> {
    let status = core
        .call(AuthCommand::CallStatus {
            token: token.into(),
        })
        .map_err(|_| Error::Forbidden)?;
    if status["available"] != true {
        return Err(Error::Forbidden);
    }
    status["live_calls"]
        .as_array()
        .and_then(|calls| {
            calls
                .iter()
                .find(|c| c["id"] == call && c["availability"] == "observed")
        })
        .and_then(|c| c["modem_state"].as_i64())
        .filter(|s| matches!(s, 1..=4))
        .ok_or(Error::Forbidden)
}
