// SPDX-License-Identifier: AGPL-3.0-only
//! One-time pairing: X25519, HKDF-SHA256 and AES-256-GCM, as in the app's
//! `SamsungPairingCrypto.seal`.
use crate::Error;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// How long a pairing window stays open, as on Android.
pub const PAIRING_LIFETIME_SECONDS: i64 = 120;

fn random(bytes: &mut [u8]) -> Result<(), Error> {
    getrandom::fill(bytes).map_err(|_| Error::Random)
}

/// One short-lived pairing window: a random session ID, a six-digit code shown
/// only on the owner's page, and an ephemeral X25519 key. The app seals its
/// registration (including its VoWLAN secret) to that key.
pub struct PairingSession {
    id: String,
    code: String,
    expires_at: i64,
    private: StaticSecret,
    used: bool,
}
impl PairingSession {
    /// `now` is Unix seconds.
    pub fn new(now: i64) -> Result<Self, Error> {
        let mut key = Zeroizing::new([0u8; 32]);
        random(&mut key[..])?;
        // Rejection sampling keeps the six digits uniform.
        let code = loop {
            let mut raw = [0u8; 4];
            random(&mut raw)?;
            let value = u32::from_le_bytes(raw);
            if value < 4_294_000_000 {
                break value % 1_000_000;
            }
        };
        let mut id = [0u8; 16];
        random(&mut id)?;
        id[6] = id[6] & 0x0f | 0x40;
        id[8] = id[8] & 0x3f | 0x80;
        let h = crate::hex(&id);
        Ok(Self {
            id: format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            ),
            code: format!("{code:06}"),
            expires_at: now + PAIRING_LIFETIME_SECONDS,
            private: StaticSecret::from(*key),
            used: false,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Shown to the owner only; never sent to the app.
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    /// The raw 32-byte X25519 public key.
    pub fn public_key(&self) -> [u8; 32] {
        PublicKey::from(&self.private).to_bytes()
    }
    /// Decrypts a claim. A session opens successfully only once; a wrong code
    /// leaves it usable until it expires, as on Android.
    pub fn open(
        &mut self,
        client_public_key: &[u8],
        sealed: &[u8],
        now: i64,
    ) -> Result<Vec<u8>, Error> {
        if self.used {
            return Err(Error::PairingUsed);
        }
        if now > self.expires_at {
            return Err(Error::PairingExpired);
        }
        let plaintext = open_sealed_registration(
            &self.private,
            client_public_key,
            sealed,
            &self.id,
            &self.code,
        )?;
        self.used = true;
        Ok(plaintext)
    }
}

fn derive_key(
    private: &StaticSecret,
    peer_public: &[u8],
    session_id: &str,
    code: &str,
) -> Result<Zeroizing<[u8; 32]>, Error> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) || session_id.is_empty() {
        return Err(Error::Invalid);
    }
    let peer: [u8; 32] = peer_public.try_into().map_err(|_| Error::Authentication)?;
    let shared = private.diffie_hellman(&PublicKey::from(peer));
    // A low-order point yields an all-zero secret that any code would "open".
    if !shared.was_contributory() {
        return Err(Error::Authentication);
    }
    hkdf_key(shared.as_bytes(), session_id, code)
}

pub(crate) fn hkdf_key(
    shared: &[u8],
    session_id: &str,
    code: &str,
) -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(session_id.as_bytes()), shared)
        .expand(format!("AirSIMPair/v1:{code}").as_bytes(), &mut key[..])
        .map_err(|_| Error::Invalid)?;
    Ok(key)
}

/// The receiving half of the app's seal: `nonce(12) || ciphertext || tag(16)`,
/// with the session ID as associated data.
pub fn open_sealed_registration(
    private: &StaticSecret,
    client_public_key: &[u8],
    sealed: &[u8],
    session_id: &str,
    code: &str,
) -> Result<Vec<u8>, Error> {
    let key = derive_key(private, client_public_key, session_id, code)?;
    if sealed.len() < 12 + 16 {
        return Err(Error::Authentication);
    }
    Aes256Gcm::new_from_slice(&key[..])
        .map_err(|_| Error::Invalid)?
        .decrypt(
            Nonce::from_slice(&sealed[..12]),
            Payload {
                msg: &sealed[12..],
                aad: session_id.as_bytes(),
            },
        )
        .map_err(|_| Error::Authentication)
}

/// The app's side; the gateway uses it only in tests.
pub fn seal_registration(
    client_private: &StaticSecret,
    server_public_key: &[u8],
    registration: &[u8],
    session_id: &str,
    code: &str,
    nonce: [u8; 12],
) -> Result<Vec<u8>, Error> {
    let key = derive_key(client_private, server_public_key, session_id, code)?;
    let mut sealed = nonce.to_vec();
    sealed.extend(
        Aes256Gcm::new_from_slice(&key[..])
            .map_err(|_| Error::Invalid)?
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: registration,
                    aad: session_id.as_bytes(),
                },
            )
            .map_err(|_| Error::Invalid)?,
    );
    Ok(sealed)
}
