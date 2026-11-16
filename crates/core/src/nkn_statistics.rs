// SPDX-License-Identifier: AGPL-3.0-only
//! Candidate fixed-purpose local signing, not a wire route or RPC transport.
//! Uses the original Auth, participation ledger and finite <=300s LOCAL Owner
//! proof. No identity creation, service activation, grant renewal or submission.
use rustsimhub_auth::{nkn_session::NknRootAuthority, Auth};
use rustsimhub_policy::{Principal, Scope};
use rustsimhub_usage_stats::{
    check_in::AttemptBudget,
    protocol::{
        CheckInIntent, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent, SystemClock,
    },
    store::{
        participation::{Actor, Binding, Ledger, Snapshot},
        FileStore,
    },
    transaction::PreparedSubscription,
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Denied,
    Busy,
    Store,
    Protocol,
    Deadline,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics local signer: {self:?}")
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

/// Host-owned only; never deserializable. Retaining this value grants no future
/// operation: every preparation rechecks original Auth/realm/consent/budget.
pub struct SubscriptionSigner {
    auth: Arc<Mutex<Auth>>,
    token: Zeroizing<String>,
    authority: NknRootAuthority,
    principal: Principal,
    binding: Binding,
    instance: String,
    permission: PathBuf,
    consent: Snapshot,
}
impl Drop for SubscriptionSigner {
    fn drop(&mut self) {
        self.authority.invalidate();
    }
}
impl SubscriptionSigner {
    /// permission is the Core-owned participation directory, never a wire path.
    /// Absence/OFF/wrong binding fails without creating or opening any identity.
    pub fn from_current_owner(
        auth: Arc<Mutex<Auth>>,
        token: &str,
        permission: &Path,
    ) -> Result<Self> {
        let (principal, binding, instance, authority) = {
            let mut current = auth.try_lock().map_err(|_| Error::Busy)?;
            let principal = current.recent_owner(token).map_err(|_| Error::Denied)?;
            if current
                .authorize(token, Scope::SystemManage)
                .map_err(|_| Error::Denied)?
                != principal
            {
                return Err(Error::Denied);
            }
            let binding = Binding {
                core_public_key: current.public_key(),
                realm: current.durable_realm().map_err(|_| Error::Denied)?,
            };
            let instance = current.observer_instance();
            let authority = current
                .nkn_root_authority(token)
                .map_err(|_| Error::Denied)?;
            (principal, binding, instance, authority)
        };
        let ledger = Ledger::open_if_present(permission, binding.clone())
            .map_err(|_| Error::Store)?
            .ok_or(Error::Denied)?;
        let consent = ledger.snapshot().map_err(|_| Error::Store)?;
        if !consent.enabled
            || !ledger
                .authorized_for(&Actor {
                    public_key: principal.public_key,
                    generation: principal.generation,
                })
                .map_err(|_| Error::Store)?
        {
            return Err(Error::Denied);
        }
        let signer = Self {
            auth,
            token: Zeroizing::new(token.to_owned()),
            authority,
            principal,
            binding,
            instance,
            permission: permission.to_owned(),
            consent,
        };
        {
            let current = signer.auth.try_lock().map_err(|_| Error::Busy)?;
            signer.check_auth(&current)?;
        }
        Ok(signer)
    }
    fn check_auth(&self, auth: &Auth) -> Result<()> {
        auth.check_nkn_root_authority(&self.authority)
            .map_err(|_| Error::Denied)?;
        if auth.public_key() != self.binding.core_public_key
            || auth.durable_realm().map_err(|_| Error::Denied)? != self.binding.realm
            || auth.observer_instance() != self.instance
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn budget(&self, budget: &AttemptBudget) -> Result<()> {
        if budget.cancelled() || budget.expired() || Instant::now() >= self.authority.deadline() {
            return Err(Error::Deadline);
        }
        Ok(())
    }
    /// Pure prepare for an original CheckInMachine attempt. nonce must come from
    /// its finite, registration-bound preflight adapter. This component does
    /// not claim that the future Root/SDK transaction protocol is connected.
    pub fn prepare(
        &self,
        store: &FileStore,
        intent: CheckInIntent,
        nonce: u64,
        budget: &AttemptBudget,
    ) -> Result<PreparedSubscription> {
        self.budget(budget)?;
        // Auth stays locked over the small bounded local operation. A completed
        // logout/revoke cannot race a later successful signature return.
        let mut auth = self.auth.try_lock().map_err(|_| Error::Busy)?;
        self.check_auth(&auth)?;
        if auth.recent_owner(&self.token).map_err(|_| Error::Denied)? != self.principal
            || auth
                .authorize(&self.token, Scope::SystemManage)
                .map_err(|_| Error::Denied)?
                != self.principal
        {
            return Err(Error::Denied);
        }
        // The existing lifetime file lock prevents a completed OFF/consent edit
        // crossing this prepare. No second consent store or identity is made.
        let ledger = Ledger::open_if_present(&self.permission, self.binding.clone())
            .map_err(|_| Error::Store)?
            .ok_or(Error::Denied)?;
        if ledger.snapshot().map_err(|_| Error::Store)? != self.consent
            || !ledger
                .authorized_for(&Actor {
                    public_key: self.principal.public_key,
                    generation: self.principal.generation,
                })
                .map_err(|_| Error::Store)?
        {
            return Err(Error::Denied);
        }
        store.load().map_err(|_| Error::Store)?;
        self.budget(budget)?;
        let policy = EnabledPolicy::from_owner_consent(
            OwnerConsent::Granted,
            DisclosureAcknowledgement::Current,
        )
        .map_err(|_| Error::Protocol)?;
        let prepared = store
            .identity()
            .prepare_subscription(policy, intent, nonce, &SystemClock)
            .map_err(|_| Error::Protocol)?
            .ok_or(Error::Denied)?;
        self.budget(budget)?;
        self.check_auth(&auth)?;
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests;
