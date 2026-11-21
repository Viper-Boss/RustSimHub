// SPDX-License-Identifier: AGPL-3.0-only
//! Original Owner-authenticated durable permission; no signing or network path.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::{Kind, Principal, Scope};
use rustsimhub_usage_stats::store::participation::{
    Actor, Binding, Change, Ledger, Receipt, Snapshot,
};
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[cfg(feature = "usage-root-transport-candidate")]
pub(crate) fn initialize_statistics_identity(state: &Path) -> rustsimhub_auth::Result<Value> {
    use rustsimhub_usage_stats::{identity::StatisticsIdentity, store::FileStore};
    if !state.is_absolute() || rustix::process::geteuid().is_root() {
        return Err(Error::Forbidden);
    }
    let identity = StatisticsIdentity::generate().map_err(|_| Error::Forbidden)?;
    let public_key = identity.public_key();
    let store = FileStore::create(state.join("usage-statistics"), identity)
        .map_err(|_| Error::Forbidden)?;
    if store.identity().public_key() != public_key || store.load().is_err() {
        return Err(Error::Forbidden);
    }
    Ok(json!({"statistics_public_key":public_key}))
}

pub(super) struct Service {
    directory: Option<PathBuf>,
    #[cfg(feature = "usage-root-transport-candidate")]
    statistics_directory: Option<PathBuf>,
}
struct Caller {
    principal: Principal,
    binding: Binding,
    instance: String,
}

