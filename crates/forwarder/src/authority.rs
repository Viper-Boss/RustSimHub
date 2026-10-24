// SPDX-License-Identifier: AGPL-3.0-only
//! Actual Auth/Store integration for Core, not a caller-supplied Owner flag.
//! Wire handlers and the service/worker channel are intentionally separate.
use crate::*;
use rustsimhub_auth::Auth;
use rustsimhub_policy::{allows, Kind, Principal, Scope};
use rustsimhub_storage::{Handle, LineStatus};

fn recent(auth: &mut Auth, token: &str) -> Result<Binding> {
    let owner = auth.recent_owner(token).map_err(|_| Error::Forbidden)?;
    auth.authorize(token, Scope::SystemManage)
        .map_err(|_| Error::Forbidden)?;
    binding(auth, &owner)
}
fn binding(auth: &Auth, owner: &Principal) -> Result<Binding> {
    if owner.kind != Kind::Owner
        || !owner.active
        || !allows(owner.scopes, Scope::SmsRead)
        || !allows(owner.scopes, Scope::SystemManage)
    {
        return Err(Error::Forbidden);
    }
    let b = Binding {
        device_key: auth.public_key(),
        realm: auth.durable_realm().map_err(|_| Error::Forbidden)?,
        owner_key: owner.public_key,
        owner_generation: owner.generation,
    };
    b.validate()?;
    Ok(b)
}
impl Binding {
    pub fn from_recent_owner(auth: &mut Auth, token: &str) -> Result<Self> {
        recent(auth, token)
    }
    /// No dependence on the short-lived browser bearer after explicit consent,
    /// but every capture/attempt still checks the current durable Owner.
    pub fn check_current(&self, auth: &Auth) -> Result<()> {
        let owner = auth
            .current_principal(self.owner_key)
            .map_err(|_| Error::Forbidden)?;
        if &binding(auth, &owner)? != self {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
impl Queue {
    pub fn configure_owner(
        &mut self,
        auth: &mut Auth,
        reader: &Handle,
        token: &str,
        expected: u64,
        request_id: &str,
        policy: Policy,
    ) -> Result<u64> {
        let b = recent(auth, token)?;
        if &b != self.binding() {
            return Err(Error::Forbidden);
        }
        let head = reader.inbox_event_head().map_err(|_| Error::Storage)?;
        if recent(auth, token)? != b {
            return Err(Error::Forbidden);
        }
        self.configure(&b, expected, request_id, policy, head)
    }
    pub fn capture_current(
        &mut self,
        auth: &Auth,
        reader: &Handle,
        generation: u64,
    ) -> Result<usize> {
        let b = self.binding().clone();
        b.check_current(auth)?;
        let count = self.capture(&b, generation, reader)?;
        b.check_current(auth)?;
        Ok(count)
    }
    pub fn claim_current(
        &mut self,
        auth: &Auth,
        reader: &Handle,
        generation: u64,
    ) -> Result<Option<Claim>> {
        let b = self.binding().clone();
        b.check_current(auth)?;
        if reader
            .inbox_event_head()
            .map_err(|_| Error::Storage)?
            .db_epoch
            != b.realm
        {
            return Err(Error::Stale);
        }
        let live = reader.line_status().map_err(|_| Error::Storage)?;
        if live.status != LineStatus::Ready {
            return Err(Error::Stale);
        }
        let claim = self.claim(&b, generation, live.line)?;
        if let Some(claim) = claim.as_ref() {
            if let Err(error) = self.check_current_claim(auth, reader, claim) {
                // A fenced-out reservation is ambiguous, never automatically
                // returned to the queue, even if no network bytes were written.
                let _ = self.finish(&b, claim, State::Unknown);
                return Err(error);
            }
        }
        Ok(claim)
    }
    /// The worker MUST call this via its Core IPC before any application write;
    /// a locally cached Binding or boolean cannot stand in for this result.
    pub fn check_current_claim(&self, auth: &Auth, reader: &Handle, claim: &Claim) -> Result<()> {
        self.binding().check_current(auth)?;
        if reader
            .inbox_event_head()
            .map_err(|_| Error::Storage)?
            .db_epoch
            != self.binding().realm
        {
            return Err(Error::Stale);
        }
        let live = reader.line_status().map_err(|_| Error::Storage)?;
        if live.status != LineStatus::Ready {
            return Err(Error::Stale);
        }
        self.check_claim(self.binding(), claim, live.line)?;
        self.binding().check_current(auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_auth::{Identity, Token};
    use rustsimhub_storage::{IncomingSms, LineObservation, Store};
    use std::{fs, os::unix::fs::PermissionsExt};
    use tempfile::TempDir;
    use uuid::Uuid;
    fn directory() -> TempDir {
        let d = TempDir::new().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    fn owner(auth: &mut Auth, key: &SigningKey) -> Token {
        let c = auth
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        let c = auth
            .login_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
    }
    fn policy() -> Policy {
        let target = Uuid::new_v4().to_string();
        Policy {
            enabled: true,
            targets: vec![Target {
                id: target.clone(),
                destination: Destination::Webhook {
                    url: Secret::new("https://example.invalid/synthetic".into()),
                    bearer: None,
                },
            }],
            rules: vec![Rule {
                id: Uuid::new_v4().to_string(),
                targets: vec![target],
                sender_equals: None,
                body_contains: None,
                line: None,
                share: Share::Notice,
            }],
        }
    }
    #[test]
    fn actual_signed_owner_configures_while_even_privileged_paired_client_cannot() {
        let business = directory();
        let s = Store::open(business.path()).unwrap();
        let identity = directory();
        let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), s.handle()).unwrap();
        let owner_token = owner(&mut auth, &SigningKey::from_bytes(&[81; 32]));
        let b = Binding::from_recent_owner(&mut auth, owner_token.expose()).unwrap();
        let outbox = directory();
        let mut q = Queue::open(outbox.path(), b).unwrap();
        assert!(q
            .configure_owner(
                &mut auth,
                &s.handle(),
                "invalid",
                0,
                &Uuid::new_v4().to_string(),
                policy()
            )
            .is_err());
        assert_eq!(q.status().unwrap().generation, 0);
        let client = SigningKey::from_bytes(&[82; 32]);
        let scopes = [Scope::SmsRead, Scope::SystemManage, Scope::ClientsManage];
        auth.open_pairing(owner_token.expose()).unwrap();
        let c = auth
            .pairing_challenge(client.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let pair = auth
            .pairing_finish(&c.id, &client.sign(&c.transcript).to_bytes())
            .unwrap();
        auth.approve(
            owner_token.expose(),
            &pair,
            client.verifying_key().to_bytes(),
            Kind::Client,
            &scopes,
        )
        .unwrap();
        let c = auth
            .login_challenge(client.verifying_key().to_bytes())
            .unwrap();
        let token = auth
            .login_finish(&c.id, &client.sign(&c.transcript).to_bytes())
            .unwrap();
        assert!(q
            .configure_owner(
                &mut auth,
                &s.handle(),
                token.expose(),
                0,
                &Uuid::new_v4().to_string(),
                policy()
            )
            .is_err());
        assert_eq!(q.status().unwrap().generation, 0);
        assert_eq!(
            q.configure_owner(
                &mut auth,
                &s.handle(),
                owner_token.expose(),
                0,
                &Uuid::new_v4().to_string(),
                policy()
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn actual_owner_revocation_and_request_tampering_fence_reserved_payload() {
        let business = directory();
        let s = Store::open(business.path()).unwrap();
        let h = s.handle();
        let identity = directory();
        let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), h.clone()).unwrap();
        let key = SigningKey::from_bytes(&[83; 32]);
        let token = owner(&mut auth, &key);
        let line = h
            .observe_line(LineObservation {
                backend: "synthetic-modem".into(),
                fingerprint: Some([7; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let b = Binding::from_recent_owner(&mut auth, token.expose()).unwrap();
        let outbox = directory();
        let mut q = Queue::open(outbox.path(), b).unwrap();
        q.configure_owner(
            &mut auth,
            &h,
            token.expose(),
            0,
            &Uuid::new_v4().to_string(),
            policy(),
        )
        .unwrap();
        h.receive_sms(IncomingSms {
            line,
            source: "synthetic-source".into(),
            sender: "synthetic sender".into(),
            timestamp: "synthetic time".into(),
            text: "synthetic private text".into(),
            data: vec![],
        })
        .unwrap();
        assert_eq!(q.capture_current(&auth, &h, 1).unwrap(), 1);
        let mut claim = q.claim_current(&auth, &h, 1).unwrap().unwrap();
        q.check_current_claim(&auth, &h, &claim).unwrap();
        let original = serde_json::to_string(&claim.request).unwrap();
        claim.request.body = Secret::new("changed request".into());
        assert!(q.check_current_claim(&auth, &h, &claim).is_err());
        claim.request = serde_json::from_str(&original).unwrap();
        q.check_current_claim(&auth, &h, &claim).unwrap();
        // Emulate an offline Owner revocation in the real durable authority,
        // not an in-memory boolean. No target device or real SMS is involved.
        let db = rusqlite::Connection::open(business.path().join("store.sqlite3")).unwrap();
        db.execute(
            "UPDATE principals SET active=0,generation=generation+1 WHERE kind='owner'",
            [],
        )
        .unwrap();
        assert!(q.check_current_claim(&auth, &h, &claim).is_err());
        assert!(q.capture_current(&auth, &h, 1).is_err());
        db.execute("UPDATE principals SET active=1 WHERE kind='owner'", [])
            .unwrap();
        assert!(
            q.binding().check_current(&auth).is_err(),
            "re-enabling a principal does not restore its old generation"
        );
    }
    #[test]
    fn actual_sim_change_fences_an_otherwise_valid_claim() {
        let business = directory();
        let s = Store::open(business.path()).unwrap();
        let h = s.handle();
        let identity = directory();
        let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), h.clone()).unwrap();
        let token = owner(&mut auth, &SigningKey::from_bytes(&[84; 32]));
        let line = h
            .observe_line(LineObservation {
                backend: "synthetic-modem".into(),
                fingerprint: Some([8; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let b = Binding::from_recent_owner(&mut auth, token.expose()).unwrap();
        let outbox = directory();
        let mut q = Queue::open(outbox.path(), b).unwrap();
        q.configure_owner(
            &mut auth,
            &h,
            token.expose(),
            0,
            &Uuid::new_v4().to_string(),
            policy(),
        )
        .unwrap();
        h.receive_sms(IncomingSms {
            line,
            source: "synthetic-source".into(),
            sender: "synthetic".into(),
            timestamp: "synthetic".into(),
            text: "synthetic".into(),
            data: vec![],
        })
        .unwrap();
        q.capture_current(&auth, &h, 1).unwrap();
        let claim = q.claim_current(&auth, &h, 1).unwrap().unwrap();
        q.check_current_claim(&auth, &h, &claim).unwrap();
        let other_directory = directory();
        let other = Store::open(other_directory.path()).unwrap();
        other
            .handle()
            .observe_line(LineObservation {
                backend: "other synthetic modem".into(),
                fingerprint: Some([8; 32]),
                status: LineStatus::Ready,
            })
            .unwrap();
        assert!(matches!(
            q.check_current_claim(&auth, &other.handle(), &claim),
            Err(Error::Stale)
        ));
        assert!(matches!(
            q.claim_current(&auth, &other.handle(), 1),
            Err(Error::Stale)
        ));
        let next = h
            .observe_line(LineObservation {
                backend: "synthetic-modem".into(),
                fingerprint: Some([9; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        assert_ne!(line, next);
        q.binding().check_current(&auth).unwrap();
        assert!(matches!(
            q.check_current_claim(&auth, &h, &claim),
            Err(Error::Stale)
        ));
    }
}
