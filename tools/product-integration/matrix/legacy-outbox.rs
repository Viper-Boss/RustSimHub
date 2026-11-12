// SPDX-License-Identifier: AGPL-3.0-only
// Synthetic format3 data using original Auth/Queue in its separate feature graph.
// No transport, service or real SMS is created. Unknown attempts remain terminal.
use ed25519_dalek::{Signer, SigningKey};
use std::{fs, os::unix::fs::DirBuilderExt, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw() != 986
        || fs::read("/fixture/WEB4_ISOLATED")? != b"private-production-wiring-only\n"
        || std::env::var("RSH_WEB4_PRIVATE_CONTAINER").as_deref() != Ok("yes")
        || std::env::args().len() != 1
    {
        return Err("private UID986 fixture required".into());
    }
    let store = rustsimhub_storage::Store::open("/var/lib/rustsimhub/rustsimhub-v1/core/state")?;
    let mut auth = rustsimhub_auth::Auth::open(
        "/var/lib/rustsimhub/rustsimhub-v1/core/identity",
        store.handle(),
    )?;
    let key = SigningKey::from_bytes(&[73; 32]);
    let challenge = auth.login_challenge(key.verifying_key().to_bytes())?;
    let token = auth.login_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())?;
    let binding = rustsimhub_forwarder::Binding::from_owner(&mut auth, token.expose())?;
    let directory = Path::new("/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding");
    fs::DirBuilder::new().mode(0o700).create(directory)?;
    let mut queue = rustsimhub_forwarder::Queue::open(directory, binding.clone())?;
    let handle = store.handle();
    let line = handle
        .observe_line(rustsimhub_storage::LineObservation {
            backend: "synthetic-matrix-no-transport".into(),
            fingerprint: Some([41; 32]),
            status: rustsimhub_storage::LineStatus::Ready,
        })?
        .line;
    let target = "00000000-0000-4000-8000-000000000101";
    assert_eq!(
        queue.configure_owner(
            &mut auth,
            &handle,
            token.expose(),
            0,
            "00000000-0000-4000-8000-000000000100",
            rustsimhub_forwarder::Policy {
                enabled: true,
                targets: vec![rustsimhub_forwarder::Target {
                    id: target.into(),
                    destination: rustsimhub_forwarder::Destination::Webhook {
                        url: rustsimhub_forwarder::Secret::new(
                            "https://example.invalid/no-transport".into()
                        ),
                        bearer: None,
                    },
                }],
                rules: vec![rustsimhub_forwarder::Rule {
                    id: "00000000-0000-4000-8000-000000000103".into(),
                    targets: vec![target.into()],
                    sender_equals: None,
                    body_contains: None,
                    line: None,
                    share: rustsimhub_forwarder::Share::Notice,
                    conditions: None,
                }],
            }
        )?,
        1
    );
    for (id, outcome) in [
        (1, Some(rustsimhub_forwarder::State::Confirmed)),
        (2, Some(rustsimhub_forwarder::State::Unknown)),
        (3, None),
    ] {
        queue.enqueue_test(
            &binding,
            1,
            &format!("00000000-0000-4000-8000-{id:012x}"),
            target,
            line,
        )?;
        let claim = queue
            .claim(&binding, 1, line)?
            .ok_or("missing real queue claim")?;
        if let Some(state) = outcome {
            queue.finish(&binding, &claim, state)?;
        }
    }
    // The real Owner switch turns the outstanding attempt into Unknown.
    // OFF is durable before handing the fixture to the privileged updater.
    assert_eq!(
        queue.set_enabled_owner(
            &mut auth,
            &handle,
            token.expose(),
            1,
            "00000000-0000-4000-8000-000000000102",
            false
        )?,
        2
    );
    assert!(!queue.status()?.enabled);
    drop(handle);
    drop(queue);
    drop(auth);
    drop(store);
    Ok(())
}
