// SPDX-License-Identifier: AGPL-3.0-only
//! Owned bridge between an actually accepted peer and the same Core's Auth gates.
//! No listener, verifier configuration, cryptography, or business dispatch.
use crate::recovery_transport::{self as transport, Kind, Packet, PeerConnection};
use rustsimhub_auth::{
    recovery_capability::{
        AuthorizedRecoveryRequest, RecoveryConnection, RecoveryVerification,
        RecoveryVerificationResult,
    },
    Auth,
};
use rustsimhub_storage::RecoveryGrant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Transport(transport::Error),
    Auth(rustsimhub_auth::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<transport::Error> for Error {
    fn from(e: transport::Error) -> Self {
        Self::Transport(e)
    }
}
impl From<rustsimhub_auth::Error> for Error {
    fn from(e: rustsimhub_auth::Error) -> Self {
        Self::Auth(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "recovery channel: {self:?}")
    }
}
impl std::error::Error for Error {}

/// No public fields, Clone, Serde, or raw-socket constructor. Drop invalidates Auth
/// before releasing the accepted peer's slot. It does not cancel Wi-Fi or revoke a grant.
pub struct RecoveryChannel {
    token: RecoveryConnection,
    peer: PeerConnection,
}
impl RecoveryChannel {
    /// Short synchronous Auth phase, on the host's existing Auth worker/lock.
    /// The only source nonce is from SourceGuard's private kernel-checked peer.
    pub fn attach(auth: &mut Auth, peer: PeerConnection) -> Result<Self> {
        let nonce = *peer
            .context()
            .ok_or(transport::Error::Closed)?
            .connection_nonce();
        let token = auth.open_recovery_connection_from_trusted_transport(nonce)?;
        Ok(Self { token, peer })
    }
    /// Owned I/O: dropping even an unpolled future closes this channel.
    pub async fn read(mut self) -> Result<(Self, Packet)> {
        self.require_live()?;
        let packet = self.peer.read().await?;
        if packet.kind() == Kind::CloseTransport {
            self.token.invalidate();
        }
        Ok((self, packet))
    }
    pub async fn write(mut self, packet: &Packet) -> Result<Self> {
        self.require_live()?;
        self.peer.write(packet).await?;
        if packet.kind() == Kind::CloseTransport {
            self.token.invalidate();
        }
        Ok(self)
    }
    /// Short synchronous Auth phase. This method never starts a verifier thread.
    pub fn prepare(
        self,
        auth: &mut Auth,
        expected: RecoveryGrant,
        packet: Packet,
    ) -> Result<PendingVerification> {
        self.require_live()?;
        if packet.kind() != Kind::EncryptedRequest {
            return Err(transport::Error::InvalidFrame.into());
        }
        let work = auth.prepare_recovery_verification(&self.token, expected, packet.payload())?;
        Ok(PendingVerification {
            channel: self,
            packet,
            work,
        })
    }
    fn require_live(&self) -> Result<()> {
        if self.token.is_live() && !self.peer.is_poisoned() {
            Ok(())
        } else {
            Err(rustsimhub_auth::Error::Forbidden.into())
        }
    }
}
/// The host must release its Auth mutex before polling verify. This type borrows no Auth.
pub struct PendingVerification {
    channel: RecoveryChannel,
    packet: Packet,
    work: RecoveryVerification,
}
impl PendingVerification {
    /// Core's bounded worker owns peer/packet/work until actual verifier completion.
    /// The Auth token stays with this caller future: cancellation invalidates it
    /// while the late worker still retains its peer and Auth work permit.
    pub async fn verify(self) -> Result<VerifiedChannel> {
        let Self {
            channel,
            packet,
            work,
        } = self;
        let RecoveryChannel { token, peer } = channel;
        let (peer, result) = peer
            .verify_blocking(packet, move |_, _| Ok(work.verify()))
            .await?;
        if !token.is_live() {
            return Err(rustsimhub_auth::Error::Forbidden.into());
        }
        Ok(VerifiedChannel {
            result: Some(result),
            channel: Some(RecoveryChannel { token, peer }),
        })
    }
}
/// Output remains opaque until finish rechecks the same Auth. Drop fences the token
/// first, then destroys result/permit before closing peer/releasing its slot.
pub struct VerifiedChannel {
    result: Option<RecoveryVerificationResult>,
    channel: Option<RecoveryChannel>,
}
impl Drop for VerifiedChannel {
    fn drop(&mut self) {
        if let Some(channel) = &self.channel {
            channel.token.invalidate();
        }
    }
}
impl VerifiedChannel {
    /// Short synchronous Auth phase, never call across an external await.
    pub fn finish(
        mut self,
        auth: &mut Auth,
    ) -> Result<(RecoveryChannel, AuthorizedRecoveryRequest)> {
        let channel = self.channel.take().ok_or(transport::Error::Closed)?;
        let result = self.result.take().ok_or(transport::Error::Closed)?;
        let authorized = auth.finish_recovery_verification(&channel.token, result)?;
        channel.require_live()?;
        Ok((channel, authorized))
    }
}
