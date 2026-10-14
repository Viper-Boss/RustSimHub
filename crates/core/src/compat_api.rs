// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_auth::Auth;
use rustsimhub_compat::session::{Authority, SignedSession};
use rustsimhub_domain::NasEntity;
use rustsimhub_policy::Scope;
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, rustsimhub_auth::Error>;
pub(crate) fn dispatch(
    auth: &mut Auth,
    authority: &mut Authority,
    command: AuthCommand,
) -> Result<Value> {
    fn denied<T>(_: T) -> rustsimhub_auth::Error {
        rustsimhub_auth::Error::Forbidden
    }
    Ok(match command {
        AuthCommand::BindLegacy {
            token,
            core_id,
            public_key,
            expected_generation,
            scopes,
        } => {
            let owner = auth.recent_owner(&token)?;
            let p = auth.current_principal(public_key)?;
            if p.generation != expected_generation {
                return Err(rustsimhub_auth::Error::Forbidden);
            }
            serde_json::to_value(
                authority
                    .bind(&owner, &p, &core_id, &scopes)
                    .map_err(denied)?,
            )
            .map_err(denied)?
        }
        AuthCommand::RevokeLegacy { token, core_id } => {
            let owner = auth.recent_owner(&token)?;
            authority.revoke(&owner, &core_id).map_err(denied)?;
            json!({"ok":true})
        }
        AuthCommand::LegacySession {
            core_id,
            device_id,
            binding_epoch,
            timestamp,
            nonce,
            signature,
            device_proof,
            body,
        } => {
            let b = authority.binding(&core_id).map_err(denied)?;
            let p = auth.current_principal(b.public_key)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(denied)?
                .as_secs();
            let now = i64::try_from(now).map_err(denied)?;
            let issued = authority.issue(
                &p,
                SignedSession {
                    core_id: &core_id,
                    device_id: &device_id,
                    binding_epoch: &binding_epoch,
                    timestamp: &timestamp,
                    nonce: &nonce,
                    signature: &signature,
                    device_proof: &device_proof,
                    body: &body,
                },
                now,
            );
            match issued {
                Ok(issued) => json!({"token":issued.token.as_str(),"expires_at":issued.expires_at}),
                Err(error) => {
                    json!({"error":match error {rustsimhub_compat::session::Error::Clock=>"legacy_clock_not_ready",rustsimhub_compat::session::Error::Replay=>"legacy_replay",rustsimhub_compat::session::Error::Busy=>"legacy_busy",_=>"denied"}})
                }
            }
        }
        AuthCommand::LegacyRead {
            token,
            entity,
            after,
            limit,
            record_id,
        } => {
            let b = authority.subject(&token).map_err(denied)?;
            let p = auth.current_principal(b.public_key)?;
            let scope = match entity {
                NasEntity::Status => Scope::StatusRead,
                NasEntity::Calls => Scope::CallsRead,
                _ => Scope::SmsRead,
            };
            authority.authorize(&token, &p, scope).map_err(denied)?;
            let data = auth.nas_read(&p, entity, after, limit, record_id)?;
            authority
                .authorize(&token, &auth.current_principal(p.public_key)?, scope)
                .map_err(denied)?;
            json!({"adapter_api":1,"data_format":2,"data":data})
        }
        _ => return Err(rustsimhub_auth::Error::Forbidden),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_auth::Identity;
    use rustsimhub_policy::Kind;
    use rustsimhub_storage::Store;
    use std::{fs, os::unix::fs::PermissionsExt};
    fn dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    fn login(a: &mut Auth, k: &SigningKey) -> String {
        let c = a.login_challenge(k.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .into()
    }
    #[test]
    fn legacy_bearers_never_become_v2_authority_and_each_read_checks_current_nas() {
        let state = dir();
        let identity = dir();
        let ledger = dir();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
        let owner = SigningKey::from_bytes(&[54; 32]);
        let nas = SigningKey::from_bytes(&[55; 32]);
        let c = auth
            .bootstrap_challenge(owner.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
            .unwrap();
        let ot = login(&mut auth, &owner);
        let scopes = vec![Scope::StatusRead, Scope::SmsRead];
        auth.open_pairing(&ot).unwrap();
        let c = auth
            .pairing_challenge(nas.verifying_key().to_bytes(), Kind::Nas, &scopes)
            .unwrap();
        let pending = auth
            .pairing_finish(&c.id, &nas.sign(&c.transcript).to_bytes())
            .unwrap();
        let current = auth
            .approve(
                &ot,
                &pending,
                nas.verifying_key().to_bytes(),
                Kind::Nas,
                &scopes,
            )
            .unwrap();
        let nt = login(&mut auth, &nas);
        let mut gate = Authority::open(
            ledger.path(),
            auth.public_key(),
            &auth.durable_realm().unwrap(),
        )
        .unwrap();
        let bind = |token: String, generation| AuthCommand::BindLegacy {
            token,
            core_id: "synthetic-nas".into(),
            public_key: nas.verifying_key().to_bytes(),
            expected_generation: generation,
            scopes: scopes.clone(),
        };
        assert!(dispatch(&mut auth, &mut gate, bind(nt.clone(), current.generation)).is_err());
        assert!(dispatch(
            &mut auth,
            &mut gate,
            bind(ot.clone(), current.generation + 1)
        )
        .is_err());
        let binding = dispatch(&mut auth, &mut gate, bind(ot.clone(), current.generation)).unwrap();
        let stamp = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let nonce = "synthetic-nonce-first-session";
        let body = serde_json::to_vec(&json!({"core_id":"synthetic-nas"})).unwrap();
        let canonical =
            rustsimhub_compat::signing_bytes("POST", "/v1/session", &stamp, nonce, &body).unwrap();
        let epoch = binding["binding_epoch"].as_str().unwrap();
        let proof =
            rustsimhub_compat::session::device_bound_bytes(auth.public_key(), epoch, &canonical)
                .unwrap();
        let command = AuthCommand::LegacySession {
            core_id: "synthetic-nas".into(),
            device_id: rustsimhub_compat::device_id(&auth.public_key()),
            binding_epoch: epoch.into(),
            timestamp: stamp,
            nonce: nonce.into(),
            signature: STANDARD.encode(nas.sign(&canonical).to_bytes()),
            device_proof: STANDARD.encode(nas.sign(&proof).to_bytes()),
            body,
        };
        let issued = dispatch(&mut auth, &mut gate, command).unwrap();
        let legacy = issued["token"].as_str().unwrap().to_owned();
        assert!(auth.authorize(&legacy, Scope::StatusRead).is_err());
        let read = |token: String| AuthCommand::LegacyRead {
            token,
            entity: NasEntity::Status,
            after: None,
            limit: 1,
            record_id: None,
        };
        assert!(dispatch(&mut auth, &mut gate, read(nt)).is_err());
        assert!(crate::authenticated::dispatch(&mut auth, read(legacy.clone())).is_err());
        assert_eq!(
            dispatch(&mut auth, &mut gate, read(legacy.clone())).unwrap()["data_format"],
            2
        );
        auth.revoke(&ot, nas.verifying_key().to_bytes()).unwrap();
        assert!(dispatch(&mut auth, &mut gate, read(legacy)).is_err());
    }
}
