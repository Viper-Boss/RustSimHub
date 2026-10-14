// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_auth::{Auth, Error, Result};
use rustsimhub_compat::history::HistoryService;
use rustsimhub_domain::HistoryEntity;
use rustsimhub_policy::Scope;
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
pub(crate) fn dispatch(
    auth: &mut Auth,
    history: &mut HistoryService,
    command: AuthCommand,
) -> Result<Value> {
    use AuthCommand::*;
    let denied = |_| Error::Forbidden;
    Ok(match command {
        HistorySources { token } => {
            let p = auth.authorize(&token, Scope::StatusRead)?;
            let value = serde_json::to_value(history.sources(&p).map_err(denied)?)
                .map_err(|_| Error::InvalidInput)?;
            if auth.authorize(&token, Scope::StatusRead)? != p {
                return Err(Error::Forbidden);
            }
            json!({"sources":value,"archived":true})
        }
        HistoryPage {
            token,
            source,
            entity,
            after,
            limit,
        } => {
            let p = auth.authorize(&token, scope(entity))?;
            let value = history
                .page(&p, &source, entity, after.as_ref(), limit)
                .map_err(denied)?;
            if auth.authorize(&token, scope(entity))? != p {
                return Err(Error::Forbidden);
            }
            value
        }
        HistoryDetail {
            token,
            source,
            entity,
            record_id,
        } => {
            let p = auth.authorize(&token, scope(entity))?;
            let value = history
                .detail(&p, &source, entity, &record_id)
                .map_err(denied)?;
            if auth.authorize(&token, scope(entity))? != p {
                return Err(Error::Forbidden);
            }
            value
        }
        GrantHistory {
            token,
            public_key,
            expected_generation,
            source,
            legacy_user,
            entities,
        } => {
            let owner = auth.recent_owner(&token)?;
            let current = auth.current_principal(public_key)?;
            if current.generation != expected_generation {
                return Err(Error::Forbidden);
            }
            history
                .grant(&owner, &current, &source, legacy_user.as_deref(), &entities)
                .map_err(denied)?;
            json!({"ok":true,"generation":expected_generation,"explicit_history_delegation":true})
        }
        RevokeHistory {
            token,
            public_key,
            source,
        } => {
            let owner = auth.recent_owner(&token)?;
            history
                .revoke(&owner, public_key, &source)
                .map_err(denied)?;
            json!({"ok":true})
        }
        _ => return Err(Error::Forbidden),
    })
}
fn scope(entity: HistoryEntity) -> Scope {
    match entity {
        HistoryEntity::Calls => Scope::CallsRead,
        _ => Scope::SmsRead,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn login(auth: &mut Auth, key: &SigningKey) -> String {
        let c = auth
            .login_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .into()
    }
    #[test]
    fn history_requires_current_signed_authority_explicit_recent_owner_delegation_and_enabled_provider(
    ) {
        let state = dir();
        let identity = dir();
        let root = dir();
        let archive = dir();
        let access = dir();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
        let owner = SigningKey::from_bytes(&[31; 32]);
        let client = SigningKey::from_bytes(&[32; 32]);
        let c = auth
            .bootstrap_challenge(owner.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
            .unwrap();
        let token = login(&mut auth, &owner);
        let scopes = vec![Scope::StatusRead, Scope::SmsRead];
        auth.open_pairing(&token).unwrap();
        let c = auth
            .pairing_challenge(client.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let pair = auth
            .pairing_finish(&c.id, &client.sign(&c.transcript).to_bytes())
            .unwrap();
        let principal = auth
            .approve(
                &token,
                &pair,
                client.verifying_key().to_bytes(),
                Kind::Client,
                &scopes,
            )
            .unwrap();
        let ct = login(&mut auth, &client);
        // Use the public importer to construct an independent archive, not a policy fixture.
        let input = root.path().join("input.sqlite");
        let sql = include_str!("../../compat/src/history/test-schema.sql");
        let db = rusqlite::Connection::open(&input).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        fs::set_permissions(&input, fs::Permissions::from_mode(0o600)).unwrap();
        let source = "a".repeat(64);
        rustsimhub_compat::history::import(&input, &source, archive.path()).unwrap();
        let mut h = HistoryService::open(
            archive.path(),
            access.path(),
            auth.public_key(),
            &auth.durable_realm().unwrap(),
        )
        .unwrap();
        assert!(crate::authenticated::dispatch(
            &mut auth,
            AuthCommand::HistorySources {
                token: token.clone()
            }
        )
        .is_err());
        assert!(dispatch(
            &mut auth,
            &mut h,
            AuthCommand::HistoryPage {
                token: ct.clone(),
                source: source.clone(),
                entity: HistoryEntity::Sms,
                after: None,
                limit: 1
            }
        )
        .is_err());
        let grant = |t: String, g| AuthCommand::GrantHistory {
            token: t,
            public_key: client.verifying_key().to_bytes(),
            expected_generation: g,
            source: source.clone(),
            legacy_user: Some("legacy_user_a".into()),
            entities: vec![HistoryEntity::Sms],
        };
        assert!(dispatch(&mut auth, &mut h, grant(ct.clone(), principal.generation)).is_err());
        assert!(dispatch(
            &mut auth,
            &mut h,
            grant(token.clone(), principal.generation + 1)
        )
        .is_err());
        assert_eq!(
            dispatch(
                &mut auth,
                &mut h,
                grant(token.clone(), principal.generation)
            )
            .unwrap()["ok"],
            true
        );
        let page = dispatch(
            &mut auth,
            &mut h,
            AuthCommand::HistoryPage {
                token: ct.clone(),
                source: source.clone(),
                entity: HistoryEntity::Sms,
                after: None,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(page["records"].as_array().unwrap().len(), 1);
        dispatch(
            &mut auth,
            &mut h,
            AuthCommand::RevokeHistory {
                token: token.clone(),
                public_key: client.verifying_key().to_bytes(),
                source: source.clone(),
            },
        )
        .unwrap();
        assert!(dispatch(
            &mut auth,
            &mut h,
            AuthCommand::HistoryPage {
                token: ct.clone(),
                source: source.clone(),
                entity: HistoryEntity::Sms,
                after: serde_json::from_value(page["cursor"].clone()).unwrap(),
                limit: 1
            }
        )
        .is_err());
        dispatch(
            &mut auth,
            &mut h,
            grant(token.clone(), principal.generation),
        )
        .unwrap();
        auth.revoke(&token, client.verifying_key().to_bytes())
            .unwrap();
        assert!(dispatch(&mut auth, &mut h, AuthCommand::HistorySources { token: ct }).is_err());
        assert!(dispatch(&mut auth, &mut h, grant(token, principal.generation)).is_err());
    }
}
