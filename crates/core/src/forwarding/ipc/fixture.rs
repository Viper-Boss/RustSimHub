// SPDX-License-Identifier: AGPL-3.0-only
//! DEV ONLY: real Auth/Store/handlers with synthetic messages and different
//! actual UIDs. No system bus, modem, interface changes or external network.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::Identity;
use rustsimhub_storage::{IncomingSms, LineObservation, LineStatus, Store};
use tokio::signal::unix::{signal, SignalKind};

pub(crate) async fn run(
    socket: &str,
    edge: u32,
    worker: u32,
    state: &str,
    identity: &str,
    target: Option<&str>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let access = Access::new(edge, worker)?;
    let store = Store::open(state)?;
    let mut a = Auth::new(Identity::open(identity)?, store.handle())?;
    let key = SigningKey::from_bytes(&[125; 32]);
    let new = a.current_principal(key.verifying_key().to_bytes()).is_err();
    if new {
        let c = a.bootstrap_challenge(key.verifying_key().to_bytes())?;
        a.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())?;
    }
    let c = a.login_challenge(key.verifying_key().to_bytes())?;
    let token = a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())?;
    let owner_token = token.expose().to_owned();
    let auth = Arc::new(Mutex::new(a));
    let controller = Controller::new(Path::new(state), store.handle());
    let policy = json!({"enabled":true,"targets":[{
        "id":"33333333-3333-4333-8333-333333333333","destination":{
            "kind":"webhook","url":target.unwrap_or("https://example.invalid/synthetic-only"),"bearer":"synthetic"}}],
        "rules":[{"id":"44444444-4444-4444-8444-444444444444",
            "targets":["33333333-3333-4333-8333-333333333333"],"sender_equals":null,
            "body_contains":null,"line":null,"share":"notice"}]});
    if new {
        controller
            .request(
                auth.clone(),
                AuthCommand::ForwardingConfigure {
                    token: owner_token.clone(),
                    expected_generation: 0,
                    request_id: "11111111-1111-4111-8111-111111111111".into(),
                    policy: policy.clone(),
                },
            )
            .await?;
        let line = store
            .handle()
            .observe_line(LineObservation {
                backend: "synthetic-only".into(),
                fingerprint: Some([30; 32]),
                status: LineStatus::Ready,
            })?
            .line;
        for i in 0..2 {
            store.handle().receive_sms(IncomingSms {
                line,
                source: format!("synthetic-source-{i}"),
                sender: "synthetic sender".into(),
                timestamp: "synthetic timestamp".into(),
                text: format!("synthetic body {i}"),
                data: vec![],
            })?;
        }
    }
    let (listener, _runtime) = crate::socket_runtime::SocketRuntime::bind(Path::new(socket))?;
    // Only the disposable Root test parent receives this synthetic Owner
    // bearer through a private inherited stdout FD. Excluded from the
    // production feature set.
    println!(
        "{}",
        json!({"fixture_ready":true,"owner_token":owner_token,"new":new})
    );
    let mut term = signal(SignalKind::terminate())?;
    let mut synthetic = signal(SignalKind::user_defined1())?;
    let mut batch = 0u32;
    let mut tasks = tokio::task::JoinSet::new();
    let deadline = tokio::time::sleep(Duration::from_secs(120));
    tokio::pin!(deadline);
    loop {
        let accepted = tokio::select! {
            _=synthetic.recv()=>{
                batch+=1;
                let line=store.handle().line_epoch()?;
                for i in 0..2 {
                    store.handle().receive_sms(IncomingSms {line,source:format!("synthetic-signal-{batch}-{i}"),
                        sender:"synthetic sender".into(),timestamp:"synthetic timestamp".into(),
                        text:format!("synthetic body batch {batch} {i}"),data:vec![]})?;
                }
                continue;
            },
            _=term.recv()=>break,
            _=&mut deadline=>break,
            _=tasks.join_next(),if !tasks.is_empty()=>continue,
            v=listener.accept()=>v,
        };
        let (mut stream, _) = accepted?;
        let Some(route) = access.route(&stream) else {
            continue;
        };
        if tasks.len() >= 8 {
            continue;
        }
        let a = auth.clone();
        let c = controller.clone();
        tasks.spawn(async move {
            let _ = timeout(Duration::from_secs(10), async {
                if route == Route::Browser {
                    crate::authenticated::handle(
                        &mut stream,
                        a,
                        None,
                        None,
                        None,
                        crate::authenticated::Providers {
                            forwarding: Some(c),
                            ..Default::default()
                        },
                    )
                    .await
                } else {
                    handle(&mut stream, a, c, route).await
                }
            })
            .await;
        });
    }
    drop(listener);
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    controller
        .0
        .lock()
        .map_err(|_| "fixture state poisoned")?
        .retire();
    Ok(())
}
