// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit synthetic crypto interoperability peer. No Auth/Owner/Core business
//! receipt or production identity. Never installed or selected by a daemon.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_recovery_session::{ClientHello, Context, CoreBinding, Session};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use zeroize::Zeroizing;
fn bytes(value: &Value, field: &str) -> Result<Vec<u8>, ()> {
    let text = value[field].as_str().ok_or(())?;
    if text.len() > 4096 {
        return Err(());
    }
    hex::decode(text).map_err(|_| ())
}
fn random<const N: usize>() -> Result<[u8; N], ()> {
    let mut value = [0; N];
    getrandom::fill(&mut value).map_err(|_| ())?;
    Ok(value)
}
fn main() {
    let mut key: Option<SigningKey> = None;
    let mut session: Option<Session> = None;
    let stdin = io::stdin();
    for line in stdin.lock().lines().take(4096) {
        let Ok(line) = line else {
            break;
        };
        if line.len() > 8192 {
            break;
        }
        let result = (|| -> Result<Value, ()> {
            let value: Value = serde_json::from_str(&line).map_err(|_| ())?;
            let now = value["now"].as_u64().unwrap_or(0);
            match value["op"].as_str().ok_or(())? {
                "reset" => {
                    session = None;
                    let seed = Zeroizing::new(random::<32>()?);
                    let created = SigningKey::from_bytes(&seed);
                    let pin = hex::encode(created.verifying_key().to_bytes());
                    key = Some(created);
                    Ok(json!({"device_pin":pin,"scope":"synthetic_crypto_only"}))
                }
                "hello" => {
                    if session.is_some() {
                        return Err(());
                    }
                    let hello = ClientHello::parse(&bytes(&value, "wire")?).map_err(|_| ())?;
                    let key = key.as_ref().ok_or(())?;
                    let context = Context::new(
                        hello.purpose,
                        key.verifying_key().to_bytes(),
                        hello.client_key,
                        hello.generation,
                        hello.nonce,
                    )
                    .map_err(|_| ())?;
                    let binding =
                        CoreBinding::new(random::<32>()?, random::<32>()?, random::<32>()?)
                            .map_err(|_| ())?;
                    let mut created =
                        Session::device(context, binding, now, now.checked_add(300_000).ok_or(())?)
                            .map_err(|_| ())?;
                    created.read_handshake(&hello.noise, now).map_err(|_| ())?;
                    let reply = created.write_handshake(now).map_err(|_| ())?;
                    session = Some(created);
                    Ok(json!({"wire":hex::encode(reply)}))
                }
                "finish_noise" => {
                    let session = session.as_mut().ok_or(())?;
                    session
                        .read_handshake(&bytes(&value, "wire")?, now)
                        .map_err(|_| ())?;
                    let signature = key
                        .as_ref()
                        .ok_or(())?
                        .sign(&session.binding_message(now).map_err(|_| ())?)
                        .to_bytes();
                    Ok(
                        json!({"wire":hex::encode(session.seal_binding(&signature,now).map_err(|_| ())?)}),
                    )
                }
                "client_binding" => {
                    let session = session.as_mut().ok_or(())?;
                    session
                        .open_binding(&bytes(&value, "wire")?, now)
                        .map_err(|_| ())?;
                    Ok(json!({"wire":hex::encode(session.seal_binding_ack(now).map_err(|_| ())?)}))
                }
                "request" => {
                    let session = session.as_mut().ok_or(())?;
                    let request = session
                        .open_request(&bytes(&value, "wire")?, now)
                        .map_err(|_| ())?;
                    let reply = session
                        .seal_response(request.id, &request.body, now)
                        .map_err(|_| ())?;
                    Ok(json!({"wire":hex::encode(reply),"scope":"synthetic_crypto_echo_only"}))
                }
                "close" => {
                    session = None;
                    Ok(json!({"closed":true}))
                }
                _ => Err(()),
            }
        })();
        let output = match result {
            Ok(v) => v,
            Err(_) => {
                session = None;
                json!({"error":"rejected"})
            }
        };
        if writeln!(io::stdout(), "{output}").is_err() || io::stdout().flush().is_err() {
            break;
        }
    }
}
