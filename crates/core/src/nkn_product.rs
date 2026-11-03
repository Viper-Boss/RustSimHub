// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit actual Root/SDK provider. No activation on construction/read; the
//! original Auth instance, three-reference supervisor and fixed ports remain.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_wire::AuthCommand;
use serde_json::Value;
use std::sync::{Arc, Mutex};
#[cfg(feature = "root-provider-candidate")]
mod candidate;
#[cfg(feature = "root-provider-candidate")]
mod membership;
#[derive(Clone)]
pub(crate) struct Controller {
    #[cfg(feature = "root-provider-candidate")]
    inner: Arc<candidate::Inner>,
}
impl Controller {
    #[cfg(feature = "root-provider-candidate")]
    pub fn new(auth: Arc<Mutex<Auth>>) -> Self {
        Self {
            inner: Arc::new(candidate::Inner::new(auth)),
        }
    }
    pub async fn request(
        &self,
        auth: Arc<Mutex<Auth>>,
        command: AuthCommand,
    ) -> rustsimhub_auth::Result<Value> {
        #[cfg(feature = "root-provider-candidate")]
        {
            let inner = self.inner.clone();
            if !Arc::ptr_eq(&inner.auth, &auth) {
                return Err(Error::Forbidden);
            }
            tokio::task::spawn_blocking(move || inner.request(command))
                .await
                .map_err(|_| Error::Forbidden)?
        }
        #[cfg(not(feature = "root-provider-candidate"))]
        {
            let _ = (auth, command);
            Err(Error::Forbidden)
        }
    }
    #[cfg(feature = "root-provider-candidate")]
    pub fn root(&self) -> Arc<rustsimhub_core::nkn_root::Controller> {
        self.inner.root.clone()
    }
    #[cfg(feature = "root-provider-candidate")]
    pub async fn changed(&self) {
        self.inner.wake.notified().await;
    }
}

#[cfg(feature = "root-provider-candidate")]
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "product-providers-candidate")]
pub fn activate_explicit() {
    ACTIVE.store(true, std::sync::atomic::Ordering::Release);
}
pub fn construct(auth: Arc<Mutex<Auth>>) -> Option<Controller> {
    #[cfg(feature = "root-provider-candidate")]
    {
        if ACTIVE.load(std::sync::atomic::Ordering::Acquire) {
            Some(Controller::new(auth))
        } else {
            None
        }
    }
    #[cfg(not(feature = "root-provider-candidate"))]
    {
        let _ = auth;
        None
    }
}
/// The future is polled by the ORIGINAL server select and dropped before its
/// shutdown drain. No detached Root/listener task survives host cancellation.
pub async fn run(
    controller: Option<Controller>,
    auth: Arc<Mutex<Auth>>,
    metrics: Option<crate::observer::Metrics>,
    sender: Option<crate::sender::Controller>,
    calls: Option<crate::calls::Controller>,
    providers: crate::authenticated::Providers,
) {
    #[cfg(feature = "root-provider-candidate")]
    if let Some(controller) = controller {
        let ports = crate::nkn_business::CorePorts {
            auth,
            metrics,
            sender,
            calls,
            providers,
        };
        candidate::drive(controller, ports).await;
        return;
    }
    #[cfg(not(feature = "root-provider-candidate"))]
    let _ = (controller, auth, metrics, sender, calls, providers);
    std::future::pending::<()>().await;
}

