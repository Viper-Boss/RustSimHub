// SPDX-License-Identifier: AGPL-3.0-only
//! Normal authenticated signaling host. Reuses Core's original media lease and
//! ticket RPCs; no call command, HTTP listener, alternate identity or lease book.
mod port;
pub use rustsimhub_domain::CallLease;
use rustsimhub_media::{
    remote_worker::{self as wire, Client, Command, Release},
    worker::{Counters, Phase},
    Error,
};
use serde::Serialize;
use std::path::PathBuf;
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Error>;
/// Normal authenticated request material, not a capability. Core verifies the
/// token, existing call/line/lease/owner/scopes; expected_peer comes from that
/// same normal signaling host, not inferred from an unsigned SDP fingerprint.
pub struct Prepare {
    pub token: String,
    pub lease: CallLease,
    pub relay_handle: String,
    pub expected_peer: [u8; 32],
    pub offer: String,
}
/// Outbound transport metadata only. No Deserialize, ticket, peer key, token,
/// Core binding, registration credentials or worker-wide command is exposed.
#[derive(Serialize)]
pub struct Status {
    pub phase: Phase,
    pub pcm_opened: bool,
    pub counters: Counters,
    pub release: Release,
}
struct Endpoints {
    core: PathBuf,
    core_uid: u32,
    worker: PathBuf,
    worker_uid: u32,
}
/// One opaque process-local host and one actual accepted media connection.
/// No Clone/Debug/Deserialize or mutable identity/lease/session setters.
pub struct Host {
    core: port::Core,
    worker: Client,
    session: String,
    token: Zeroizing<String>,
    lease: CallLease,
    binding: String,
    expected_peer: [u8; 32],
    activation_started: bool,
}
impl Host {
    pub async fn prepare(request: Prepare) -> Result<(Self, String)> {
        if rustix::process::getuid().as_raw() != 985 || rustix::process::geteuid().as_raw() != 985 {
            return Err(Error::Denied);
        }
        Self::prepare_on(
            request,
            Endpoints {
                core: "/run/rustsimhub/core/core.sock".into(),
                core_uid: 986,
                worker: "/run/rustsimhub/media/remote.sock".into(),
                worker_uid: 984,
            },
        )
        .await
    }
    async fn prepare_on(request: Prepare, endpoints: Endpoints) -> Result<(Self, String)> {
        if request.expected_peer == [0; 32]
            || !wire::relay_handle(&request.relay_handle)
            || request.offer.is_empty()
            || request.offer.len() > wire::MAX_SDP
        {
            return Err(Error::Denied);
        }
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|_| Error::Entropy)?;
        let session = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let mut core = port::Core::new(endpoints.core, endpoints.core_uid, session.clone())?;
        let token = Zeroizing::new(request.token);
        // Before even connecting Media or creating a TURN allocation, the real
        // normal Core must approve this current active call and original lease.
        let binding = core.check(&token, &request.lease).await?;
        let worker =
            Client::connect(&endpoints.worker, endpoints.worker_uid, session.clone()).await?;
        let mut host = Self {
            core,
            worker,
            session,
            token,
            lease: request.lease,
            binding,
            expected_peer: request.expected_peer,
            activation_started: false,
        };
        let guard = host.worker.cancellation_guard();
        let snapshot = host
            .worker
            .exchange(Command::Prepare {
                session: host.session.clone(),
                relay_handle: request.relay_handle,
                expected_peer: host.expected_peer,
                offer: request.offer,
            })
            .await?
            .result
            .map_err(|_| Error::Denied)?;
        host.check().await?;
        host.validate(&snapshot)?;
        let answer = snapshot.answer.ok_or(Error::Denied)?;
        guard.disarm();
        Ok((host, answer))
    }
    async fn check(&mut self) -> Result<()> {
        let binding = self.core.check(&self.token, &self.lease).await?;
        if binding != self.binding {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn validate(&self, snapshot: &wire::Snapshot) -> Result<()> {
        if snapshot.failure.is_some()
            || !self.activation_started
                && matches!(snapshot.phase, Phase::Authorizing | Phase::Active)
            || matches!(snapshot.phase, Phase::Closing | Phase::Closed)
            || snapshot
                .peer_key
                .is_some_and(|key| key != self.expected_peer)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn status(snapshot: wire::Snapshot) -> Status {
        Status {
            phase: snapshot.phase,
            pcm_opened: snapshot.pcm_opened,
            counters: snapshot.counters,
            release: snapshot.release,
        }
    }
    pub async fn poll(&mut self) -> Result<Status> {
        let guard = self.worker.cancellation_guard();
        self.check().await?;
        let snapshot = self
            .worker
            .exchange(Command::Inspect {
                session: self.session.clone(),
            })
            .await?
            .result
            .map_err(|_| Error::Denied)?;
        self.check().await?;
        self.validate(&snapshot)?;
        let status = Self::status(snapshot);
        guard.disarm();
        Ok(status)
    }
    pub async fn activate(&mut self) -> Result<Status> {
        let guard = self.worker.cancellation_guard();
        if self.activation_started {
            return Err(Error::Denied);
        }
        self.check().await?;
        let actual = self
            .worker
            .exchange(Command::Inspect {
                session: self.session.clone(),
            })
            .await?
            .result
            .map_err(|_| Error::Denied)?;
        self.validate(&actual)?;
        if actual.phase != Phase::Verified
            || actual.peer_key != Some(self.expected_peer)
            || actual.pcm_opened
        {
            return Err(Error::Denied);
        }
        self.check().await?;
        // Burn this host's attempt before awaiting a potentially committed Core
        // ticket. Lost/cancelled replies never cause an automatic reissue.
        self.activation_started = true;
        let credentials = self
            .core
            .issue(&self.token, &self.lease, self.expected_peer)
            .await?;
        let snapshot = self
            .worker
            .exchange(Command::Authorize {
                session: self.session.clone(),
                credentials,
            })
            .await?
            .result
            .map_err(|_| Error::Denied)?;
        self.check().await?;
        self.validate(&snapshot)?;
        if !matches!(snapshot.phase, Phase::Authorizing | Phase::Active) {
            return Err(Error::Denied);
        }
        let status = Self::status(snapshot);
        guard.disarm();
        Ok(status)
    }
    /// Only this opaque host's local media connection; never StopAll, normal
    /// call Hangup/Cancel/Renew or a claim of remote TURN release verification.
    pub async fn close(mut self) -> Result<Status> {
        let snapshot = self
            .worker
            .exchange(Command::Stop {
                session: self.session.clone(),
            })
            .await?
            .result
            .map_err(|_| Error::Denied)?;
        if !matches!(snapshot.phase, Phase::Closing | Phase::Closed)
            || snapshot.release != Release::Unknown
        {
            return Err(Error::Denied);
        }
        Ok(Self::status(snapshot))
    }
}
// Compile-time isolated fixtures only, not a feature or installed path override.
#[cfg(test)]
pub struct FixtureEndpoints {
    pub core: PathBuf,
    pub core_uid: u32,
    pub worker: PathBuf,
    pub worker_uid: u32,
}
#[cfg(test)]
pub async fn prepare_fixture(
    request: Prepare,
    endpoints: FixtureEndpoints,
) -> Result<(Host, String)> {
    Host::prepare_on(
        request,
        Endpoints {
            core: endpoints.core,
            core_uid: endpoints.core_uid,
            worker: endpoints.worker,
            worker_uid: endpoints.worker_uid,
        },
    )
    .await
}
