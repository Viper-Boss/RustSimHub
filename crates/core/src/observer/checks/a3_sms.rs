// SPDX-License-Identifier: AGPL-3.0-only
//! Named MM MemoryFull failures on a private D-Bus, no SIM or modem writes.
use super::*;
use rustsimhub_wire::AuthCommand;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.ModemManager1.Error.MobileEquipment")]
enum FullError {
    MemoryFull(String),
}
fn full() -> FullError {
    let error = FullError::MemoryFull("synthetic SIM storage full".into());
    assert_eq!(
        zbus::DBusError::name(&error).as_str(),
        "org.freedesktop.ModemManager1.Error.MobileEquipment.MemoryFull"
    );
    error
}
struct FullMessaging {
    creates: Arc<AtomicUsize>,
    fail_create: bool,
}
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Messaging")]
impl FullMessaging {
    fn create(
        &self,
        _properties: HashMap<String, OwnedValue>,
    ) -> std::result::Result<OwnedObjectPath, FullError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        if self.fail_create {
            Err(full())
        } else {
            Ok(path(9000))
        }
    }
}
struct FullSms {
    sends: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sms")]
impl FullSms {
    fn send(&self) -> std::result::Result<(), FullError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        Err(full())
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        1
    }
    #[zbus(property)]
    fn pdu_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn number(&self) -> &str {
        "+12025550123"
    }
    #[zbus(property)]
    fn text(&self) -> &str {
        "synthetic outgoing"
    }
    #[zbus(property)]
    fn data(&self) -> Vec<u8> {
        vec![]
    }
}

#[tokio::test]
async fn actual_named_sim_memory_full_at_create_or_send_keeps_unknown_and_never_retries() {
    for fail_create in [true, false] {
        let (server, client, _sim) = peers().await;
        server
            .object_server()
            .remove::<FakeMessaging, _>("/org/freedesktop/ModemManager1/Modem/0")
            .await
            .unwrap();
        let creates = Arc::new(AtomicUsize::new(0));
        let sends = Arc::new(AtomicUsize::new(0));
        server
            .object_server()
            .at(
                "/org/freedesktop/ModemManager1/Modem/0",
                FullMessaging {
                    creates: creates.clone(),
                    fail_create,
                },
            )
            .await
            .unwrap();
        server
            .object_server()
            .at(
                path(9000),
                FullSms {
                    sends: sends.clone(),
                },
            )
            .await
            .unwrap();
        let state = directory();
        let identity = directory();
        let store = rustsimhub_storage::Store::open(state.path()).unwrap();
        let h = store.handle();
        let (auth, token, principal) = super::sender_recovery::login(&h, identity.path(), true);
        let line = h
            .observe_line(LineObservation {
                backend: "synthetic-sender".into(),
                fingerprint: Some([7; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let (_fence, guard) = tokio::sync::watch::channel(watch::Notice::default());
        let binding = Binding {
            connection: client,
            context: rustsimhub_storage::SendContext {
                line,
                backend: "synthetic-sender".into(),
                owner: ":1.20".into(),
                modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
                fingerprint: [7; 32],
            },
            iccid: "8901234567890123456".into(),
            sim: "/org/freedesktop/ModemManager1/SIM/0".try_into().unwrap(),
            guard,
            fence: 0,
            seen: Instant::now(),
        };
        let (_publisher, bindings) = tokio::sync::watch::channel(Some(binding));
        let (controller, worker) =
            crate::sender::channel(Arc::new(Mutex::new(auth)), h.clone(), bindings);
        let exercise = async {
            let prepared = controller
                .request(AuthCommand::PrepareSms {
                    token: token.clone(),
                    line_epoch: line.0,
                    idempotency_key: "a3-sim-full".into(),
                    recipient: "+12025550123".into(),
                    text: "synthetic outgoing".into(),
                })
                .await
                .unwrap();
            let id = prepared["id"].as_str().unwrap().to_owned();
            let submit = controller
                .request(AuthCommand::SubmitSms {
                    token: token.clone(),
                    operation_id: id.clone(),
                })
                .await
                .unwrap();
            assert_eq!(submit["state"], "unknown");
            for _ in 0..2 {
                assert_eq!(
                    controller
                        .request(AuthCommand::SubmitSms {
                            token: token.clone(),
                            operation_id: id.clone()
                        })
                        .await
                        .unwrap()["state"],
                    "unknown"
                );
                assert_eq!(
                    controller
                        .request(AuthCommand::SmsOperation {
                            token: token.clone(),
                            operation_id: id.clone()
                        })
                        .await
                        .unwrap()["state"],
                    "unknown"
                );
            }
            assert_eq!(
                h.sender_operation(id, principal).unwrap().state,
                rustsimhub_domain::SmsState::Unknown
            );
            drop(controller);
        };
        tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(worker.run(), exercise);
        })
        .await
        .unwrap();
        assert_eq!(creates.load(Ordering::SeqCst), 1);
        assert_eq!(sends.load(Ordering::SeqCst), usize::from(!fail_create));
        server.close().await.unwrap();
    }
}
