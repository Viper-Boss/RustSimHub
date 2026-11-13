// SPDX-License-Identifier: AGPL-3.0-only
//! Volatile Edge-only PCM capabilities. Pairing secrets/bearers stay in Core;
//! media admission still goes through the ordinary active-call controller.
use super::*;
use std::time::Instant;

const LIFETIME: Duration = Duration::from_secs(2);
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
                return Err(Error::Busy);
            }
            let pairing = inner
                .gateway
                .authenticate_pcm(
                    &preface,
                    peer.parse().map_err(|_| Error::InvalidInput)?,
                    now_ms().div_euclid(1000),
                )
                .ok_or(Error::Forbidden)?;
            let Some((owner, lease, _)) = inner.call_book.lease.clone() else {
                return Err(Error::Forbidden);
            };
            if owner != pairing {
                return Err(Error::Forbidden);
            }
            let mut core = context(controller, inner, auth, runtime);
            let token = core.token().map_err(|_| Error::Forbidden)?;
            observed_state(&mut core, &token, &lease.call_id)?;
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
                .ok_or(Error::Forbidden)?;
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
                return Err(Error::Forbidden);
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
                return Err(Error::Forbidden);
            }
            let mut core = context(controller, inner, auth, runtime);
            // Audio traffic is proof of this phone's continued presence. Renew
            // only its own call, then re-run the active/media/scopes check.
            core.keep(&pairing, std::slice::from_ref(&call), now_ms());
            let token = core.token().map_err(|_| Error::Forbidden)?;
            let state = observed_state(&mut core, &token, &call)?;
            if state != 4 {
                if issued || Instant::now() >= activate_until {
                    inner.pcm = None;
                    return Err(Error::Forbidden);
                }
                inner.pcm.as_mut().ok_or(Error::Forbidden)?.until = Instant::now() + LIFETIME;
                return Ok(json!({"active": false, "waiting": true}));
            }
            let lease = core
                .book
                .lease
                .as_ref()
                .map(|(_, l, _)| l.clone())
                .ok_or(Error::Forbidden)?;
            let checked = core.call(AuthCommand::CheckMediaLease {
                token: token.to_string(),
                lease: lease.clone(),
            });
            if !checked.is_ok_and(|v| v["media_lease_valid"] == true) {
                inner.pcm = None;
                return Err(Error::Forbidden);
            }
            let credentials = if !issued {
                let peer_key = random::<32>()?;
                let value = core
                    .call(AuthCommand::IssueMedia {
                        token: token.to_string(),
                        lease,
                        peer_key,
                    })
                    .map_err(|_| Error::Forbidden)?;
                let credentials: rustsimhub_media::protocol::Credentials =
                    serde_json::from_value(value).map_err(|_| Error::Forbidden)?;
                if credentials.peer_key != peer_key {
                    return Err(Error::Forbidden);
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