#[cfg(all(test, feature = "root-provider-candidate"))]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_wire::{AuthRequest, NknOwner, Response, AUTH_PROTOCOL};
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixStream;
    fn id(n: u32) -> String {
        format!("{n:08x}-1111-4111-8111-111111111111")
    }
    struct Fixture {
        _state: tempfile::TempDir,
        _identity: tempfile::TempDir,
        _store: rustsimhub_storage::Store,
        auth: Arc<Mutex<Auth>>,
        controller: Controller,
        token: String,
        client: String,
        instance: String,
    }
    impl Fixture {
        fn new() -> Self {
            let state = tempfile::tempdir().unwrap();
            let identity = tempfile::tempdir().unwrap();
            for d in [state.path(), identity.path()] {
                std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            let store = rustsimhub_storage::Store::open(state.path()).unwrap();
            let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
            let key = SigningKey::from_bytes(&[91; 32]);
            let challenge = auth
                .bootstrap_challenge(key.verifying_key().to_bytes())
                .unwrap();
            auth.bootstrap_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
                .unwrap();
            let login = |auth: &mut Auth, key: &SigningKey| {
                let c = auth
                    .login_challenge(key.verifying_key().to_bytes())
                    .unwrap();
                auth.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
                    .unwrap()
                    .expose()
                    .to_owned()
            };
            let token = login(&mut auth, &key);
            let owner = store
                .handle()
                .principal(key.verifying_key().to_bytes())
                .unwrap()
                .unwrap();
            let client = SigningKey::from_bytes(&[92; 32]);
            store
                .handle()
                .approve_client(
                    owner,
                    client.verifying_key().to_bytes(),
                    rustsimhub_policy::Kind::Client,
                    rustsimhub_policy::ALL_SCOPES,
                )
                .unwrap();
            let client = login(&mut auth, &client);
            let instance = auth.observer_instance();
            let auth = Arc::new(Mutex::new(auth));
            let controller = Controller::new(auth.clone());
            Self {
                _state: state,
                _identity: identity,
                _store: store,
                auth,
                controller,
                token,
                client,
                instance,
            }
        }
        fn off(&self, n: u32, revision: u64) -> AuthCommand {
            AuthCommand::SetNknEnabled {
                token: self.token.clone(),
                owner: NknOwner::Remote,
                enabled: false,
                expected_revision: revision.to_string(),
                expected_core_instance: self.instance.clone(),
                request_id: id(n),
            }
        }
        async fn request(&self, command: AuthCommand) -> Value {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            let auth = self.auth.clone();
            let controller = self.controller.clone();
            let task = tokio::spawn(async move {
                crate::authenticated::handle(
                    &mut server,
                    auth,
                    None,
                    None,
                    None,
                    crate::authenticated::Providers {
                        nkn: Some(controller),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            });
            crate::linux::write_frame(
                &mut client,
                &serde_json::to_vec(&AuthRequest {
                    protocol: AUTH_PROTOCOL,
                    id: "nkn-test".into(),
                    command,
                })
                .unwrap(),
            )
            .await
            .unwrap();
            let response: Response =
                serde_json::from_slice(&crate::linux::read_frame(&mut client).await.unwrap())
                    .unwrap();
            task.await.unwrap();
            response.result
        }
    }
    #[tokio::test]
    async fn original_owner_rpc_reads_off_without_io_and_rejects_delegated_or_missing_tokens() {
        let f = Fixture::new();
        let value = f
            .request(AuthCommand::NknStatus {
                token: f.token.clone(),
            })
            .await;
        assert_eq!(value["available"], true);
        assert_eq!(value["revision"], "0");
        assert_eq!(value["provider"]["status"]["source"], "unavailable");
        assert_eq!(value["provider"]["original_process_exit_verified"], false);
        assert!(f.controller.root().snapshot().unwrap().is_none());
        for token in [f.client.clone(), "a".repeat(64)] {
            assert_eq!(
                f.request(AuthCommand::NknStatus { token }).await["error"],
                "denied"
            );
        }
        assert!(f.controller.root().snapshot().unwrap().is_none());
    }
    #[tokio::test]
    async fn no_op_off_has_one_cas_receipt_and_eviction_never_replays_an_old_revision() {
        let f = Fixture::new();
        let first = f.request(f.off(1, 0)).await;
        assert_eq!(first["state"], "accepted");
        assert_eq!(first["admitted_revision"], "1");
        assert_eq!(f.request(f.off(1, 0)).await, first);
        for n in 2..=70 {
            assert_eq!(
                f.request(f.off(n, u64::from(n) - 1)).await["state"],
                "accepted"
            );
        }
        assert_eq!(f.request(f.off(1, 0)).await["error"], "denied");
        let absent = f
            .request(AuthCommand::NknRequest {
                token: f.token.clone(),
                request_id: id(1),
                expected_core_instance: f.instance.clone(),
            })
            .await;
        assert_eq!(absent["known"], false);
        assert_eq!(absent["not_found_is_not_proof"], true);
        let status = f
            .request(AuthCommand::NknStatus {
                token: f.token.clone(),
            })
            .await;
        assert_eq!(status["revision"], "70");
        assert!(f.controller.root().snapshot().unwrap().is_none());
    }
    #[tokio::test]
    async fn context_mismatch_and_shutdown_cannot_admit_a_late_root_action() {
        let f = Fixture::new();
        let mut command = f.off(1, 0);
        if let AuthCommand::SetNknEnabled {
            expected_core_instance,
            ..
        } = &mut command
        {
            *expected_core_instance = "a".repeat(64);
        }
        assert_eq!(f.request(command).await["error"], "denied");
        assert_eq!(
            f.request(AuthCommand::NknStatus {
                token: f.token.clone()
            })
            .await["revision"],
            "0"
        );
        f.controller
            .inner
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        f.controller.root().shutdown();
        assert_eq!(f.request(f.off(2, 0)).await["error"], "denied");
        assert!(f
            .controller
            .root()
            .enable(&f.token, rustsimhub_nkn_agent::Owner::ManualEngine)
            .is_err());
        assert!(f.controller.root().snapshot().unwrap().is_none());
    }
}
