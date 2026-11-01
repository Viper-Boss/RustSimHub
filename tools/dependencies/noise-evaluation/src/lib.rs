// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Isolated dependency/interop experiment, NOT SecureSessionPortV1 or an Auth
//! verifier. No device pin, signature domain, grant, Core, network or storage.
//! Never feed product credentials to this harness: Noise XX's first message
//! payload is plaintext. Public fixed keys below belong ONLY to upstream KATs.
use snow::{Builder, HandshakeState, TransportState};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const SUITE: &str = "Noise_XX_25519_AESGCM_SHA256";
pub const MAX_BODY: usize = 1024;
pub const MAX_WIRE: usize = 2048;
const PROLOGUE: &[u8] = b"isolated dependency evaluation; no RustSimHub application authentication";

enum State {
    Handshake(Box<HandshakeState>),
    Transport(TransportState),
    Closed,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct EvalSession {
    state: State,
    final_hash: Option<Vec<u8>>,
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl EvalSession {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new(initiator: bool) -> Result<Self, String> {
        let builder = Builder::new(SUITE.parse().map_err(|_| "suite")?);
        let key = builder.generate_keypair().map_err(|_| "entropy")?;
        let builder = builder
            .local_private_key(&key.private)
            .map_err(|_| "key")?
            .prologue(PROLOGUE)
            .map_err(|_| "prologue")?;
        let handshake = if initiator {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(|_| "handshake")?;
        Ok(Self {
            state: State::Handshake(Box::new(handshake)),
            final_hash: None,
        })
    }
    pub fn write_handshake(&mut self, body: &[u8]) -> Result<Vec<u8>, String> {
        let result = (|| {
            if body.len() > MAX_BODY {
                return Err("limit".into());
            }
            let State::Handshake(state) = &mut self.state else {
                return Err("state".into());
            };
            let mut output = vec![0; MAX_WIRE];
            let n = state
                .write_message(body, &mut output)
                .map_err(|_| "handshake")?;
            output.truncate(n);
            Ok(output)
        })();
        self.retire_on_error(result)
    }
    pub fn read_handshake(&mut self, wire: &[u8]) -> Result<Vec<u8>, String> {
        let result = (|| {
            if wire.is_empty() || wire.len() > MAX_WIRE {
                return Err("limit".into());
            }
            let State::Handshake(state) = &mut self.state else {
                return Err("state".into());
            };
            let mut output = vec![0; MAX_WIRE];
            let n = state
                .read_message(wire, &mut output)
                .map_err(|_| "handshake")?;
            if n > MAX_BODY {
                return Err("limit".into());
            }
            output.truncate(n);
            Ok(output)
        })();
        self.retire_on_error(result)
    }
    pub fn finish(&mut self) -> Result<(), String> {
        let old = std::mem::replace(&mut self.state, State::Closed);
        let State::Handshake(state) = old else {
            self.close();
            return Err("state".into());
        };
        if !state.is_handshake_finished() {
            self.close();
            return Err("unfinished".into());
        }
        let hash = state.get_handshake_hash().to_vec();
        match state.into_transport_mode() {
            Ok(transport) => {
                self.state = State::Transport(transport);
                self.final_hash = Some(hash);
                Ok(())
            }
            Err(_) => {
                self.close();
                Err("handshake".into())
            }
        }
    }
    pub fn handshake_hash(&self) -> Result<Vec<u8>, String> {
        self.final_hash.clone().ok_or_else(|| "unfinished".into())
    }
    pub fn write_transport(&mut self, body: &[u8]) -> Result<Vec<u8>, String> {
        let result = (|| {
            if body.len() > MAX_BODY {
                return Err("limit".into());
            }
            let State::Transport(state) = &mut self.state else {
                return Err("state".into());
            };
            let mut output = vec![0; MAX_WIRE];
            let n = state
                .write_message(body, &mut output)
                .map_err(|_| "transport")?;
            output.truncate(n);
            Ok(output)
        })();
        self.retire_on_error(result)
    }
    pub fn read_transport(&mut self, wire: &[u8]) -> Result<Vec<u8>, String> {
        let result = (|| {
            if wire.len() < 16 || wire.len() > MAX_WIRE {
                return Err("limit".into());
            }
            let State::Transport(state) = &mut self.state else {
                return Err("state".into());
            };
            let mut output = vec![0; MAX_WIRE];
            let n = state
                .read_message(wire, &mut output)
                .map_err(|_| "transport")?;
            if n > MAX_BODY {
                return Err("limit".into());
            }
            output.truncate(n);
            Ok(output)
        })();
        self.retire_on_error(result)
    }
    /// Local Drop only; neither snow nor this wrapper promises secret wiping.
    pub fn close(&mut self) {
        self.state = State::Closed;
        self.final_hash = None;
    }
    pub fn closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }
}
impl EvalSession {
    fn retire_on_error<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        if result.is_err() {
            self.close();
        }
        result
    }
}
impl Drop for EvalSession {
    fn drop(&mut self) {
        self.close();
    }
}

/// Maintainer-distributed snow 0.10.0 KAT, byte-for-byte, not self-generated
/// expected output or an official security certification. Fixed test keys are
/// never exposed through EvalSession's random constructor.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn verify_public_vectors() -> Result<u32, String> {
    let document: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/snow-xx-aesgcm-sha256.json"))
            .map_err(|_| "vector fixture")?;
    let vectors = document["vectors"].as_array().ok_or("vector fixture")?;
    if vectors.len() != 1 {
        return Err("vector count".into());
    }
    let mut checked = 0;
    for v in vectors {
        if v["protocol_name"] != SUITE {
            return Err("vector suite".into());
        }
        let decode = |name: &str| -> Result<Vec<u8>, String> {
            hex::decode(v[name].as_str().ok_or("vector field")?).map_err(|_| "vector hex".into())
        };
        let ik = decode("init_static")?;
        let ie = decode("init_ephemeral")?;
        let rk = decode("resp_static")?;
        let re = decode("resp_ephemeral")?;
        let ip = decode("init_prologue")?;
        let rp = decode("resp_prologue")?;
        let init = Builder::new(SUITE.parse().map_err(|_| "suite")?)
            .local_private_key(&ik)
            .map_err(|_| "vector key")?
            .fixed_ephemeral_key_for_testing_only(&ie)
            .prologue(&ip)
            .map_err(|_| "vector prologue")?
            .build_initiator()
            .map_err(|_| "vector build")?;
        let resp = Builder::new(SUITE.parse().map_err(|_| "suite")?)
            .local_private_key(&rk)
            .map_err(|_| "vector key")?
            .fixed_ephemeral_key_for_testing_only(&re)
            .prologue(&rp)
            .map_err(|_| "vector prologue")?
            .build_responder()
            .map_err(|_| "vector build")?;
        let mut handshakes = [init, resp];
        let messages = v["messages"].as_array().ok_or("vector messages")?;
        if messages.len() != 5 {
            return Err("vector count".into());
        }
        let mut cipher = [0u8; MAX_WIRE];
        let mut plain = [0u8; MAX_WIRE];
        for (index, message) in messages.iter().take(3).enumerate() {
            let payload = hex::decode(message["payload"].as_str().ok_or("vector field")?)
                .map_err(|_| "vector hex")?;
            let expected = hex::decode(message["ciphertext"].as_str().ok_or("vector field")?)
                .map_err(|_| "vector hex")?;
            let n = handshakes[index % 2]
                .write_message(&payload, &mut cipher)
                .map_err(|_| "KAT write")?;
            let m = handshakes[1 - index % 2]
                .read_message(&cipher[..n], &mut plain)
                .map_err(|_| "KAT read")?;
            if cipher[..n] != expected || plain[..m] != payload {
                return Err("KAT mismatch".into());
            }
            checked += 1;
        }
        if !handshakes.iter().all(|h| h.is_handshake_finished())
            || handshakes[0].get_handshake_hash() != handshakes[1].get_handshake_hash()
        {
            return Err("KAT handshake".into());
        }
        let [init, resp] = handshakes;
        let mut transports = [
            init.into_transport_mode().map_err(|_| "KAT mode")?,
            resp.into_transport_mode().map_err(|_| "KAT mode")?,
        ];
        for (index, message) in messages.iter().enumerate().skip(3) {
            let payload = hex::decode(message["payload"].as_str().ok_or("vector field")?)
                .map_err(|_| "vector hex")?;
            let expected = hex::decode(message["ciphertext"].as_str().ok_or("vector field")?)
                .map_err(|_| "vector hex")?;
            let n = transports[index % 2]
                .write_message(&payload, &mut cipher)
                .map_err(|_| "KAT write")?;
            let m = transports[1 - index % 2]
                .read_message(&cipher[..n], &mut plain)
                .map_err(|_| "KAT read")?;
            if cipher[..n] != expected || plain[..m] != payload {
                return Err("KAT mismatch".into());
            }
            checked += 1;
        }
    }
    Ok(checked)
}

#[cfg(test)]
mod tests;
