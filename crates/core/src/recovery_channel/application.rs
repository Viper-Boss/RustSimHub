// SPDX-License-Identifier: AGPL-3.0-only
//! Actual accepted channel -> Auth -> original application -> A-sealed reply.
//! A's handshake/session implementation is supplied separately, never defaulted.
use super::*;
use rustsimhub_auth::RecoveryResponseScope;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;

/// One owned output for the trusted A adapter. No public constructor/Clone or
/// Deserialize; the adapter must bind its session and AEAD to this exact scope.
pub struct SealingRequest {
    scope: Arc<RecoveryResponseScope>,
    plaintext: Zeroizing<Vec<u8>>,
}
impl SealingRequest {
    pub fn scope(&self) -> &RecoveryResponseScope {
        &self.scope
    }
    pub fn plaintext(&self) -> &[u8] {
        &self.plaintext
    }
    /// Trusted A-session output only. This validates bounds and association,
    /// not encryption. Never call this on bytes labelled encrypted by a peer.
    pub fn finish_from_trusted_session(
        self,
        ciphertext: Zeroizing<Vec<u8>>,
    ) -> rustsimhub_auth::Result<SealedReply> {
        if ciphertext.is_empty() || ciphertext.len() > transport::MAX_ENVELOPE {
            return Err(rustsimhub_auth::Error::InvalidInput);
        }
        Ok(SealedReply {
            scope: self.scope,
            ciphertext,
        })
    }
}
pub struct SealedReply {
    scope: Arc<RecoveryResponseScope>,
    ciphertext: Zeroizing<Vec<u8>>,
}
/// Source-only A adapter; no implementation/default/plaintext fallback here.
/// Must not detach work, take Auth's lock or keep work running after return.
pub trait ApplicationReplySealerV1: Send + Sync {
    fn seal(&self, request: SealingRequest) -> rustsimhub_auth::Result<SealedReply>;
}
pub enum ApplicationStep {
    Continue(RecoveryChannel),
    Closed,
}

async fn scoped_check(scope: Arc<RecoveryResponseScope>) -> Result<()> {
    bounded_auth(tokio::task::spawn_blocking(move || scope.check_current())).await
}
async fn bounded_auth<T: Send + 'static, E: Into<Error> + Send + 'static>(
    job: tokio::task::JoinHandle<std::result::Result<T, E>>,
) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(5), job)
        .await
        .map_err(|_| transport::Error::Deadline)?
        .map_err(|_| rustsimhub_auth::Error::Busy)?
        .map_err(Into::into)
}
impl RecoveryChannel {
    /// One ENCRYPTED application request after A's handshake. Reads the actual
    /// accepted descriptor, never an arbitrary caller Packet. No new listener.
    /// A missing sealer rejects before reading, verifying or dispatching work.
    /// Cancellation drops the original channel token; original Wi-Fi cleanup
    /// remains the original actor's responsibility, not a successful Cancel.
    pub async fn dispatch_application_v1(
        self,
        auth: Arc<Mutex<Auth>>,
        expected: RecoveryGrant,
        sealer: Option<Arc<dyn ApplicationReplySealerV1>>,
    ) -> Result<ApplicationStep> {
        // The caller retains this fence even while the actual channel has
        // moved into a queued Auth worker or the dedicated A adapter thread.
        let mut cancellation = Some(self.token.disconnect_guard());
        let monitor = self.peer.disconnect_monitor()?;
        let result = tokio::select! {
            biased;
            _=monitor.wait_closed()=>{
                drop(cancellation.take());
                Err(transport::Error::Closed.into())
            }
            result=self.dispatch_inner(auth,expected,sealer)=>result,
        };
        if result.is_ok() {
            if let Some(guard) = cancellation.take() {
                guard.disarm();
            }
        }
        result
    }
    async fn dispatch_inner(
        self,
        auth: Arc<Mutex<Auth>>,
        expected: RecoveryGrant,
        sealer: Option<Arc<dyn ApplicationReplySealerV1>>,
    ) -> Result<ApplicationStep> {
        let sealer = sealer.ok_or(Error::Auth(rustsimhub_auth::Error::Forbidden))?;
        let (channel, packet) = self.read().await?;
        if packet.kind() == Kind::CloseTransport {
            return Ok(ApplicationStep::Closed);
        }
        // Handshake is A's separate protocol, not an application success.
        if packet.kind() != Kind::EncryptedRequest {
            return Err(transport::Error::InvalidFrame.into());
        }
        let a = auth.clone();
        let pending = bounded_auth(tokio::task::spawn_blocking(move || {
            let mut a = a.try_lock().map_err(|_| rustsimhub_auth::Error::Busy)?;
            channel.prepare(&mut a, expected, packet)
        }))
        .await?;
        let verified = pending.verify().await?;
        let a = auth.clone();
        let (channel, request, scope) = bounded_auth(tokio::task::spawn_blocking(move || {
            let mut a = a.try_lock().map_err(|_| rustsimhub_auth::Error::Busy)?;
            let (channel, request) = verified.finish(&mut a)?;
            let scope = Arc::new(a.prepare_recovery_response(&request)?);
            Ok::<_, Error>((channel, request, scope))
        }))
        .await?;
        // The channel stays owned by this future throughout the actual native
        // business operation; dropping it invalidates the original background gate.
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            crate::network_trial::provision::recovery_v1(auth, request),
        )
        .await
        .map_err(|_| transport::Error::Deadline)?;
        let json = match response {
            Ok(v) => serde_json::to_vec(&v),
            Err(e) => serde_json::to_vec(&e),
        }
        .map_err(|_| rustsimhub_auth::Error::Io)?;
        scoped_check(scope.clone()).await?;
        let sealing = SealingRequest {
            scope: scope.clone(),
            plaintext: Zeroizing::new(json),
        };
        let RecoveryChannel { token, peer } = channel;
        // Existing worker retains the REAL stream/slot until a timed-out or
        // cancelled A sealer actually drains. No additional unbounded pool.
        let (peer, sealed) = peer
            .verify_blocking(
                Packet::new(Kind::EncryptedRequest, vec![])?,
                move |peer, _| {
                    let s = sealing.scope();
                    if s.binding().transport_nonce() != peer.connection_nonce() {
                        return Err(transport::Error::Denied);
                    }
                    s.check_current().map_err(|_| transport::Error::Denied)?;
                    let sealed = sealer.seal(sealing).map_err(|_| transport::Error::Denied)?;
                    sealed
                        .scope
                        .check_current()
                        .map_err(|_| transport::Error::Denied)?;
                    Ok(sealed)
                },
            )
            .await?;
        let channel = RecoveryChannel { token, peer };
        channel.require_live()?;
        if !Arc::ptr_eq(&scope, &sealed.scope) {
            return Err(transport::Error::Denied.into());
        }
        scoped_check(scope.clone()).await?;
        let packet = Packet::new(Kind::EncryptedRequest, sealed.ciphertext.to_vec())?;
        let channel = channel.write(&packet).await?;
        scoped_check(scope).await?;
        Ok(ApplicationStep::Continue(channel))
    }
}
