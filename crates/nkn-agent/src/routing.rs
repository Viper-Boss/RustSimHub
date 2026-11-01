// SPDX-License-Identifier: AGPL-3.0-only
//! A-26 in-process routing policy candidate; no wire format or crypto suite.
//!
//! Only trusted, reviewed adapters may implement the ports below. There is no
//! production adapter in this crate. Membership alone MUST NOT admit a session.
//! A Binding is an opaque full Core/identity/channel binding, never a peer's
//! assertion, address, PID or an "authenticated" JSON field. See
//! docs/nkn/member-a-routing-policy.md for the producer and execution contracts.
use crate::{Clock, SystemClock};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Conservative local caps, pending protocol review; not a new wire contract.
pub const MAX_RECORD_BYTES: usize = 2048;
pub const MAX_BODY_BYTES: usize = 1024;
pub const MAX_REQUESTS: u64 = 128;
const CALL_BUDGET: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    CoreAuthV2,
    CallMediaV1,
}
impl Service {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "core-auth-v2" => Some(Self::CoreAuthV2),
            "call-media-v1" => Some(Self::CallMediaV1),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    NknRemote,
    BleRecovery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    PeerToDevice,
    DeviceToPeer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Closed,
    Busy,
    Denied,
    Unavailable,
    InvalidRecord,
    Expired,
    Revoked,
    ClockRollback,
    Exhausted,
    /// The Core may have executed. Preserve its original operation UUID and
    /// query there; this router must never resubmit or manufacture success.
    OutcomeUnknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortError {
    Denied,
    Unavailable,
    InvalidRecord,
}
impl From<PortError> for Error {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied => Self::Denied,
            PortError::Unavailable => Self::Unavailable,
            PortError::InvalidRecord => Self::InvalidRecord,
        }
    }
}

