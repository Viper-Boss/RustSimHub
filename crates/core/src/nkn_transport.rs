// SPDX-License-Identifier: AGPL-3.0-only
//! Actual purpose2 cipher producer for the existing A26 finite routing policy.
use rustsimhub_auth::{
    nkn_session::{NknBinding, NknScope, NknSession},
    Auth,
};
use rustsimhub_nkn_agent::routing::{
    CallBudget, Direction, PortError, Purpose, Service, SessionFacts, TransportPort, VerifiedRecord,
};
use rustsimhub_recovery_session::Action;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

struct GateState {
    live: AtomicBool,
    scope: Mutex<Option<NknScope>>,
    nonce: [u8; 32],
    deadline: Instant,
    parent: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}
#[derive(Clone)]
pub struct SourceGate(Arc<GateState>);
impl SourceGate {
    pub(crate) fn new(nonce: [u8; 32], deadline: Instant) -> Self {
        Self(Arc::new(GateState {
            live: AtomicBool::new(true),
            scope: Mutex::new(None),
            nonce,
            deadline,
            parent: None,
        }))
    }
    #[cfg(feature = "root-provider-candidate")]
    pub(crate) fn from_root(
        nonce: [u8; 32],
        admission: Arc<rustsimhub_core::nkn_root::Admission>,
    ) -> Self {
        let deadline = admission.deadline();
        let parent = admission.parent_fence();
        Self(Arc::new(GateState {
            live: AtomicBool::new(true),
            scope: Mutex::new(None),
            nonce,
            deadline,
            parent: Some(parent),
        }))
    }
    pub fn close(&self) {
        self.0.live.store(false, Ordering::Release);
        let scope = self.0.scope.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(scope) = scope.as_ref() {
            scope.invalidate();
        }
    }
    pub(crate) fn check(&self) -> Result<(), PortError> {
        if !self.0.live.load(Ordering::Acquire)
            || Instant::now() >= self.0.deadline
            || self.0.parent.as_ref().is_some_and(|p| !p())
        {
            return Err(PortError::Denied);
        }
        Ok(())
    }
    pub(crate) fn publish(&self, scope: NknScope) -> Result<(), PortError> {
        let mut saved = self.0.scope.lock().map_err(|_| PortError::Denied)?;
        if scope.transport_nonce() != &self.0.nonce || saved.is_some() {
            scope.invalidate();
            return Err(PortError::Denied);
        }
        if let Some(parent) = &self.0.parent {
            scope
                .restrict_parent(parent.clone())
                .map_err(|_| PortError::Denied)?;
        }
        *saved = Some(scope);
        if self.check().is_err() {
            if let Some(scope) = saved.as_ref() {
                scope.invalidate();
            }
            return Err(PortError::Denied);
        }
        Ok(())
    }
}
pub struct NknAdmission {
    session: NknSession,
    gate: SourceGate,
}
impl NknAdmission {
    pub(crate) fn new(session: NknSession, gate: SourceGate) -> Self {
        Self { session, gate }
    }
}
pub struct ActiveCipher {
    session: NknSession,
    gate: SourceGate,
    received: u64,
    sent: u64,
    pending: Option<([u8; 16], Service)>,
}
pub struct NknTransportPort {
    auth: Arc<Mutex<Auth>>,
    gate: SourceGate,
}
impl NknTransportPort {
    pub(crate) fn new(auth: Arc<Mutex<Auth>>, gate: SourceGate) -> Self {
        Self { auth, gate }
    }
    fn facts(
        &self,
        active: &ActiveCipher,
        budget: &CallBudget,
    ) -> Result<SessionFacts<NknBinding>, PortError> {
        self.gate.check()?;
        active.gate.check()?;
        if !Arc::ptr_eq(&self.gate.0, &active.gate.0)
            || budget.cancelled()
            || Instant::now() >= budget.deadline()
        {
            return Err(PortError::Denied);
        }
        let scope = active.session.scope().map_err(|_| PortError::Denied)?;
        if scope.transport_nonce() != &self.gate.0.nonce {
            return Err(PortError::Denied);
        }
        self.auth
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .check_nkn_scope(&scope)
            .map_err(|_| PortError::Denied)?;
        self.gate.check()?;
        Ok(SessionFacts {
            binding: scope.binding().clone(),
            purpose: Purpose::NknRemote,
            expires_at: scope.deadline().min(self.gate.0.deadline),
        })
    }
}
impl TransportPort for NknTransportPort {
    type Admission = NknAdmission;
    type Session = ActiveCipher;
    type Binding = NknBinding;
    fn admit(
        &mut self,
        admission: NknAdmission,
        budget: &CallBudget,
    ) -> Result<(ActiveCipher, SessionFacts<NknBinding>), PortError> {
        let active = ActiveCipher {
            session: admission.session,
            gate: admission.gate,
            received: 0,
            sent: 0,
            pending: None,
        };
        let facts = self.facts(&active, budget)?;
        Ok((active, facts))
    }
    fn inspect(
        &mut self,
        session: &mut ActiveCipher,
        budget: &CallBudget,
    ) -> Result<SessionFacts<NknBinding>, PortError> {
        self.facts(session, budget)
    }
    fn open_record(
        &mut self,
        session: &mut ActiveCipher,
        wire: &[u8],
        budget: &CallBudget,
    ) -> Result<VerifiedRecord<NknBinding>, PortError> {
        let facts = self.facts(session, budget)?;
        if session.pending.is_some() {
            return Err(PortError::Denied);
        }
        let request = session
            .session
            .open_record(wire)
            .map_err(|_| PortError::InvalidRecord)?;
        // The authenticated sequence includes one client binding and one
        // membership bootstrap request. Normalize only this fixed offset for
        // A26's business counter; the Noise cipher retains its actual nonce.
        let sequence = request
            .sequence
            .checked_sub(2)
            .ok_or(PortError::InvalidRecord)?;
        if sequence != session.received {
            return Err(PortError::InvalidRecord);
        }
        let service = match request.action {
            Action::NormalCore => Service::CoreAuthV2,
            Action::CallMedia => Service::CallMediaV1,
            _ => return Err(PortError::Denied),
        };
        session.pending = Some((request.id, service));
        session.received = session.received.checked_add(1).ok_or(PortError::Denied)?;
        Ok(VerifiedRecord {
            binding: facts.binding,
            version: 1,
            purpose: Purpose::NknRemote,
            direction: Direction::PeerToDevice,
            sequence,
            service: match service {
                Service::CoreAuthV2 => "core-auth-v2",
                Service::CallMediaV1 => "call-media-v1",
            }
            .into(),
            body: request.body.to_vec(),
        })
    }
    fn seal_reply(
        &mut self,
        session: &mut ActiveCipher,
        service: Service,
        sequence: u64,
        body: &[u8],
        budget: &CallBudget,
    ) -> Result<Vec<u8>, PortError> {
        self.facts(session, budget)?;
        let (id, expected) = session.pending.take().ok_or(PortError::Denied)?;
        if expected != service || sequence != session.sent {
            return Err(PortError::Denied);
        }
        let wire = session
            .session
            .seal_reply(id, body)
            .map_err(|_| PortError::Denied)?;
        session.sent = session.sent.checked_add(1).ok_or(PortError::Denied)?;
        self.facts(session, budget)?;
        Ok(wire)
    }
    fn retire(&mut self, session: ActiveCipher) {
        session.gate.close();
        drop(session);
    }
}
