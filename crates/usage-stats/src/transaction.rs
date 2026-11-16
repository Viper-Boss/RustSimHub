// SPDX-License-Identifier: AGPL-3.0-only
//! Candidate pure local encoder for exactly this product's three subscriptions.
//! No I/O, nonce lookup, key creation, arbitrary-message signer or submission.
//! Empty attributes are an explicit candidate policy, not the SDK default.
use crate::{
    identity::StatisticsIdentity,
    protocol::{CheckInIntent, Clock, EnabledPolicy, RENEW_INTERVAL_SECS},
};
use sha2::{Digest, Sha256};

pub const MAX_TRANSACTION_BYTES: usize = 768;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    WrongIdentity,
    Clock,
    StaleIntent,
    Encoding,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics subscription preparation: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Immutable public transaction material. Construction remains purpose-fixed;
/// callers cannot substitute a signature, fee, attributes or subscription.
pub struct PreparedSubscription {
    intent: CheckInIntent,
    nonce: u64,
    unsigned: Vec<u8>,
    signing_hash: [u8; 32],
    signature: [u8; 64],
    transaction: Vec<u8>,
    digest: [u8; 32],
}
impl std::fmt::Debug for PreparedSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSubscription")
            .field("intent", &self.intent)
            .field("nonce", &self.nonce)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
impl PreparedSubscription {
    pub fn intent(&self) -> CheckInIntent {
        self.intent
    }
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
    pub fn unsigned_bytes(&self) -> &[u8] {
        &self.unsigned
    }
    pub fn signing_hash(&self) -> &[u8; 32] {
        &self.signing_hash
    }
    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }
    pub fn transaction_bytes(&self) -> &[u8] {
        &self.transaction
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

fn varint(mut n: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    while n >= 128 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn field(tag: u8, data: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(varint(data.len() as u64));
    out.extend_from_slice(data);
    out
}
fn chain_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = if data.len() < 253 {
        vec![data.len() as u8]
    } else {
        let n = u16::try_from(data.len()).map_err(|_| Error::Encoding)?;
        let mut prefix = vec![253];
        prefix.extend(n.to_le_bytes());
        prefix
    };
    out.extend_from_slice(data);
    Ok(out)
}

impl StatisticsIdentity {
    /// Caller must authenticate the current Owner/realm, consent generation and
    /// finite grant. This pure library policy is not an authentication proof.
    /// Nonce provenance belongs to the finite adapter, never an implicit RPC.
    pub fn prepare_subscription<C: Clock + ?Sized>(
        &self,
        policy: EnabledPolicy,
        intent: CheckInIntent,
        nonce: u64,
        clock: &C,
    ) -> Result<Option<PreparedSubscription>, Error> {
        if !policy.is_enabled() {
            return Ok(None);
        }
        if intent.public_key() != self.public_key() {
            return Err(Error::WrongIdentity);
        }
        let now = clock.now_secs().map_err(|_| Error::Clock)?;
        if now / RENEW_INTERVAL_SECS != intent.renewal_period() {
            return Err(Error::StaleIntent);
        }
        // CheckInIntent is a strict closed contract: fixed topics/durations and
        // exact canonical metadata. No arbitrary caller-supplied JSON is signed.
        let meta = intent.to_json().map_err(|_| Error::Encoding)?;
        let mut payload = field(0x0a, self.public_key().as_bytes());
        payload.extend(field(0x1a, intent.topic().as_bytes()));
        payload.push(0x28);
        payload.extend(varint(u64::from(intent.subscription_blocks())));
        payload.extend(field(0x32, &meta));
        let mut unsigned = 6_u32.to_le_bytes().to_vec();
        unsigned.extend(chain_bytes(&payload)?);
        unsigned.extend(nonce.to_le_bytes());
        unsigned.extend(0_u64.to_le_bytes()); // fixed fee
        unsigned.push(0); // explicitly empty attributes, before any hash/signature
        let signing_hash = <[u8; 32]>::from(Sha256::digest(&unsigned));
        let digest = <[u8; 32]>::from(Sha256::digest(signing_hash));
        let signature = self.subscription_signature(&signing_hash);
        // Protobuf transaction serialization is distinct from the unsigned
        // chain serialization above. Omit zero nonce as the fixed SDK does.
        let mut pl = vec![0x08, 6];
        pl.extend(field(0x12, &payload));
        let mut unsigned_pb = field(0x0a, &pl);
        if nonce != 0 {
            unsigned_pb.push(0x10);
            unsigned_pb.extend(varint(nonce));
        }
        let mut code = vec![32];
        code.extend(self.public_key().as_bytes());
        code.push(0xac);
        let mut parameter = vec![64];
        parameter.extend(signature);
        let mut program = field(0x0a, &code);
        program.extend(field(0x12, &parameter));
        let mut transaction = field(0x0a, &unsigned_pb);
        transaction.extend(field(0x12, &program));
        if transaction.len() > MAX_TRANSACTION_BYTES {
            return Err(Error::Encoding);
        }
        Ok(Some(PreparedSubscription {
            intent,
            nonce,
            unsigned,
            signing_hash,
            signature,
            transaction,
            digest,
        }))
    }
}
