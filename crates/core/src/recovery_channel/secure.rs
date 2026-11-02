// SPDX-License-Identifier: AGPL-3.0-only
//! Real Noise handshake on the original kernel-admitted descriptor, followed by
//! the existing same-Core application host. No listener or default activation.
use super::*;
use rustsimhub_auth::secure_session::SecureRecoverySessions;
use rustsimhub_recovery_session::{ClientHello, Purpose};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::{timeout_at, Instant};

pub struct SecureRecoveryChannel {
    channel: RecoveryChannel,
    expected: Box<RecoveryGrant>,
    sessions: Arc<SecureRecoverySessions>,
}
pub enum SecureApplicationStep {
    Continue(SecureRecoveryChannel),
    Closed,
}
struct Sealer(Arc<SecureRecoverySessions>);
impl ApplicationReplySealerV1 for Sealer {
    fn seal(&self, request: SealingRequest) -> rustsimhub_auth::Result<SealedReply> {
        let ciphertext = self.0.seal_response(request.scope(), request.plaintext())?;
        request.finish_from_trusted_session(ciphertext)
    }
}
async fn work<T, F>(
    channel: RecoveryChannel,
    deadline: Instant,
    action: F,
) -> Result<(RecoveryChannel, T)>
where
    T: Send + 'static,
    F: FnOnce(&mut RecoveryChannel) -> Result<T> + Send + 'static,
{
    // The dedicated worker owns the real descriptor/admission slot until it
    // drains. The caller's independent token fence invalidates late output.
    let (channel, result) =
        transport::run_owned(channel, deadline, move |channel| Ok(action(channel))).await?;
    let result = result?;
    channel.require_live()?;
    Ok((channel, result))
}
fn handshake_packet(packet: &Packet) -> Result<&[u8]> {
    if packet.kind() != Kind::Handshake {
        return Err(transport::Error::InvalidFrame.into());
    }
    Ok(packet.payload())
}
impl RecoveryChannel {
    pub async fn establish_secure(
        self,
        auth: Arc<Mutex<Auth>>,
        sessions: Arc<SecureRecoverySessions>,
    ) -> Result<SecureRecoveryChannel> {
        self.require_live()?;
        let mut cancellation = Some(self.token.disconnect_guard());
        let monitor = self.peer.disconnect_monitor()?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let result = tokio::select! {
            biased;
            _=monitor.wait_closed()=>Err(transport::Error::Closed.into()),
            result=timeout_at(deadline,self.establish_inner(auth,sessions,deadline))=>result.map_err(|_| Error::Transport(transport::Error::Deadline))?,
        };
        if result.is_ok() {
            if let Some(guard) = cancellation.take() {
                guard.disarm();
            }
        }
        result
    }
    async fn establish_inner(
        self,
        auth: Arc<Mutex<Auth>>,
        sessions: Arc<SecureRecoverySessions>,
        deadline: Instant,
    ) -> Result<SecureRecoveryChannel> {
        let (channel, packet) = self.read().await?;
        let hello = ClientHello::parse(handshake_packet(&packet)?)
            .map_err(|_| transport::Error::InvalidFrame)?;
        if hello.purpose != Purpose::BleRecovery {
            return Err(transport::Error::Denied.into());
        }
        let a = auth.clone();
        let (channel, (expected, preparation)) = work(channel, deadline, move |channel| {
            let a = a.try_lock().map_err(|_| rustsimhub_auth::Error::Busy)?;
            let grant =
                a.secure_recovery_grant(&channel.token, hello.client_key, hello.generation)?;
            let preparation =
                a.prepare_secure_recovery_handshake(&channel.token, grant.clone(), hello.nonce)?;
            Ok((grant, (preparation, hello.noise)))
        })
        .await?;
        let s = sessions.clone();
        let (channel, noise) = work(channel, deadline, move |channel| {
            let (preparation, first) = preparation;
            s.begin_prepared(&channel.token, preparation)?;
            channel.session_lifetime = Some(Box::new(s.lifetime(&channel.token)?));
            s.read_handshake(&channel.token, &first)?;
            s.write_handshake(&channel.token).map_err(Into::into)
        })
        .await?;
        let channel = channel.write(&Packet::new(Kind::Handshake, noise)?).await?;
        let (channel, packet) = channel.read().await?;
        handshake_packet(&packet)?;
        let s = sessions.clone();
        let (channel, signing) = work(channel, deadline, move |channel| {
            s.read_handshake(&channel.token, packet.payload())?;
            s.device_signing_work(&channel.token).map_err(Into::into)
        })
        .await?;
        let a = auth.clone();
        let (channel, (signing, signature)) = work(channel, deadline, move |channel| {
            let a = a.try_lock().map_err(|_| rustsimhub_auth::Error::Busy)?;
            let signature = a.sign_secure_recovery_binding(&channel.token, &signing)?;
            Ok((signing, signature))
        })
        .await?;
        let s = sessions.clone();
        let (channel, binding) = work(channel, deadline, move |channel| {
            s.seal_device_binding(&channel.token, signing, &signature)
                .map_err(Into::into)
        })
        .await?;
        let channel = channel
            .write(&Packet::new(Kind::Handshake, binding)?)
            .await?;
        let (channel, packet) = channel.read().await?;
        handshake_packet(&packet)?;
        let s = sessions.clone();
        let (channel, ()) = work(channel, deadline, move |channel| {
            s.read_client_binding(&channel.token, packet.payload())
                .map_err(Into::into)
        })
        .await?;
        // Recheck the actual grant after the client has proved its private key,
        // and before returning the encrypted binding ACK. No Auth lock during DH.
        let a = auth.clone();
        let s = sessions.clone();
        let (channel, ()) = work(channel, deadline, move |channel| {
            let binding = s.binding(&channel.token)?;
            a.try_lock()
                .map_err(|_| rustsimhub_auth::Error::Busy)?
                .check_secure_recovery_context(&channel.token, &binding)?;
            Ok(())
        })
        .await?;
        let s = sessions.clone();
        let (channel, ack) = work(channel, deadline, move |channel| {
            s.write_binding_ack(&channel.token).map_err(Into::into)
        })
        .await?;
        let channel = channel.write(&Packet::new(Kind::Handshake, ack)?).await?;
        Ok(SecureRecoveryChannel {
            channel,
            expected: Box::new(expected),
            sessions,
        })
    }
}
impl SecureRecoveryChannel {
    pub async fn dispatch(self, auth: Arc<Mutex<Auth>>) -> Result<SecureApplicationStep> {
        let Self {
            channel,
            expected,
            sessions,
        } = self;
        let sealer = Some(Arc::new(Sealer(sessions.clone())) as Arc<dyn ApplicationReplySealerV1>);
        match channel
            .dispatch_application_v1(auth, expected.as_ref().clone(), sealer)
            .await?
        {
            ApplicationStep::Continue(channel) => Ok(SecureApplicationStep::Continue(Self {
                channel,
                expected,
                sessions,
            })),
            ApplicationStep::Closed => Ok(SecureApplicationStep::Closed),
        }
    }
}
