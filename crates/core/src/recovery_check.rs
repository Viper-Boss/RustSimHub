// SPDX-License-Identifier: AGPL-3.0-only
//! Isolated random synthetic identities only; absent from production builds.
use rustsimhub_auth::{Auth, ClientKey, Identity, ProofExpectation};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::Store;
use serde_json::json;
use std::{fs, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn hex(key: [u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
fn key(hex: &str) -> Result<[u8; 32]> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid fixture marker".into());
    }
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)?;
    }
    Ok(bytes)
}
pub fn prepare(directory: &str) -> Result<()> {
    let base = Path::new(directory);
    for name in [
        "original",
        "restored",
        "identity",
        "identity-backup",
        "database-backup",
        "owner-key",
        "old-client-key",
        "sessions",
        "runtime",
    ] {
        fs::create_dir(base.join(name))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(base.join(name), fs::Permissions::from_mode(0o700))?;
        }
    }
    let owner = ClientKey::create(base.join("owner-key"))?;
    let client = ClientKey::create(base.join("old-client-key"))?;
    let identity = Identity::open(base.join("identity"))?;
    let pin = identity.public_key();
    identity.backup(base.join("identity-backup"))?;
    if Identity::load(base.join("identity-backup"))?.public_key() != pin {
        return Err("identity backup mismatch".into());
    }
    let store = Store::open(base.join("original"))?;
    let mut auth = Auth::new(identity, store.handle())?;
    let c = auth.bootstrap_challenge(owner.public_key())?;
    auth.bootstrap_finish(&c.id, &owner.prove(&c, pin, ProofExpectation::Bootstrap)?)?;
    let c = auth.login_challenge(owner.public_key())?;
    let token = auth.login_finish(&c.id, &owner.prove(&c, pin, ProofExpectation::Login)?)?;
    auth.open_pairing(token.expose())?;
    let c = auth.pairing_challenge(client.public_key(), Kind::Nas, &[Scope::StatusRead])?;
    let pending = auth.pairing_finish(
        &c.id,
        &client.prove(
            &c,
            pin,
            ProofExpectation::Pair {
                kind: Kind::Nas,
                scopes: 1,
            },
        )?,
    )?;
    auth.approve(
        token.expose(),
        &pending,
        client.public_key(),
        Kind::Nas,
        &[Scope::StatusRead],
    )?;
    store.handle().backup(base.join("database-backup"))?;
    auth.revoke(token.expose(), client.public_key())?;
    drop(auth);
    drop(store);
    let restored = Store::restore(base.join("database-backup"), base.join("restored"))?;
    if restored.handle().principals()?.iter().any(|p| p.active) {
        return Err("backup revived authority".into());
    }
    drop(restored);
    for (file, contents) in [
        ("device.pubhex", hex(pin)),
        ("owner.pubhex", hex(owner.public_key())),
        ("old-client.pubhex", hex(client.public_key())),
    ] {
        rustsimhub_auth::write_private_new(&base.join(file), contents.as_bytes())?;
    }
    rustsimhub_auth::write_private_new(&base.join(".fixture-recovery"), b"RSH-RECOVERY-FIXTURE-1")?;
    println!(
        "{}",
        json!({"ok":true,"old_backup_restored":true,"all_grants_quarantined":true,"identity_backup_verified":true,"hardware_mutations":false})
    );
    Ok(())
}
pub fn verify(directory: &str) -> Result<()> {
    let base = Path::new(directory);
    if fs::read(base.join(".fixture-recovery"))? != b"RSH-RECOVERY-FIXTURE-1" {
        return Err("fixture marker missing".into());
    }
    let pin = key(&fs::read_to_string(base.join("device.pubhex"))?)?;
    if !base.join("restored/store.sqlite3").is_file() {
        return Err("fixture database missing".into());
    }
    let store = Store::open(base.join("restored"))?;
    let mut auth = Auth::open(base.join("identity"), store.handle())?;
    if auth.public_key() != pin {
        return Err("identity changed".into());
    }
    let owner = ClientKey::load(base.join("owner-key"))?;
    let client = ClientKey::load(base.join("old-client-key"))?;
    let c = auth.login_challenge(owner.public_key())?;
    let token = auth.login_finish(&c.id, &owner.prove(&c, pin, ProofExpectation::Login)?)?;
    let principals = auth.clients(token.expose())?;
    if principals.iter().filter(|p| p.active).count() != 1
        || auth.login_challenge(client.public_key()).is_ok()
    {
        return Err("client authority revived".into());
    }
    println!(
        "{}",
        json!({"ok":true,"owner_restored":true,"identity_unchanged":true,"clients_still_revoked":true,"writer_reopened_after_shutdown":true,"hardware_mutations":false})
    );
    Ok(())
}
