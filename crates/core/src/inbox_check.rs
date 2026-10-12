// SPDX-License-Identifier: AGPL-3.0-only
//! Independent development directory and random keys; never compiled into default builds.
use rustsimhub_auth::{write_private_new, Auth, ClientKey, Identity, ProofExpectation};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::Store;
use serde_json::json;
use std::{fs, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn prepare(directory: &str) -> Result<()> {
    let base = Path::new(directory);
    for name in [
        "state",
        "identity",
        "owner-key",
        "status-key",
        "reader-key",
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
    let identity = Identity::open(base.join("identity"))?;
    let pin = identity.public_key();
    let store = Store::open(base.join("state"))?;
    let mut auth = Auth::new(identity, store.handle())?;
    let c = auth.bootstrap_challenge(owner.public_key())?;
    auth.bootstrap_finish(&c.id, &owner.prove(&c, pin, ProofExpectation::Bootstrap)?)?;
    let c = auth.login_challenge(owner.public_key())?;
    let token = auth.login_finish(&c.id, &owner.prove(&c, pin, ProofExpectation::Login)?)?;
    for (name, scopes) in [
        ("status-key", vec![Scope::StatusRead]),
        ("reader-key", vec![Scope::SmsRead]),
    ] {
        let client = ClientKey::create(base.join(name))?;
        auth.open_pairing(token.expose())?;
        let c = auth.pairing_challenge(client.public_key(), Kind::Client, &scopes)?;
        let id = auth.pairing_finish(
            &c.id,
            &client.prove(
                &c,
                pin,
                ProofExpectation::Pair {
                    kind: Kind::Client,
                    scopes: rustsimhub_policy::mask(&scopes).ok_or("invalid fixture scopes")?,
                },
            )?,
        )?;
        auth.approve(
            token.expose(),
            &id,
            client.public_key(),
            Kind::Client,
            &scopes,
        )?;
        let hex: String = client
            .public_key()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        write_private_new(&base.join(format!("{name}.pubhex")), hex.as_bytes())?;
    }
    let hex: String = pin.iter().map(|b| format!("{b:02x}")).collect();
    write_private_new(&base.join("device.pubhex"), hex.as_bytes())?;
    println!(
        "{}",
        json!({"ok":true,"fixture":"independent_random_readers","hardware_mutations":false})
    );
    Ok(())
}
pub fn verify(directory: &str) -> Result<()> {
    let store = Store::open(Path::new(directory).join("state"))?;
    let h = store.handle();
    let mut after = None;
    let mut count = 0usize;
    loop {
        let page = h.inbox(after, 32)?;
        count += page.messages.len();
        if page.messages.len() < 32 {
            break;
        }
        after = Some(page.cursor);
    }
    if h.line_epoch()?.0 <= 1 {
        return Err("no observation persisted".into());
    }
    println!(
        "{}",
        json!({"ok":true,"persisted_messages":count,"line_observation_persisted":true,"startup_readiness_reset":h.line_status()?.status==rustsimhub_storage::LineStatus::Unavailable,"hardware_mutations":false})
    );
    Ok(())
}