/// A local, irreversible cancellation handle, NOT an authentication proof.
#[derive(Clone)]
pub struct Revocation(Arc<AtomicBool>);
impl Revocation {
    pub fn revoke(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_revoked(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
pub struct CallBudget {
    deadline: Instant,
    revocation: Revocation,
}
impl CallBudget {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn cancelled(&self) -> bool {
        self.revocation.is_revoked()
    }
}

/// Trusted producer metadata. No serialization, Debug, or authorization flag.
/// Binding equality MUST include the complete Core instance/connection nonce,
/// device pin, independent transport/session identity, registered client,
/// realm, Owner/client revocation generations and current line/SIM context.
pub struct SessionFacts<B> {
    pub binding: B,
    pub purpose: Purpose,
    pub expires_at: Instant,
}
/// Output of real authenticated decryption, not a parser for untrusted JSON.
/// Every field below, including service/direction/sequence, must be covered by
/// the reviewed record authentication. Never use a plaintext fallback.
pub struct VerifiedRecord<B> {
    pub binding: B,
    pub version: u16,
    pub purpose: Purpose,
    pub direction: Direction,
    pub sequence: u64,
    pub service: String,
    pub body: Vec<u8>,
}
pub trait TransportPort {
    type Admission;
    type Session;
    type Binding: Eq;

    /// Validate real membership AND device pin, independent transport keys,
    /// registered Core identity and purpose. Own all partial cleanup on error.
    fn admit(
        &mut self,
        admission: Self::Admission,
        budget: &CallBudget,
    ) -> Result<(Self::Session, SessionFacts<Self::Binding>), PortError>;
    /// Fresh live authority/parent connection check, not a cached peer claim.
    fn inspect(
        &mut self,
        session: &mut Self::Session,
        budget: &CallBudget,
    ) -> Result<SessionFacts<Self::Binding>, PortError>;
    fn open_record(
        &mut self,
        session: &mut Self::Session,
        wire: &[u8],
        budget: &CallBudget,
    ) -> Result<VerifiedRecord<Self::Binding>, PortError>;
    /// Authenticated response with the original binding, service, device->peer
    /// direction and its own sequence. No sending/detached output after return.
    fn seal_reply(
        &mut self,
        session: &mut Self::Session,
        service: Service,
        sequence: u64,
        body: &[u8],
        budget: &CallBudget,
    ) -> Result<Vec<u8>, PortError>;
    /// Infallible local retirement, no network wait/reconnect/business action.
    /// Drop/close is not a proof of remote or Root resource cleanup.
    fn retire(&mut self, session: Self::Session);
}
pub enum BusinessOutcome {
    /// Original Core/media result bytes, not a new router success receipt.
    Reply(Vec<u8>),
    /// These two variants certify NO execution was admitted.
    Denied,
    Unavailable,
    Unknown,
}
pub trait BusinessPort<B> {
    /// Original AuthRequest parser/dispatcher, same request UUID, current Auth
    /// and scope gates at execution and commit. No raw AuthCommand bypass.
    /// Enforce the SAME absolute budget/cancellation at the actual side effect;
    /// no detached work, retries, second queue or caller-created Principal.
    fn core_auth(&mut self, binding: &B, body: &[u8], budget: &CallBudget) -> BusinessOutcome;
    /// Original media Permit/Ticket and peer/call/line/revocation gates. Only
    /// bounded signaling; never an arbitrary TCP/UDP/LAN byte proxy.
    fn call_media(&mut self, binding: &B, body: &[u8], budget: &CallBudget) -> BusinessOutcome;
}

struct Active<T: TransportPort> {
    session: T::Session,
    facts: SessionFacts<T::Binding>,
    revocation: Revocation,
    received: u64,
    sent: u64,
}
/// One session, one synchronous call; &mut self prevents overlapping dispatch.
/// Ports are owned and never exposed for replacement while a session is live.
pub struct Router<T: TransportPort, B: BusinessPort<T::Binding>, C: Clock> {
    transport: T,
    business: B,
    clock: C,
    last_now: Option<Instant>,
    active: Option<Active<T>>,
}
impl<T: TransportPort, B: BusinessPort<T::Binding>, C: Clock> Router<T, B, C> {
    pub fn new(transport: T, business: B, clock: C) -> Self {
        Self {
            transport,
            business,
            clock,
            last_now: None,
            active: None,
        }
    }
    pub fn is_open(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| !a.revocation.is_revoked())
    }
    fn now(&mut self) -> Result<Instant, Error> {
        let now = self.clock.now();
        if self.last_now.is_some_and(|previous| now < previous) {
            return Err(Error::ClockRollback);
        }
        self.last_now = Some(now);
        Ok(now)
    }
    pub fn open(&mut self, admission: T::Admission) -> Result<Revocation, Error> {
        if self.active.is_some() {
            return Err(Error::Busy);
        }
        let now = self.now()?;
        let deadline = now.checked_add(CALL_BUDGET).ok_or(Error::Expired)?;
        let revocation = Revocation(Arc::new(AtomicBool::new(false)));
        let mut budget = CallBudget {
            deadline,
            revocation: revocation.clone(),
        };
        let (session, facts) = self.transport.admit(admission, &budget)?;
        self.active = Some(Active {
            session,
            facts,
            revocation: revocation.clone(),
            received: 0,
            sent: 0,
        });
        if let Err(error) = self.check_live(&mut budget) {
            self.close();
            return Err(error);
        }
        Ok(revocation)
    }
    fn check_time(&mut self, budget: &CallBudget) -> Result<(), Error> {
        let now = self.now()?;
        let active = self.active.as_ref().ok_or(Error::Closed)?;
        if budget.cancelled() {
            return Err(Error::Revoked);
        }
        if now >= budget.deadline || now >= active.facts.expires_at {
            return Err(Error::Expired);
        }
        Ok(())
    }
    fn check_live(&mut self, budget: &mut CallBudget) -> Result<(), Error> {
        self.check_time(budget)?;
        let active = self.active.as_mut().ok_or(Error::Closed)?;
        // Admission may return authority shorter than the admission budget.
        // Even the first read-only inspection must use that shorter deadline.
        budget.deadline = budget.deadline.min(active.facts.expires_at);
        let current = self.transport.inspect(&mut active.session, budget)?;
        if current.purpose != Purpose::NknRemote
            || active.facts.purpose != Purpose::NknRemote
            || current.binding != active.facts.binding
        {
            return Err(Error::Denied);
        }
        // A provider refresh can shorten authority, never extend this session.
        active.facts.expires_at = active.facts.expires_at.min(current.expires_at);
        budget.deadline = budget.deadline.min(active.facts.expires_at);
        self.check_time(budget)
    }
    pub fn dispatch(&mut self, wire: &[u8]) -> Result<Vec<u8>, Error> {
        let result = self.dispatch_inner(wire);
        if result.is_err()
            || self
                .active
                .as_ref()
                .is_some_and(|a| a.received == MAX_REQUESTS)
        {
            self.close();
        }
        result
    }
    fn dispatch_inner(&mut self, wire: &[u8]) -> Result<Vec<u8>, Error> {
        if self.active.is_none() {
            return Err(Error::Closed);
        }
        let now = self.now()?;
        let active = self.active.as_ref().ok_or(Error::Closed)?;
        let deadline = now
            .checked_add(CALL_BUDGET)
            .ok_or(Error::Expired)?
            .min(active.facts.expires_at);
        let mut budget = CallBudget {
            deadline,
            revocation: active.revocation.clone(),
        };
        self.check_live(&mut budget)?;
        if wire.is_empty() || wire.len() > MAX_RECORD_BYTES {
            return Err(Error::InvalidRecord);
        }
        let active = self.active.as_mut().ok_or(Error::Closed)?;
        if active.received >= MAX_REQUESTS || active.sent >= MAX_REQUESTS {
            return Err(Error::Exhausted);
        }
        let record = self
            .transport
            .open_record(&mut active.session, wire, &budget)?;
        self.check_live(&mut budget)?;
        let active = self.active.as_mut().ok_or(Error::Closed)?;
        if record.binding != active.facts.binding
            || record.version != 1
            || record.purpose != Purpose::NknRemote
            || record.direction != Direction::PeerToDevice
            || record.sequence != active.received
            || record.body.is_empty()
            || record.body.len() > MAX_BODY_BYTES
        {
            return Err(Error::InvalidRecord);
        }
        let service = Service::parse(&record.service).ok_or(Error::Denied)?;
        // Burn before entering Core, including lost reply and Unknown. Neither
        // subsequent seal failure nor a new call can roll the sequence back.
        active.received = active.received.checked_add(1).ok_or(Error::Exhausted)?;
        let outcome = match service {
            Service::CoreAuthV2 => {
                self.business
                    .core_auth(&active.facts.binding, &record.body, &budget)
            }
            Service::CallMediaV1 => {
                self.business
                    .call_media(&active.facts.binding, &record.body, &budget)
            }
        };
        let body = match outcome {
            BusinessOutcome::Reply(body) => body,
            BusinessOutcome::Denied => return Err(Error::Denied),
            BusinessOutcome::Unavailable => return Err(Error::Unavailable),
            BusinessOutcome::Unknown => return Err(Error::OutcomeUnknown),
        };
        // Every failure after an admitted execution is conservatively Unknown.
        // Never return plaintext or a guessed success on a late/revoked reply.
        self.check_live(&mut budget)
            .map_err(|_| Error::OutcomeUnknown)?;
        if body.is_empty() || body.len() > MAX_BODY_BYTES {
            return Err(Error::OutcomeUnknown);
        }
        let active = self.active.as_mut().ok_or(Error::OutcomeUnknown)?;
        let wire = self
            .transport
            .seal_reply(&mut active.session, service, active.sent, &body, &budget)
            .map_err(|_| Error::OutcomeUnknown)?;
        active.sent = active.sent.checked_add(1).ok_or(Error::OutcomeUnknown)?;
        self.check_live(&mut budget)
            .map_err(|_| Error::OutcomeUnknown)?;
        if wire.is_empty() || wire.len() > MAX_RECORD_BYTES {
            return Err(Error::OutcomeUnknown);
        }
        Ok(wire)
    }
    pub fn close(&mut self) {
        if let Some(active) = self.active.take() {
            active.revocation.revoke();
            self.transport.retire(active.session);
        }
    }
}
impl<T: TransportPort, B: BusinessPort<T::Binding>, C: Clock> Drop for Router<T, B, C> {
    fn drop(&mut self) {
        self.close();
    }
}

/// Explicit unavailable production defaults. No mock, key, listener or plaintext.
pub struct UnavailableTransport;
impl TransportPort for UnavailableTransport {
    type Admission = ();
    type Session = ();
    type Binding = ();
    fn admit(&mut self, _: (), _: &CallBudget) -> Result<((), SessionFacts<()>), PortError> {
        Err(PortError::Unavailable)
    }
    fn inspect(&mut self, _: &mut (), _: &CallBudget) -> Result<SessionFacts<()>, PortError> {
        Err(PortError::Unavailable)
    }
    fn open_record(
        &mut self,
        _: &mut (),
        _: &[u8],
        _: &CallBudget,
    ) -> Result<VerifiedRecord<()>, PortError> {
        Err(PortError::Unavailable)
    }
    fn seal_reply(
        &mut self,
        _: &mut (),
        _: Service,
        _: u64,
        _: &[u8],
        _: &CallBudget,
    ) -> Result<Vec<u8>, PortError> {
        Err(PortError::Unavailable)
    }
    fn retire(&mut self, _: ()) {}
}
pub struct UnavailableBusiness;
impl<B> BusinessPort<B> for UnavailableBusiness {
    fn core_auth(&mut self, _: &B, _: &[u8], _: &CallBudget) -> BusinessOutcome {
        BusinessOutcome::Unavailable
    }
    fn call_media(&mut self, _: &B, _: &[u8], _: &CallBudget) -> BusinessOutcome {
        BusinessOutcome::Unavailable
    }
}
impl Default for Router<UnavailableTransport, UnavailableBusiness, SystemClock> {
    fn default() -> Self {
        Self::new(UnavailableTransport, UnavailableBusiness, SystemClock)
    }
}

#[cfg(test)]
mod tests;