fn caller(auth: &Arc<Mutex<Auth>>, token: &str, change: bool) -> rustsimhub_auth::Result<Caller> {
    let mut auth = auth.try_lock().map_err(|_| Error::Busy)?;
    let principal = if change {
        auth.recent_owner(token)?
    } else {
        auth.authorize(token, Scope::SystemManage)?
    };
    auth.authorize(token, Scope::SystemManage)?;
    if principal.kind != Kind::Owner || !principal.active {
        return Err(Error::Forbidden);
    }
    if change {
        // Reuse the original recent LOCAL Owner check; mint no Root launch,
        // extend no deadline, and immediately invalidate this unused proof.
        auth.nkn_root_authority(token)?.invalidate();
    }
    Ok(Caller {
        principal,
        binding: Binding {
            core_public_key: auth.public_key(),
            realm: auth.durable_realm()?,
        },
        instance: auth.observer_instance(),
    })
}
fn actor(c: &Caller) -> Actor {
    Actor {
        public_key: c.principal.public_key,
        generation: c.principal.generation,
    }
}
fn recheck(
    auth: &Arc<Mutex<Auth>>,
    token: &str,
    before: &Caller,
    change: bool,
) -> rustsimhub_auth::Result<()> {
    let after = caller(auth, token, change)?;
    if after.principal != before.principal
        || after.binding != before.binding
        || after.instance != before.instance
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn denied<T>(
    value: Result<T, rustsimhub_usage_stats::store::participation::Error>,
) -> rustsimhub_auth::Result<T> {
    value.map_err(|_| Error::Forbidden)
}
fn receipt_view(receipt: &Receipt, current: &str) -> Value {
    json!({"request_id":receipt.change.request_id,"known":true,"state":"saved","core_instance":receipt.change.core_instance,"current_core_instance":current,"enabled":receipt.change.enabled,"disclosure_version":receipt.change.disclosure_version,"expected_generation":receipt.change.expected_generation.to_string(),"generation":receipt.generation.to_string(),"not_found_is_not_proof":true,"reporting_available":false})
}
impl Service {
    pub fn new(state: Option<&Path>) -> Self {
        Self {
            directory: state.map(|p| p.join("usage-participation")),
            #[cfg(feature = "usage-root-transport-candidate")]
            statistics_directory: state.map(|p| p.join("usage-statistics")),
        }
    }
    fn path(&self) -> rustsimhub_auth::Result<&Path> {
        self.directory.as_deref().ok_or(Error::Forbidden)
    }
    #[cfg(feature = "usage-root-transport-candidate")]
    fn open_statistics(
        &self,
        root: &rustsimhub_core::nkn_root::Controller,
    ) -> rustsimhub_auth::Result<rustsimhub_usage_stats::store::FileStore> {
        use rustsimhub_usage_stats::store::{FileStore, StoreError};
        let path = self
            .statistics_directory
            .as_deref()
            .ok_or(Error::Forbidden)?;
        let key = root
            .configured_statistics_key()
            .map_err(|_| Error::Forbidden)?;
        FileStore::open(path, key).map_err(|error| match error {
            StoreError::Busy => Error::Busy,
            _ => Error::Forbidden,
        })
    }

    #[cfg(feature = "usage-root-transport-candidate")]
    pub fn check_in_available(&self, root: &rustsimhub_core::nkn_root::Controller) -> bool {
        self.open_statistics(root).is_ok()
    }

    #[cfg(feature = "usage-root-transport-candidate")]
    pub fn check_in(
        &self,
        auth: &Arc<Mutex<Auth>>,
        root: &Arc<rustsimhub_core::nkn_root::Controller>,
        token: &str,
    ) -> rustsimhub_auth::Result<Value> {
        use rustsimhub_core::{nkn_root::StatisticsTransport, nkn_statistics::SubscriptionSigner};
        use rustsimhub_usage_stats::protocol::{Clock, SystemClock, RENEW_INTERVAL_SECS};

        // Build the signer first, so an absent/stale Owner permission never
        // opens the statistics identity or creates a Root/SDK reference.
        let before = caller(auth, token, true)?;
        let signer = SubscriptionSigner::from_current_owner(auth.clone(), token, self.path()?)
            .map_err(|_| Error::Forbidden)?;
        let store = Arc::new(self.open_statistics(root)?);
        let state = store.load().map_err(|_| Error::Forbidden)?;
        let now = SystemClock.now_secs().map_err(|_| Error::Forbidden)?;
        if state.windows.iter().any(|entry| {
            entry
                .last_success
                .as_ref()
                .is_some_and(|last| last.confirmed_at > now)
        }) {
            return Err(Error::Forbidden);
        }
        let period = now / RENEW_INTERVAL_SECS;
        let window = state
            .windows
            .iter()
            .find(|entry| entry.pending.is_some())
            .or_else(|| {
                state.windows.iter().find(|entry| {
                    entry
                        .last_success
                        .as_ref()
                        .is_none_or(|last| last.period < period)
                })
            });
        let Some(entry) = window else {
            recheck(auth, token, &before, true)?;
            return Ok(json!({"state":"all_confirmed"}));
        };
        let result = StatisticsTransport::new(root.clone(), signer, store)
            .map_err(|_| Error::Forbidden)?
            .run_once(entry.window)
            .map_err(|_| Error::Forbidden)?;
        recheck(auth, token, &before, true)?;
        let state = match result {
            rustsimhub_usage_stats::check_in::Outcome::Pending { .. } => "pending",
            rustsimhub_usage_stats::check_in::Outcome::Unknown { .. } => "unknown",
            rustsimhub_usage_stats::check_in::Outcome::Confirmed { .. } => "confirmed",
            rustsimhub_usage_stats::check_in::Outcome::Due { .. } => "due",
            _ => "unavailable",
        };
        Ok(json!({"state":state,"window":entry.window}))
    }

    pub fn request(
        &self,
        auth: &Arc<Mutex<Auth>>,
        command: AuthCommand,
    ) -> rustsimhub_auth::Result<Value> {
        match command {
            AuthCommand::UsageParticipationStatus { token } => {
                let c = caller(auth, &token, false)?;
                let ledger = denied(Ledger::open_if_present(self.path()?, c.binding.clone()))?;
                let (snapshot, current) = match &ledger {
                    Some(l) => (denied(l.snapshot())?, denied(l.authorized_for(&actor(&c)))?),
                    None => (Snapshot::absent(), false),
                };
                recheck(auth, &token, &c, false)?;
                Ok(
                    json!({"format":1,"available":true,"core_instance":c.instance,"generation":snapshot.generation.to_string(),"enabled":snapshot.enabled,"disclosure_version":snapshot.disclosure_version,"disabled_generation":snapshot.disabled_generation.to_string(),"owner_current":current,"record_exists":ledger.is_some(),"reporting_available":false,"runtime":if !snapshot.enabled {"disabled"} else if current {"authorized_not_connected"} else {"reauthorization_required"}}),
                )
            }
            AuthCommand::UsageParticipationRequest { token, request_id } => {
                let c = caller(auth, &token, false)?;
                let ledger = denied(Ledger::open_if_present(self.path()?, c.binding.clone()))?;
                let receipt = match ledger {
                    Some(l) => denied(l.receipt(&actor(&c), &request_id))?,
                    None => None,
                };
                recheck(auth, &token, &c, false)?;
                Ok(receipt.map_or_else(||json!({"request_id":request_id,"known":false,"current_core_instance":c.instance,"not_found_is_not_proof":true,"reporting_available":false}),|r|receipt_view(&r,&c.instance)))
            }
            AuthCommand::UsageParticipationSet {
                token,
                enabled,
                disclosure_version,
                expected_generation,
                expected_core_instance,
                request_id,
            } => {
                let c = caller(auth, &token, true)?;
                if c.instance != expected_core_instance {
                    return Err(Error::Forbidden);
                }
                let expected = expected_generation
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidInput)?;
                if expected.to_string() != expected_generation {
                    return Err(Error::InvalidInput);
                }
                let edit = Change {
                    request_id: request_id.clone(),
                    core_instance: c.instance.clone(),
                    expected_generation: expected,
                    enabled,
                    disclosure_version,
                };
                let ledger = denied(Ledger::open_if_present(self.path()?, c.binding.clone()))?;
                let receipt = if let Some(ledger) = ledger {
                    denied(ledger.change(actor(&c), edit))?
                } else {
                    denied(Ledger::create(
                        self.path()?,
                        c.binding.clone(),
                        actor(&c),
                        edit,
                    ))?
                    .1
                };
                // Reopen the actual path after publish: no in-memory success or
                // pre-write receipt can stand in for durable readback.
                let readback = denied(Ledger::open_if_present(self.path()?, c.binding.clone()))?
                    .ok_or(Error::Forbidden)?;
                let saved = denied(readback.receipt(&actor(&c), &request_id))?
                    .filter(|r| r == &receipt)
                    .ok_or(Error::Forbidden)?;
                recheck(auth, &token, &c, true)?;
                Ok(receipt_view(&saved, &c.instance))
            }
            _ => Err(Error::Forbidden),
        }
    }
}
