// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only durable observations; never resume or mutate an old NM transaction.
use super::*;

pub(super) async fn authority(
    auth: Arc<Mutex<Auth>>,
    token: String,
) -> Result<(Principal, String)> {
    tokio::task::spawn_blocking(move || {
        let mut a = auth.lock().map_err(|_| Error::Busy)?;
        let p = a.network_manager(&token)?;
        let mut h = Sha256::new();
        h.update(b"RustSimHub.USB.WifiReceipt.v2\0");
        h.update(a.public_key());
        h.update(a.durable_realm()?.as_bytes());
        h.update(serde_json::to_vec(&p).map_err(|_| Error::Io)?);
        let binding = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        Ok((p, binding))
    })
    .await
    .map_err(|_| Error::Io)?
}

fn optional(path: &Path) -> Result<Option<Value>> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Io),
        Ok(_) => {}
    }
    let bytes = rustsimhub_auth::read_private(path, 8192)?;
    Ok(Some(serde_json::from_slice(&bytes).map_err(|_| Error::Io)?))
}

pub(super) fn read(dir: &Path, key: &str, p: &Principal, binding: &str) -> Result<Value> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
        return Err(Error::InvalidInput);
    }
    let audit = Audit::new(dir, key, p)?;
    let unknown = || json!({"known":false,"state":"unknown","receipt_only":true,"requires_confirmation":false,"current_network_verified":false,"not_found_is_not_proof":true});
    let Some(started) = optional(&audit.started)? else {
        return Ok(unknown());
    };
    // Schema 1 remains a replay barrier, but has no complete realm/principal binding.
    if started["schema"] != 2 {
        return Ok(unknown());
    }
    if started["binding"] != binding
        || started["principal"] != p.id
        || started["generation"] != p.generation
    {
        return Err(Error::Forbidden);
    }
    let id = started["trial_id"].as_str().ok_or(Error::Io)?;
    if started.as_object().ok_or(Error::Io)?.len() != 5
        || id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(Error::Io);
    }
    let result = optional(&audit.result)?;
    let terminal_recorded = result.is_some();
    let state = if let Some(mut result) = result {
        let state = result
            .as_object_mut()
            .ok_or(Error::Io)?
            .remove("state")
            .ok_or(Error::Io)?;
        if result != started || !matches!(state.as_str(), Some("saved" | "rolled_back" | "unknown"))
        {
            return Err(Error::Io);
        }
        state
    } else {
        json!("unknown")
    };
    Ok(
        json!({"known":true,"trial_id":id,"state":state,"terminal_recorded":terminal_recorded,"receipt_only":true,"requires_confirmation":false,"current_network_verified":false,"not_found_is_not_proof":true}),
    )
}
