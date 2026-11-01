// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Actual finite supervisor policy. No process, SDK, Root IPC, shell, network,
//! identity or privileged fallback is created here. Production adapters must
//! implement a lead-approved fixed policy and real evidence producers. Such
//! adapters are deliberately unavailable in this candidate library.
pub mod routing;

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    ManualEngine,
    Remote,
    Statistics,
}
impl Owner {
    fn index(self) -> usize {
        match self {
            Self::ManualEngine => 0,
            Self::Remote => 1,
            Self::Statistics => 2,
        }
    }
}
/// Local ownership ticket, not an authentication proof. Cannot Clone/serialize.
pub struct Reference {
    instance: u64,
    owner: Owner,
    id: u64,
}
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Disabled,
    Starting,
    Ready,
    WaitingRetry,
    Stopping,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortError {
    Denied,
    Unavailable,
    Deadline,
    Offline,
    Io,
    Protocol,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    DuplicateOwner,
    StaleReference,
    ForeignReference,
    NoOwners,
    ExpiredProof,
    Unavailable,
    ClockRollback,
    Exhausted,
    InvalidLimits,
    StaleContext,
    Cancelled,
    Deadline,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NKN supervisor: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub phase: Phase,
    pub epoch: u64,
    pub references: u8,
    pub attempts: u8,
    pub last_error: Option<PortError>,
}
pub struct Budget {
    deadline: Instant,
    epoch: u64,
    cancel: Cancellation,
    cleanup: bool,
}
impl Budget {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn is_cleanup(&self) -> bool {
        self.cleanup
    }
    pub fn cancelled(&self) -> bool {
        !self.cleanup && self.cancel.0.load(Ordering::SeqCst)
    }
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}
pub trait Clock {
    fn now(&self) -> Instant;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootState {
    Active,
    Released,
    Unknown,
}
// Released must be verified for the ORIGINAL owned scope: no remaining policy
// or child and no late-start path can recreate it. It is not a JSON claim or
// evidence about any unrelated daemon. Root.start atomically rejects occupied
// registrations/invalid peer credentials/expired or mismatched proofs.
pub enum RootStart {
    Active,
    Rejected(PortError),
    Unknown(PortError),
}
/// Policy/Proof are provider-owned opaque types. No new serialization or
/// webpage-provided credential/path/UID/route format is invented by this crate.
pub trait RootPort {
    type Policy;
    type Proof;
    fn expires_at(&self, proof: &Self::Proof) -> Option<Instant>;
    fn status(&mut self, proof: &Self::Proof, budget: &Budget) -> Result<RootState, PortError>;
    fn start(&mut self, policy: &Self::Policy, proof: &Self::Proof, budget: &Budget) -> RootStart;
    fn stop(&mut self, proof: &Self::Proof, budget: &Budget) -> Result<RootState, PortError>;
}
pub enum EngineStart<P> {
    Started(P),
    Partial(P),
    Rejected(PortError),
    Unknown(PortError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineState {
    Starting,
    Ready,
    Exited,
    Unknown,
}
pub trait EnginePort<Proof> {
    type Process;
    /// Capability availability only, not authorization or readiness.
    fn supported(&self) -> bool;
    fn start(&mut self, proof: &Proof, budget: &Budget) -> EngineStart<Self::Process>;
    /// Exited means the EXACT owned child has been observed/reaped. Ready
    /// requires real engine evidence, not a PID/stdout/unavailable JSON reply.
    fn status(
        &mut self,
        process: &mut Self::Process,
        budget: &Budget,
    ) -> Result<EngineState, PortError>;
    fn stop(
        &mut self,
        process: &mut Self::Process,
        budget: &Budget,
    ) -> Result<EngineState, PortError>;
}
#[derive(Clone, Copy)]
pub struct Limits {
    start: Duration,
    stop: Duration,
    retry_delay: Duration,
    retry_window: Duration,
    attempts: u8,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            start: Duration::from_secs(10),
            stop: Duration::from_secs(5),
            retry_delay: Duration::from_secs(5),
            retry_window: Duration::from_secs(30),
            attempts: 3,
        }
    }
}
impl Limits {
    pub fn bounded(
        start: Duration,
        stop: Duration,
        retry_delay: Duration,
        retry_window: Duration,
        attempts: u8,
    ) -> Result<Self, Error> {
        let maximum = Self::default();
        if start.is_zero()
            || stop.is_zero()
            || start > maximum.start
            || stop > maximum.stop
            || retry_delay < maximum.retry_delay
            || retry_window.is_zero()
            || retry_window > maximum.retry_window
            || attempts == 0
            || attempts > 3
        {
            return Err(Error::InvalidLimits);
        }
        Ok(Self {
            start,
            stop,
            retry_delay,
            retry_window,
            attempts,
        })
    }
}

pub struct Supervisor<R: RootPort, E: EnginePort<R::Proof>, C: Clock> {
    root: R,
    engine: E,
    clock: C,
    limits: Limits,
    instance: u64,
    next_reference: u64,
    owners: [Option<u64>; 3],
    phase: Phase,
    armed: bool,
    epoch: u64,
    cancel: Cancellation,
    proof: Option<R::Proof>,
    policy: Option<R::Policy>,
    process: Option<E::Process>,
    root_stop_sent: bool,
    engine_stop_sent: bool,
    root_released: bool,
    attempts: u8,
    started: Option<Instant>,
    startup_deadline: Option<Instant>,
    stop_deadline: Option<Instant>,
    last_launch: Option<Instant>,
    last_clock: Option<Instant>,
    last_error: Option<PortError>,
}
impl<R: RootPort, E: EnginePort<R::Proof>, C: Clock> Supervisor<R, E, C> {
    /// No clock/Root/engine calls or tasks on construction/default OFF.
    pub fn new(root: R, engine: E, clock: C) -> Result<Self, Error> {
        let instance = NEXT_INSTANCE
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_add(1))
            .map_err(|_| Error::Exhausted)?;
        Ok(Self {
            root,
            engine,
            clock,
            limits: Limits::default(),
            instance,
            next_reference: 1,
            owners: [None; 3],
            phase: Phase::Disabled,
            armed: false,
            epoch: 0,
            cancel: Cancellation(Arc::new(AtomicBool::new(false))),
            proof: None,
            policy: None,
            process: None,
            root_stop_sent: false,
            engine_stop_sent: false,
            root_released: false,
            attempts: 0,
            started: None,
            startup_deadline: None,
            stop_deadline: None,
            last_launch: None,
            last_clock: None,
            last_error: None,
        })
    }
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            phase: self.phase,
            epoch: self.epoch,
            references: self.owners.iter().filter(|v| v.is_some()).count() as u8,
            attempts: self.attempts,
            last_error: self.last_error,
        }
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancel.clone()
    }
    pub fn acquire(&mut self, owner: Owner) -> Result<Reference, Error> {
        if self.owners[owner.index()].is_some() {
            return Err(Error::DuplicateOwner);
        }
        let id = self.next_reference;
        self.next_reference = id.checked_add(1).ok_or(Error::Exhausted)?;
        self.owners[owner.index()] = Some(id);
        Ok(Reference {
            instance: self.instance,
            owner,
            id,
        })
    }
    pub fn release(&mut self, reference: Reference) -> Result<Snapshot, Error> {
        if reference.instance != self.instance {
            return Err(Error::ForeignReference);
        }
        if self.owners[reference.owner.index()] != Some(reference.id) {
            return Err(Error::StaleReference);
        }
        self.owners[reference.owner.index()] = None;
        if self.owners.iter().all(Option::is_none) && self.proof.is_some() {
            self.cancel.cancel();
            self.phase = Phase::Stopping;
        }
        Ok(self.snapshot())
    }
    fn now(&mut self) -> Result<Instant, Error> {
        let now = self.clock.now();
        if self.last_clock.is_some_and(|last| now < last) {
            self.cancel.cancel();
            if self.proof.is_some() {
                self.phase = Phase::Unknown;
            }
            self.last_error = Some(PortError::Protocol);
            return Err(Error::ClockRollback);
        }
        self.last_clock = Some(now);
        Ok(now)
    }
    fn budget(&self, now: Instant, cleanup: bool) -> Result<Budget, Error> {
        let limit = if cleanup {
            self.limits.stop
        } else {
            self.limits.start
        };
        let mut deadline = now.checked_add(limit).ok_or(Error::InvalidLimits)?;
        if !cleanup {
            let expiry = self
                .proof
                .as_ref()
                .and_then(|p| self.root.expires_at(p))
                .ok_or(Error::Unavailable)?;
            if expiry <= now {
                return Err(Error::ExpiredProof);
            }
            deadline = deadline.min(expiry);
            if self.phase == Phase::Starting {
                if let Some(startup) = self.startup_deadline {
                    deadline = deadline.min(startup);
                }
            }
            if deadline <= now {
                return Err(Error::Deadline);
            }
        }
        Ok(Budget {
            deadline,
            epoch: self.epoch,
            cancel: self.cancel.clone(),
            cleanup,
        })
    }
    fn timely(&mut self, budget: &Budget) -> Result<bool, Error> {
        Ok(self.now()? < budget.deadline && !budget.cancelled())
    }
    fn stopping(&mut self, error: PortError) {
        self.phase = Phase::Unknown;
        self.last_error = Some(error);
        self.cancel.cancel();
    }
    /// Local single-use run context. It carries no Root authority. Acquire it
    /// BEFORE scheduling begin so cancellation also works inside Root.start.
    pub fn arm(&mut self) -> Result<Cancellation, Error> {
        if self.proof.is_some() || self.phase != Phase::Disabled {
            return Err(Error::Busy);
        }
        self.epoch = self.epoch.checked_add(1).ok_or(Error::Exhausted)?;
        self.cancel = Cancellation(Arc::new(AtomicBool::new(false)));
        self.armed = true;
        Ok(self.cancel.clone())
    }
    pub fn begin(
        &mut self,
        context: Cancellation,
        policy: R::Policy,
        proof: R::Proof,
    ) -> Result<Snapshot, Error> {
        if self.proof.is_some() || self.phase != Phase::Disabled {
            return Err(Error::Busy);
        }
        if !self.armed || !Arc::ptr_eq(&context.0, &self.cancel.0) {
            return Err(Error::StaleContext);
        }
        self.armed = false;
        if context.0.load(Ordering::SeqCst) {
            return Err(Error::Cancelled);
        }
        if self.owners.iter().all(Option::is_none) {
            return Err(Error::NoOwners);
        }
        if !self.engine.supported() {
            return Err(Error::Unavailable);
        }
        let now = self.now()?;
        let expiry = self.root.expires_at(&proof).ok_or(Error::Unavailable)?;
        if expiry <= now {
            return Err(Error::ExpiredProof);
        }
        self.proof = Some(proof);
        self.policy = Some(policy);
        self.phase = Phase::Starting;
        self.attempts = 0;
        self.started = Some(now);
        self.startup_deadline = None;
        self.stop_deadline = None;
        self.last_launch = None;
        self.last_error = None;
        self.root_stop_sent = false;
        self.engine_stop_sent = false;
        self.root_released = false;
        let budget = self.budget(now, false)?;
        self.startup_deadline = Some(budget.deadline);
        // Own the original proof BEFORE calling an external start: any lost
        // ACK stays cleanup-only and never starts another Root policy.
        let result = self.root.start(
            self.policy.as_ref().expect("owned policy"),
            self.proof.as_ref().expect("owned proof"),
            &budget,
        );
        if !self.timely(&budget)? {
            self.stopping(PortError::Deadline);
            return Ok(self.snapshot());
        }
        match result {
            RootStart::Active => self.launch(&budget)?,
            RootStart::Rejected(reason) => {
                self.proof = None;
                self.policy = None;
                self.phase = Phase::Disabled;
                self.last_error = Some(reason);
            }
            RootStart::Unknown(reason) => self.stopping(reason),
        }
        Ok(self.snapshot())
    }
    fn launch(&mut self, budget: &Budget) -> Result<(), Error> {
        if budget.cancelled() || !self.timely(budget)? {
            self.stopping(PortError::Deadline);
            return Ok(());
        }
        self.attempts += 1;
        self.startup_deadline = Some(budget.deadline);
        self.last_launch = self.last_clock;
        self.engine_stop_sent = false;
        let result = self
            .engine
            .start(self.proof.as_ref().expect("owned proof"), budget);
        // Always retain a late/partial process handle for exact cleanup.
        let outcome = match result {
            EngineStart::Started(process) => {
                self.process = Some(process);
                None
            }
            EngineStart::Partial(process) => {
                self.process = Some(process);
                Some(PortError::Protocol)
            }
            EngineStart::Rejected(error) => {
                self.process = None;
                Some(error)
            }
            EngineStart::Unknown(error) => {
                self.stopping(error);
                return Ok(());
            }
        };
        if !self.timely(budget)? {
            self.stopping(PortError::Deadline);
            return Ok(());
        }
        if self.process.is_some() {
            if let Some(error) = outcome {
                self.stopping(error)
            } else {
                self.phase = Phase::Starting;
            }
        } else {
            self.last_error = outcome;
            if outcome == Some(PortError::Offline) {
                self.phase = Phase::WaitingRetry;
            } else {
                self.stopping(outcome.unwrap_or(PortError::Protocol));
            }
        }
        Ok(())
    }
    pub fn drive(&mut self) -> Result<Snapshot, Error> {
        if self.proof.is_none() {
            return Ok(self.snapshot());
        }
        let now = self.now()?;
        if self.cancel.0.load(Ordering::SeqCst)
            || self.owners.iter().all(Option::is_none)
            || matches!(self.phase, Phase::Stopping | Phase::Unknown)
        {
            self.cleanup(now)?;
            return Ok(self.snapshot());
        }
        let budget = match self.budget(now, false) {
            Ok(b) => b,
            Err(Error::ExpiredProof | Error::Unavailable) => {
                self.stopping(PortError::Denied);
                self.cleanup(now)?;
                return Ok(self.snapshot());
            }
            Err(Error::Deadline) => {
                self.stopping(PortError::Deadline);
                self.cleanup(now)?;
                return Ok(self.snapshot());
            }
            Err(e) => return Err(e),
        };
        if self.phase == Phase::WaitingRetry {
            let within = self.started.is_some_and(|start| {
                now.saturating_duration_since(start) < self.limits.retry_window
            });
            if !within || self.attempts >= self.limits.attempts {
                self.stopping(PortError::Deadline);
                self.cleanup(now)?;
                return Ok(self.snapshot());
            }
            if self
                .last_launch
                .is_some_and(|last| now.saturating_duration_since(last) < self.limits.retry_delay)
            {
                return Ok(self.snapshot());
            }
        }
        let root = self
            .root
            .status(self.proof.as_ref().expect("owned proof"), &budget);
        if !self.timely(&budget)? {
            self.stopping(PortError::Deadline);
            return Ok(self.snapshot());
        }
        if root != Ok(RootState::Active) {
            self.stopping(root.err().unwrap_or(PortError::Protocol));
            return Ok(self.snapshot());
        }
        if self.phase == Phase::WaitingRetry {
            self.launch(&budget)?;
            return Ok(self.snapshot());
        }
        if let Some(process) = self.process.as_mut() {
            let state = self.engine.status(process, &budget);
            if !self.timely(&budget)? {
                self.stopping(PortError::Deadline);
                return Ok(self.snapshot());
            }
            match state {
                Ok(EngineState::Ready) => {
                    self.phase = Phase::Ready;
                    self.startup_deadline = None;
                }
                Ok(EngineState::Starting) => {
                    if self.phase == Phase::Ready {
                        self.stopping(PortError::Offline);
                    } else {
                        self.phase = Phase::Starting;
                    }
                }
                Ok(EngineState::Exited) => {
                    self.process = None;
                    self.phase = Phase::WaitingRetry;
                }
                Ok(EngineState::Unknown) => self.stopping(PortError::Protocol),
                Err(error) => self.stopping(error),
            }
        } else {
            self.stopping(PortError::Protocol);
        }
        Ok(self.snapshot())
    }
    /// Explicitly continue cleanup of the SAME retained scope after its total
    /// stop budget expires. Never repeats an already-issued stop, starts a
    /// child, changes proof/policy, or renews authority. Ordinary drive calls
    /// after expiry only reconcile observations under a fresh query budget.
    pub fn resume_cleanup(&mut self) -> Result<Snapshot, Error> {
        if self.proof.is_none() || !matches!(self.phase, Phase::Stopping | Phase::Unknown) {
            return Err(Error::Busy);
        }
        self.cancel.cancel();
        self.stop_deadline = None;
        self.phase = Phase::Stopping;
        Ok(self.snapshot())
    }
    fn cleanup(&mut self, now: Instant) -> Result<(), Error> {
        self.phase = Phase::Stopping;
        let mut budget = self.budget(now, true)?;
        let stop_deadline = *self.stop_deadline.get_or_insert(budget.deadline);
        let query_only = now >= stop_deadline;
        if !query_only {
            budget.deadline = stop_deadline;
        } else {
            self.last_error = Some(PortError::Deadline);
        }
        if let Some(process) = self.process.as_mut() {
            let state = if !self.engine_stop_sent && !query_only {
                self.engine_stop_sent = true;
                self.engine.stop(process, &budget)
            } else {
                self.engine.status(process, &budget)
            };
            if !self.timely(&budget)? {
                self.phase = Phase::Unknown;
                self.last_error = Some(PortError::Deadline);
                return Ok(());
            }
            match state {
                Ok(EngineState::Exited) => self.process = None,
                Err(e) => self.last_error = Some(e),
                _ => self.last_error = Some(PortError::Protocol),
            }
        }
        if self.process.is_none() && !self.root_released {
            let state = if !self.root_stop_sent && !query_only {
                self.root_stop_sent = true;
                self.root
                    .stop(self.proof.as_ref().expect("owned proof"), &budget)
            } else {
                self.root
                    .status(self.proof.as_ref().expect("owned proof"), &budget)
            };
            if !self.timely(&budget)? {
                self.phase = Phase::Unknown;
                self.last_error = Some(PortError::Deadline);
                return Ok(());
            }
            match state {
                Ok(RootState::Released) => self.root_released = true,
                Err(e) => self.last_error = Some(e),
                _ => self.last_error = Some(PortError::Protocol),
            }
        }
        if self.process.is_none() && self.root_released {
            self.proof = None;
            self.policy = None;
            self.phase = Phase::Disabled;
        } else {
            self.phase = Phase::Unknown;
        }
        Ok(())
    }
}

