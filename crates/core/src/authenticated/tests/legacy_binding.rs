// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_compat::session::Authority;

async fn request(
    command: AuthCommand,
    auth: Arc<Mutex<Auth>>,
    compat: Option<Arc<Mutex<Authority>>>,
) -> Value {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    let task = tokio::spawn(async move {
        handle(
            &mut receiver,
            auth,
            None,
            None,
            None,
            Providers {
                compat,
                ..Providers::default()
            },
        )
        .await
        .unwrap();
    });
    let request = rustsimhub_wire::AuthRequest {
        protocol: 2,
        id: "binding-status-ipc".into(),
        command,
    };
    write_frame(&mut sender, &serde_json::to_vec(&request).unwrap())
        .await
        .unwrap();
    let response: Response = serde_json::from_slice(
        &timeout(Duration::from_secs(3), read_frame(&mut sender))
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    task.await.unwrap();
    assert_eq!(response.id, "binding-status-ipc");
    assert_eq!(response.protocol, 2);
    response.result
}

#[tokio::test]
async fn real_ipc_binding_readback_requires_compat_and_current_owner_without_mutating_ledger() {
    use rustsimhub_policy::Kind;
    let state = directory();
    let identity = directory();
    let ledger = directory();
    let store = Store::open(state.path()).unwrap();
    let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[21; 32]);
    let nas = SigningKey::from_bytes(&[22; 32]);
    let c = auth
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let c = auth
        .login_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    let token = auth
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned();
    let scopes = vec![Scope::StatusRead];
    auth.open_pairing(&token).unwrap();
    let c = auth
        .pairing_challenge(nas.verifying_key().to_bytes(), Kind::Nas, &scopes)
        .unwrap();
    let pending = auth
        .pairing_finish(&c.id, &nas.sign(&c.transcript).to_bytes())
        .unwrap();
    let approved = auth
        .approve(
            &token,
            &pending,
            nas.verifying_key().to_bytes(),
            Kind::Nas,
            &scopes,
        )
        .unwrap();
    let c = auth
        .login_challenge(nas.verifying_key().to_bytes())
        .unwrap();
    let nas_token = auth
        .login_finish(&c.id, &nas.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned();
    let compat = Arc::new(Mutex::new(
        Authority::open(
            ledger.path(),
            auth.public_key(),
            &auth.durable_realm().unwrap(),
        )
        .unwrap(),
    ));
    let auth = Arc::new(Mutex::new(auth));
    let query = |token: String| AuthCommand::LegacyBindingStatus {
        token,
        core_id: "nas-ipc".into(),
    };
    assert_eq!(
        request(query(token.clone()), auth.clone(), None).await["error"],
        "denied"
    );
    assert_eq!(
        request(query(nas_token), auth.clone(), Some(compat.clone())).await["error"],
        "denied"
    );
    assert_eq!(
        request(query("a".repeat(64)), auth.clone(), Some(compat.clone())).await["error"],
        "denied"
    );
    assert_eq!(
        request(query(token.clone()), auth.clone(), Some(compat.clone())).await,
        json!({"known":false})
    );
    let binding = request(
        AuthCommand::BindLegacy {
            token: token.clone(),
            core_id: "nas-ipc".into(),
            public_key: approved.public_key,
            expected_generation: approved.generation,
            scopes,
        },
        auth.clone(),
        Some(compat.clone()),
    )
    .await;
    assert!(binding["binding_epoch"].is_string());
    let before = std::fs::read(ledger.path().join("legacy.sqlite")).unwrap();
    for _ in 0..2 {
        let value = request(query(token.clone()), auth.clone(), Some(compat.clone())).await;
        assert_eq!(
            value,
            json!({"known":true,"principal_current":true,"binding":binding})
        );
    }
    assert_eq!(
        before,
        std::fs::read(ledger.path().join("legacy.sqlite")).unwrap()
    );
    assert_eq!(
        request(
            AuthCommand::Revoke {
                token: token.clone(),
                public_key: approved.public_key
            },
            auth.clone(),
            Some(compat.clone())
        )
        .await["ok"],
        true
    );
    assert_eq!(
        request(query(token.clone()), auth.clone(), Some(compat.clone())).await
            ["principal_current"],
        false
    );
    assert_eq!(
        request(
            AuthCommand::RevokeLegacy {
                token: token.clone(),
                core_id: "nas-ipc".into()
            },
            auth.clone(),
            Some(compat.clone())
        )
        .await["ok"],
        true
    );
    assert_eq!(
        request(query(token), auth, Some(compat)).await,
        json!({"known":false})
    );
}
