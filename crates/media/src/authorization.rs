// SPDX-License-Identifier: AGPL-3.0-only
use crate::Error;
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

const TICKET_MS: u64 = 5_000;
const HEARTBEAT_MS: u64 = 1_000;
// Trusted core snapshot, never a client-supplied authorization assertion.
#[derive(Clone, PartialEq, Eq)]
pub struct Context {
    pub boot: String,
    pub line: u64,
    pub call: String,
    pub call_revision: u64,
    pub lease_generation: u64,
    pub principal: String,
    pub principal_generation: u64,
    pub peer_key: [u8; 32],
    pub lease_until_ms: u64,
    pub active: bool,
    pub media_scope: bool,
    pub control_scope: bool,
}
impl Context {
    fn valid(&self, now: u64) -> bool {
        self.boot.len() == 64
            && self.call.len() == 36
            && self.principal.len() == 36
            && self.line > 0
            && self.call_revision > 0
            && self.lease_generation > 0
            && self.principal_generation > 0
            && self.active
            && self.media_scope
            && self.control_scope
            && self.lease_until_ms > now
    }
    fn same_binding(&self, other: &Self) -> bool {
        let mut current = other.clone();
        current.lease_until_ms = self.lease_until_ms;
        self == &current
    }
}
pub struct Ticket([u8; 32]);
// No serialization until the authenticated core/media transport exists.
struct Pending {
    digest: [u8; 32],
    context: Context,
    until: u64,
}
struct State {
    live: AtomicBool,
    until: AtomicU64,
}
pub struct Permit(Arc<State>);
impl Permit {
    pub fn check(&self, now: u64) -> Result<(), Error> {
        if !self.0.live.load(Ordering::Acquire) {
            return Err(Error::Denied);
        }
        if now >= self.0.until.load(Ordering::Acquire) {
            return Err(Error::Expired);
        }
        Ok(())
    }
}
pub struct Authority {
    pending: Option<Pending>,
    session: Option<(Context, Arc<State>)>,
}
impl Default for Authority {
    fn default() -> Self {
        Self::new()
    }
}
impl Authority {
    pub fn new() -> Self {
        Self {
            pending: None,
            session: None,
        }
    }
    pub fn issue(&mut self, context: Context, now: u64) -> Result<Ticket, Error> {
        if !context.valid(now) {
            self.revoke();
            return Err(Error::Denied);
        }
        if self
            .session
            .as_ref()
            .is_some_and(|(_, s)| s.live.load(Ordering::Acquire))
        {
            return Err(Error::Busy);
        }
        if self.pending.as_ref().is_some_and(|p| p.until > now) {
            return Err(Error::Busy);
        }
        let until = now
            .checked_add(TICKET_MS)
            .ok_or(Error::Expired)?
            .min(context.lease_until_ms);
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| Error::Entropy)?;
        self.pending = Some(Pending {
            digest: digest(&nonce),
            context,
            until,
        });
        Ok(Ticket(nonce))
    }
    pub fn consume(
        &mut self,
        ticket: Ticket,
        current: &Context,
        peer_key: &[u8; 32],
        now: u64,
    ) -> Result<Permit, Error> {
        // Any consume attempt burns the pending ticket; failures never permit replay.
        let pending = self.pending.take().ok_or(Error::Denied)?;
        if pending.until <= now
            || pending.digest != digest(&ticket.0)
            || !current.valid(now)
            || !pending.context.same_binding(current)
            || peer_key != &current.peer_key
        {
            return Err(Error::Denied);
        }
        let state = Arc::new(State {
            live: AtomicBool::new(true),
            until: AtomicU64::new(deadline(current, now)?),
        });
        self.session = Some((current.clone(), state.clone()));
        Ok(Permit(state))
    }
    pub fn refresh(&mut self, current: Option<&Context>, now: u64) -> Result<(), Error> {
        let Some((bound, state)) = &mut self.session else {
            return Err(Error::Denied);
        };
        let Some(current) = current else {
            self.revoke();
            return Err(Error::Denied);
        };
        if !current.valid(now)
            || !bound.same_binding(current)
            || !state.live.load(Ordering::Acquire)
            || now >= state.until.load(Ordering::Acquire)
        {
            self.revoke();
            return Err(Error::Denied);
        }
        state
            .until
            .store(deadline(current, now)?, Ordering::Release);
        *bound = current.clone();
        Ok(())
    }
    pub fn revoke(&mut self) {
        self.pending = None;
        if let Some((_, state)) = self.session.take() {
            state.live.store(false, Ordering::Release);
        }
    }
}
impl Drop for Authority {
    fn drop(&mut self) {
        self.revoke();
    }
}
fn deadline(context: &Context, now: u64) -> Result<u64, Error> {
    Ok(now
        .checked_add(HEARTBEAT_MS)
        .ok_or(Error::Expired)?
        .min(context.lease_until_ms))
}
fn digest(nonce: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"RustSimHub-media-ticket-v1");
    h.update(nonce);
    h.finalize().into()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn context() -> Context {
        Context {
            boot: "a".repeat(64),
            line: 1,
            call: "c".repeat(36),
            call_revision: 1,
            lease_generation: 1,
            principal: "p".repeat(36),
            principal_generation: 1,
            peer_key: [7; 32],
            lease_until_ms: 30_000,
            active: true,
            media_scope: true,
            control_scope: true,
        }
    }
    pub fn permit() -> (Authority, Permit) {
        let mut a = Authority::new();
        let c = context();
        let t = a.issue(c.clone(), 0).unwrap();
        let p = a.consume(t, &c, &c.peer_key, 0).unwrap();
        (a, p)
    }
    #[test]
    fn ticket_is_single_use_and_bound_to_peer() {
        let mut a = Authority::new();
        let c = context();
        let t = a.issue(c.clone(), 0).unwrap();
        let replay = Ticket(t.0);
        assert!(a.consume(t, &c, &[8; 32], 0).is_err());
        assert!(a.consume(replay, &c, &c.peer_key, 0).is_err());
        let t = a.issue(c.clone(), 0).unwrap();
        let replay = Ticket(t.0);
        let _p = a.consume(t, &c, &c.peer_key, 0).unwrap();
        assert!(a.consume(replay, &c, &c.peer_key, 0).is_err());
        assert!(a.issue(c, 1).is_err());
    }
    #[test]
    fn expired_ticket_cannot_open_and_heartbeat_cannot_resurrect() {
        let mut a = Authority::new();
        let c = context();
        let t = a.issue(c.clone(), 0).unwrap();
        assert!(a.consume(t, &c, &c.peer_key, 5_000).is_err());
        let (mut a, p) = permit();
        assert_eq!(p.check(1_000), Err(Error::Expired));
        assert!(a.refresh(Some(&c), 1_000).is_err());
        assert!(p.check(1_000).is_err());
    }
    #[test]
    fn every_binding_change_revokes_running_audio() {
        for mutate in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            let (mut a, p) = permit();
            let mut c = context();
            match mutate {
                0 => c.boot = "b".repeat(64),
                1 => c.line += 1,
                2 => c.call = "d".repeat(36),
                3 => c.call_revision += 1,
                4 => c.lease_generation += 1,
                5 => c.principal = "q".repeat(36),
                6 => c.principal_generation += 1,
                7 => c.peer_key = [8; 32],
                8 => c.active = false,
                9 => c.media_scope = false,
                _ => c.control_scope = false,
            }
            assert!(a.refresh(Some(&c), 20).is_err());
            assert!(p.check(20).is_err());
        }
    }
    #[test]
    fn heartbeat_and_lease_deadline_are_server_controlled() {
        let (mut a, p) = permit();
        let mut c = context();
        c.lease_until_ms = 450;
        a.refresh(Some(&c), 20).unwrap();
        assert!(p.check(449).is_ok());
        assert!(p.check(450).is_err());
        assert!(a.refresh(Some(&context()), 450).is_err());
    }
    #[test]
    fn lost_core_authority_stops_audio() {
        let (a, p) = permit();
        drop(a);
        assert!(p.check(0).is_err());
        let (mut a, p) = permit();
        a.refresh(None, 20).unwrap_err();
        assert!(p.check(20).is_err());
    }
}