impl<R: RootPort, E: EnginePort<R::Proof>, C: Clock> Drop for Supervisor<R, E, C> {
    fn drop(&mut self) {
        // Best effort, original scope only. This cannot replace the real
        // Root provider's lease-expiry/cgroup cleanup if the host crashes or
        // drops a manager whose exit is still unproved. Never claims success.
        self.cancel.cancel();
        if self.proof.is_some() {
            let now = self.clock.now();
            let _ = self.cleanup(now);
        }
    }
}

// Deliberately unavailable production default. No Mock or mutable success flag.
pub struct UnavailableRoot;
impl RootPort for UnavailableRoot {
    type Policy = ();
    type Proof = ();
    fn expires_at(&self, _: &()) -> Option<Instant> {
        None
    }
    fn status(&mut self, _: &(), _: &Budget) -> Result<RootState, PortError> {
        Err(PortError::Unavailable)
    }
    fn start(&mut self, _: &(), _: &(), _: &Budget) -> RootStart {
        RootStart::Rejected(PortError::Unavailable)
    }
    fn stop(&mut self, _: &(), _: &Budget) -> Result<RootState, PortError> {
        Err(PortError::Unavailable)
    }
}
pub struct UnavailableEngine;
pub enum NoProcess {}
impl EnginePort<()> for UnavailableEngine {
    type Process = NoProcess;
    fn supported(&self) -> bool {
        false
    }
    fn start(&mut self, _: &(), _: &Budget) -> EngineStart<NoProcess> {
        EngineStart::Rejected(PortError::Unavailable)
    }
    fn status(&mut self, process: &mut NoProcess, _: &Budget) -> Result<EngineState, PortError> {
        match *process {}
    }
    fn stop(&mut self, process: &mut NoProcess, _: &Budget) -> Result<EngineState, PortError> {
        match *process {}
    }
}

#[cfg(test)]
mod tests;
