// SPDX-License-Identifier: AGPL-3.0-only
//! Original Core Auth/dispatcher/actor behind actual Edge and Go NAS client.
use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::UnixListener;

#[tokio::test]
#[ignore = "explicit bounded loopback Edge/Core/Go NAS fixture only"]
async fn original_core_nas_fixture() {
    let root = PathBuf::from(std::env::var_os("RSH_A3_NAS_CORE").unwrap());
    assert_eq!(std::env::var("RSH_A3_NAS").unwrap(), "1");
    assert_eq!(std::fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
    let f = fixture(false).await;
    let auth = f.worker.auth.clone();
    let nas = SigningKey::from_bytes(&[74; 32]);
    f.store
        .handle()
        .approve_client(
            f.owner.clone(),
            nas.verifying_key().to_bytes(),
            rustsimhub_policy::Kind::Nas,
            rustsimhub_policy::mask(&[Scope::StatusRead, Scope::CallsRead]).unwrap(),
        )
        .unwrap();
    let nas_token = {
        let mut a = auth.lock().unwrap();
        let c = a.login_challenge(nas.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &nas.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    let line = f.worker.binding().unwrap().context.line.0;
    let listener = UnixListener::bind(root.join("core.sock")).unwrap();
    std::fs::set_permissions(
        root.join("core.sock"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    rustsimhub_auth::write_private_new(
        &root.join("credentials.json"),
        &serde_json::to_vec(
            &json!({"first":f.first,"second":f.second,"nas":nas_token,"line":line}),
        )
        .unwrap(),
    )
    .unwrap();
    let calls = f.controller.clone();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let count_path = root.join("core-request-count");
    std::fs::write(&count_path, b"0").unwrap();
    let endpoint = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let n = count.fetch_add(1, Ordering::SeqCst) + 1;
            std::fs::write(&count_path, n.to_string()).unwrap();
            crate::authenticated::handle(
                &mut stream,
                auth.clone(),
                None,
                None,
                Some(calls.clone()),
                crate::authenticated::Providers::default(),
            )
            .await
            .unwrap();
        }
    });
    let worker = tokio::spawn(f.worker.run());
    let deadline = Instant::now() + Duration::from_secs(90);
    while !root.join("done").exists() {
        assert!(
            Instant::now() < deadline,
            "NAS fixture exceeded bounded deadline"
        );
        if root.join("revoke").exists() && !root.join("revoked").exists() {
            f.store
                .handle()
                .revoke_client(
                    f.owner.clone(),
                    SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes(),
                )
                .unwrap();
            std::fs::write(root.join("revoked"), b"1").unwrap();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    f.controller.shutdown();
    worker.await.unwrap();
    endpoint.abort();
    let _ = endpoint.await;
    assert_eq!(f.data.creates.load(Ordering::SeqCst), 1);
    assert_eq!(f.data.starts.load(Ordering::SeqCst), 1);
    assert_eq!(f.data.tones.load(Ordering::SeqCst), 0);
    assert_eq!(f.data.hangups.load(Ordering::SeqCst), 1);
    let result = json!({"actual_core_auth_dispatcher_actor":true,"creates":1,"starts":1,"tones":0,"hangups":1,"requests":requests.load(Ordering::SeqCst),"physical_modem":false});
    std::fs::write(
        root.join("core-result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
    f.server.close().await.unwrap();
}
