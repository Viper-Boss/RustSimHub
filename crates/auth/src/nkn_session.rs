// SPDX-License-Identifier: AGPL-3.0-only
//! Ordinary registered Core identities over purpose-separated Noise. Network
//! membership never creates a Principal, recovery grant, Owner or normal token.
use crate::{key_valid, token_digest, Auth, Error, Result, Token};
use ed25519_dalek::Signature;
use rustsimhub_recovery_session::{Action, Context, CoreBinding, Purpose, Session as Cipher};
use rustsimhub_storage::NknAuthoritySnapshot;
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
mod membership;
use membership::Member;
pub use membership::MembershipKey;
fn crypto(_: rustsimhub_recovery_session::Error) -> Error {
    Error::Forbidden
}

#[derive(Clone, PartialEq, Eq)]
struct SourceBinding {
    instance: String,
    device: [u8; 32],
    connection: [u8; 32],
    transport: [u8; 32],
    authority: NknAuthoritySnapshot,
}
/// Full opaque local association; cannot be supplied by network JSON.
#[derive(Clone, PartialEq, Eq)]
pub struct NknBinding {
    source: SourceBinding,
    member: Member,
    session: [u8; 32],
}
#[derive(Clone)]
pub struct NknScope {
    binding: NknBinding,
    live: Arc<AtomicBool>,
    deadline: Instant,
    proved: Instant,
    issued: Arc<AtomicBool>,
}
impl NknScope {
    pub fn binding(&self) -> &NknBinding {
        &self.binding
    }
    pub fn principal(&self) -> &rustsimhub_policy::Principal {
        &self.binding.source.authority.principal
    }
    pub fn transport_nonce(&self) -> &[u8; 32] {
        &self.binding.source.transport
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn invalidate(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
            && self.binding.member.live.load(Ordering::Acquire)
            && Instant::now() < self.deadline
    }
}
pub struct NknPreparation {
    source: SourceBinding,
    context: Context,
    created: Instant,
    deadline: Instant,
}
pub struct NknDeviceBinding {
    source: SourceBinding,
    message: Vec<u8>,
    deadline: Instant,
}
pub struct NknMembershipAdmission {
    request: [u8; 16],
    binding: NknBinding,
}
pub struct NknSession {
    source: SourceBinding,
    cipher: Cipher,
    origin: Instant,
    deadline: Instant,
    live: Arc<AtomicBool>,
    scope: Option<NknScope>,
    transcript: Option<Vec<u8>>,
}
impl Drop for NknSession {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
        self.cipher.close();
    }
}
impl Auth {
    /// Real purpose2 Noise just verified the current registered client's
    /// Ed25519 key and independent signed membership. Reuse the normal session
    /// allocator with that exact Principal and a fence on its original source.
    /// Only one token per fresh proof; this cannot refresh authentication again.
    pub fn issue_nkn_token(&mut self, scope: &NknScope) -> Result<Token> {
        self.check_nkn_scope(scope)?;
        if Instant::now() >= scope.proved + Duration::from_secs(10) {
            return Err(Error::Expired);
        }
        self.rate_limit(Instant::now())?;
        scope
            .issued
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Replay)?;
        let token = self.issue_verified_session(scope.principal().clone())?;
        let digest = token_digest(token.expose())?;
        if let Err(e) = self.check_nkn_scope(scope) {
            self.sessions.remove(&digest);
            return Err(e);
        }
        let session = self.sessions.get_mut(&digest).ok_or(Error::Forbidden)?;
        session.deadline = session.deadline.min(scope.deadline);
        session.nkn = Some(scope.clone());
        Ok(token)
    }
    /// Called only by the Core's kernel-admitted NKN source with its own fresh
    /// transport nonce and original Root lease deadline. No I/O or DH here.
    pub fn prepare_nkn_session(
        &self,
        transport: [u8; 32],
        client: [u8; 32],
        generation: u64,
        nonce: [u8; 32],
        parent_deadline: Instant,
    ) -> Result<NknPreparation> {
        if transport == [0; 32] {
            return Err(Error::Forbidden);
        }
        let authority = self.store.nkn_authority_snapshot(client)?;
        if authority.principal.generation != generation {
            return Err(Error::Forbidden);
        }
        let mut connection = [0; 32];
        getrandom::fill(&mut connection).map_err(|_| Error::Entropy)?;
        let created = Instant::now();
        let deadline = parent_deadline.min(created + Duration::from_secs(300));
        if deadline <= created {
            return Err(Error::Expired);
        }
        let context = Context::new(
            Purpose::NknControl,
            self.public_key(),
            client,
            generation,
            nonce,
        )
        .map_err(crypto)?;
        Ok(NknPreparation {
            source: SourceBinding {
                instance: self.instance.clone(),
                device: self.public_key(),
                connection,
                transport,
                authority,
            },
            context,
            created,
            deadline,
        })
    }
    fn check_nkn_source(&self, source: &SourceBinding) -> Result<()> {
        if source.instance != self.instance
            || source.device != self.public_key()
            || self
                .store
                .nkn_authority_snapshot(source.authority.principal.public_key)?
                != source.authority
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub fn sign_nkn_device_binding(&self, work: &NknDeviceBinding) -> Result<Vec<u8>> {
        self.check_nkn_source(&work.source)?;
        if Instant::now() >= work.deadline {
            return Err(Error::Expired);
        }
        Ok(self.identity.sign(&work.message))
    }
    pub fn check_nkn_session(&self, session: &NknSession) -> Result<()> {
        session.now()?;
        self.check_nkn_source(&session.source)
    }
    pub fn check_nkn_scope(&self, scope: &NknScope) -> Result<()> {
        if !scope.is_live() {
            return Err(Error::Forbidden);
        }
        self.check_nkn_source(&scope.binding.source)
    }
    /// Every token-bearing normal request must use a token created for this
    /// same opaque association, never a stolen token from another connection.
    pub fn check_nkn_token(&mut self, scope: &NknScope, token: &str) -> Result<()> {
        self.check_nkn_scope(scope)?;
        self.session_principal(token)?;
        let bound = self
            .sessions
            .get(&token_digest(token)?)
            .and_then(|s| s.nkn.as_ref())
            .ok_or(Error::Forbidden)?;
        if bound.binding != scope.binding || !Arc::ptr_eq(&bound.live, &scope.live) {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
impl NknSession {
    /// Expensive standard Noise construction occurs outside Auth's mutex.
    pub fn begin(preparation: NknPreparation) -> Result<Self> {
        let NknPreparation {
            source,
            context,
            created: origin,
            deadline,
        } = preparation;
        if Instant::now() >= origin + Duration::from_secs(5) {
            return Err(Error::Expired);
        }
        let binding = CoreBinding::new(
            Sha256::digest(source.instance.as_bytes()).into(),
            source.connection,
            source.transport,
        )
        .map_err(crypto)?;
        let cap = u64::try_from(deadline.duration_since(origin).as_millis())
            .map_err(|_| Error::Expired)?;
        let cipher = Cipher::device(context, binding, 0, cap).map_err(crypto)?;
        Ok(Self {
            source,
            cipher,
            origin,
            deadline,
            live: Arc::new(AtomicBool::new(true)),
            scope: None,
            transcript: None,
        })
    }
    fn now(&self) -> Result<u64> {
        if !self.live.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
            || self.scope.as_ref().is_some_and(|s| !s.is_live())
        {
            return Err(Error::Expired);
        }
        u64::try_from(self.origin.elapsed().as_millis()).map_err(|_| Error::Expired)
    }
    fn checked<T>(&mut self, action: impl FnOnce(&mut Self, u64) -> Result<T>) -> Result<T> {
        let result = self.now().and_then(|n| action(self, n));
        if result.is_err() {
            self.live.store(false, Ordering::Release);
            self.cipher.close();
        }
        result
    }
    pub fn read_handshake(&mut self, wire: &[u8]) -> Result<()> {
        self.checked(|s, n| s.cipher.read_handshake(wire, n).map_err(crypto))
    }
    pub fn write_handshake(&mut self) -> Result<Vec<u8>> {
        self.checked(|s, n| s.cipher.write_handshake(n).map_err(crypto))
    }
    pub fn device_binding_work(&mut self) -> Result<NknDeviceBinding> {
        self.checked(|s, n| {
            Ok(NknDeviceBinding {
                source: s.source.clone(),
                message: s.cipher.binding_message(n).map_err(crypto)?,
                deadline: s.deadline.min(s.origin + Duration::from_secs(10)),
            })
        })
    }
    pub fn seal_device_binding(
        &mut self,
        work: NknDeviceBinding,
        signature: &[u8],
    ) -> Result<Vec<u8>> {
        self.checked(|s, n| {
            if work.source != s.source
                || Instant::now() >= work.deadline
                || work.message != s.cipher.binding_message(n).map_err(crypto)?
            {
                return Err(Error::Forbidden);
            }
            s.transcript = Some(work.message);
            s.cipher.seal_binding(signature, n).map_err(crypto)
        })
    }
    pub fn read_client_binding(&mut self, wire: &[u8]) -> Result<()> {
        self.checked(|s, n| s.cipher.open_binding(wire, n).map_err(crypto))
    }
    pub fn write_binding_ack(&mut self) -> Result<Vec<u8>> {
        self.checked(|s, n| s.cipher.seal_binding_ack(n).map_err(crypto))
    }
    /// Root-key proof signs the exact device-binding transcript (which includes
    /// purpose, client, pin, nonce, generation, 3 Core nonces and final Noise hash),
    /// plus network and membership credential. Not an upstream network envelope.
    pub fn membership_message(
        device_binding: &[u8],
        network: &str,
        root: &[u8; 32],
        proof: &[u8; 32],
    ) -> Result<Vec<u8>> {
        if device_binding.len() > 512
            || device_binding.is_empty()
            || network.len() != 33
            || !network.starts_with("nkgnet_")
        {
            return Err(Error::InvalidInput);
        }
        let mut bytes = b"RustSimHub/NknMembershipBinding/v1\0".to_vec();
        bytes.extend_from_slice(&(device_binding.len() as u16).to_be_bytes());
        bytes.extend_from_slice(device_binding);
        bytes.extend_from_slice(network.as_bytes());
        bytes.extend_from_slice(root);
        bytes.extend_from_slice(proof);
        Ok(bytes)
    }
    /// The first purpose2 encrypted request is the fixed 129-byte membership
    /// proof. Only after real HMAC and root-key signature checks may normal Auth
    /// begin. The host returns a source-bound token in its encrypted bootstrap
    /// response; that response is not a business-operation success receipt.
    pub fn verify_membership(
        &mut self,
        key: &MembershipKey,
        wire: &[u8],
    ) -> Result<NknMembershipAdmission> {
        self.checked(|s, n| {
            if s.scope.is_some() || Instant::now() >= s.origin + Duration::from_secs(10) {
                return Err(Error::Forbidden);
            }
            let request = s.cipher.open_request(wire, n).map_err(crypto)?;
            if request.action != Action::NormalCore
                || request.body.len() != 129
                || request.body[0] != 1
            {
                return Err(Error::Forbidden);
            }
            let root: [u8; 32] = request.body[1..33]
                .try_into()
                .map_err(|_| Error::Forbidden)?;
            if root == s.source.authority.principal.public_key || root == s.source.device {
                return Err(Error::Forbidden);
            }
            let proof: [u8; 32] = request.body[33..65]
                .try_into()
                .map_err(|_| Error::Forbidden)?;
            let member = key.verify(root, &proof)?;
            // Device binding bytes were generated after Noise and are stable;
            // the cipher exposes them only while binding. Store that work's
            // canonical bytes explicitly in the session for this next phase.
            let canonical = s.device_transcript()?;
            let message = Self::membership_message(&canonical, key.network_id(), &root, &proof)?;
            key_valid(&root)?
                .verify_strict(
                    &message,
                    &Signature::from_slice(&request.body[65..]).map_err(|_| Error::Forbidden)?,
                )
                .map_err(|_| Error::Forbidden)?;
            s.scope = Some(NknScope {
                binding: NknBinding {
                    source: s.source.clone(),
                    member,
                    session: *s.cipher.session_id().map_err(crypto)?,
                },
                live: s.live.clone(),
                deadline: s.deadline,
                proved: s.origin,
                issued: Arc::new(AtomicBool::new(false)),
            });
            Ok(NknMembershipAdmission {
                request: request.id,
                binding: s.scope.as_ref().ok_or(Error::Forbidden)?.binding.clone(),
            })
        })
    }
    pub fn seal_membership_reply(
        &mut self,
        admission: NknMembershipAdmission,
        body: &[u8],
    ) -> Result<Vec<u8>> {
        self.checked(|s, n| {
            if &admission.binding != s.scope.as_ref().ok_or(Error::Forbidden)?.binding() {
                return Err(Error::Forbidden);
            }
            s.cipher
                .seal_response(admission.request, body, n)
                .map_err(crypto)
        })
    }
    fn device_transcript(&self) -> Result<Vec<u8>> {
        self.transcript.clone().ok_or(Error::Forbidden)
    }
    pub fn scope(&self) -> Result<NknScope> {
        self.now()?;
        self.scope.clone().ok_or(Error::Forbidden)
    }
    pub fn open_record(&mut self, wire: &[u8]) -> Result<rustsimhub_recovery_session::Request> {
        self.checked(|s, n| {
            s.scope.as_ref().ok_or(Error::Forbidden)?;
            s.cipher.open_request(wire, n).map_err(crypto)
        })
    }
    pub fn seal_reply(&mut self, id: [u8; 16], body: &[u8]) -> Result<Vec<u8>> {
        self.checked(|s, n| {
            s.scope.as_ref().ok_or(Error::Forbidden)?;
            s.cipher.seal_response(id, body, n).map_err(crypto)
        })
    }
}
#[cfg(test)]
mod tests;
