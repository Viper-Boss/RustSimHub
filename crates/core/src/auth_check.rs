// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic keys ONLY; this module is absent from default builds.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{Auth, Challenge, Identity, Token};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::Store;
use serde_json::json;
use std::{fs, io::Write, path::Path, time::Instant};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn sign(c: &Challenge, k: &SigningKey) -> Vec<u8> {
    k.sign(&c.transcript).to_bytes().to_vec()
}
fn login(a: &mut Auth, k: &SigningKey) -> Result<Token> {
    let c = a.login_challenge(k.verifying_key().to_bytes())?;
    Ok(a.login_finish(&c.id, &sign(&c, k))?)
}
fn subdir(base: &Path, name: &str) -> Result<()> {
    fs::create_dir(base.join(name))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(base.join(name), fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn check(directory: &str) -> Result<()> {
    let start = Instant::now();
    let base = Path::new(directory);
    subdir(base, "state")?;
    subdir(base, "identity")?;
    let store = Store::open(base.join("state"))?;
    let mut auth = Auth::new(Identity::open(base.join("identity"))?, store.handle())?;
    let owner = SigningKey::from_bytes(&[71; 32]);
    let client = SigningKey::from_bytes(&[72; 32]);
    let public = auth.public_key();
    let c = auth.bootstrap_challenge(owner.verifying_key().to_bytes())?;
    auth.bootstrap_finish(&c.id, &sign(&c, &owner))?;
    let token = login(&mut auth, &owner)?;
    auth.open_pairing(token.expose())?;
    let scopes = [Scope::StatusRead];
    let key = client.verifying_key().to_bytes();
    let c = auth.pairing_challenge(key, Kind::Nas, &scopes)?;
    let pending = auth.pairing_finish(&c.id, &sign(&c, &client))?;
    auth.approve(token.expose(), &pending, key, Kind::Nas, &scopes)?;
    let nas = login(&mut auth, &client)?;
    auth.authorize(nas.expose(), Scope::StatusRead)?;
    if auth.authorize(nas.expose(), Scope::SmsSend).is_ok() {
        return Err("scope escalation".into());
    }
    if auth.open_pairing(nas.expose()).is_ok() {
        return Err("NAS owner escalation".into());
    }
    let c = auth.login_challenge(key)?;
    auth.login_finish(&c.id, &sign(&c, &client))?;
    if auth.login_finish(&c.id, &sign(&c, &client)).is_ok() {
        return Err("proof replay".into());
    }
    auth.revoke(token.expose(), key)?;
    if auth.authorize(nas.expose(), Scope::StatusRead).is_ok() {
        return Err("revocation failure".into());
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut marker = options.open(base.join(".fixture-auth"))?;
    marker.write_all(&public)?;
    marker.sync_all()?;
    #[cfg(unix)]
    fs::File::open(base)?.sync_all()?;
    drop(auth);
    drop(store);
    println!(
        "{}",
        json!({"ok":true,"identity_persisted":true,"signed_pairing":true,"scope_denied":true,"replay_denied":true,"revocation_enforced":true,"hardware_mutations":false,"elapsed_ms":start.elapsed().as_millis()})
    );
    Ok(())
}
pub fn reopen(directory: &str) -> Result<()> {
    let base = Path::new(directory);
    let public = fs::read(base.join(".fixture-auth"))?;
    if public.len() != 32
        || !base.join("state/store.sqlite3").is_file()
        || !base.join("identity/identity.seed").is_file()
    {
        return Err("fixture missing".into());
    }
    let store = Store::open(base.join("state"))?;
    let mut auth = Auth::new(Identity::open(base.join("identity"))?, store.handle())?;
    if auth.public_key().as_slice() != public {
        return Err("identity changed".into());
    }
    let owner = SigningKey::from_bytes(&[71; 32]);
    let client = SigningKey::from_bytes(&[72; 32]);
    let token = login(&mut auth, &owner)?;
    auth.authorize(token.expose(), Scope::ClientsManage)?;
    if auth
        .login_challenge(client.verifying_key().to_bytes())
        .is_ok()
    {
        return Err("revocation lost".into());
    }
    println!(
        "{}",
        json!({"ok":true,"separate_process":true,"identity_unchanged":true,"owner_can_authenticate":true,"revocation_persisted":true,"hardware_mutations":false})
    );
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn ipc(directory: &str, socket: &str) -> Result<()> {
    use crate::linux::{read_frame, write_frame};
    use rustsimhub_wire::{AuthCommand, AuthRequest, Response};
    use std::time::Duration;
    use tokio::{net::UnixStream, time::timeout};
    let base = Path::new(directory);
    let expected = fs::read(base.join(".fixture-auth"))?;
    if expected.len() != 32 {
        return Err("fixture marker missing".into());
    }
    async fn call(socket: &str, command: AuthCommand) -> Result<serde_json::Value> {
        let mut s = timeout(Duration::from_secs(5), UnixStream::connect(socket)).await??;
        let r = AuthRequest {
            protocol: 2,
            id: "fixture".into(),
            command,
        };
        timeout(
            Duration::from_secs(5),
            write_frame(&mut s, &serde_json::to_vec(&r)?),
        )
        .await??;
        let bytes = timeout(Duration::from_secs(15), read_frame(&mut s)).await??;
        let reply: Response = serde_json::from_slice(&bytes)?;
        if reply.protocol != 2 || reply.id != "fixture" {
            return Err("invalid IPC response".into());
        }
        Ok(reply.result)
    }
    let identity = call(socket, AuthCommand::Identity {}).await?;
    let public: Vec<u8> = serde_json::from_value(identity["public_key"].clone())?;
    if public != expected {
        return Err("device key mismatch".into());
    }
    let denied = call(
        socket,
        AuthCommand::Health {
            token: "0".repeat(64),
        },
    )
    .await?;
    if denied["error"] != "denied" {
        return Err("unauthenticated IPC accepted".into());
    }
    let key = SigningKey::from_bytes(&[71; 32]);
    let c = call(
        socket,
        AuthCommand::Challenge {
            public_key: key.verifying_key().to_bytes(),
        },
    )
    .await?;
    let transcript: Vec<u8> = serde_json::from_value(c["transcript"].clone())?;
    let signature: Vec<u8> = serde_json::from_value(c["device_signature"].clone())?;
    let public: [u8; 32] = expected.try_into().map_err(|_| "invalid device key")?;
    ed25519_dalek::VerifyingKey::from_bytes(&public)?.verify_strict(
        &transcript,
        &ed25519_dalek::Signature::from_slice(&signature)?,
    )?;
    let id = c["id"].as_str().ok_or("invalid challenge")?.to_owned();
    let proof = key.sign(&transcript).to_bytes().to_vec();
    let issued = call(
        socket,
        AuthCommand::Authenticate {
            challenge_id: id.clone(),
            signature: proof.clone(),
        },
    )
    .await?;
    let token = issued["token"]
        .as_str()
        .ok_or("authentication failed")?
        .to_owned();
    let health = call(socket, AuthCommand::Health { token }).await?;
    if health["ok"] != true || health["write_api"] != false {
        return Err("authenticated health failed".into());
    }
    if call(
        socket,
        AuthCommand::Authenticate {
            challenge_id: id,
            signature: proof,
        },
    )
    .await?["error"]
        != "denied"
    {
        return Err("IPC replay allowed".into());
    }
    println!(
        "{}",
        json!({"ok":true,"real_authenticated_socket":true,"device_signature_verified":true,"unauthenticated_denied":true,"proof_replay_denied":true,"hardware_mutations":false})
    );
    Ok(())
}
