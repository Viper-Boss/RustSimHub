// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(all(test, target_os = "linux"))]
//! Manual isolated-container target. Real SO_PEERCRED, synthetic trusted verifier.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{
    recovery_capability::{
        RecoveryAction, RecoveryProofContext, RecoveryRequestVerifierV1, VerifiedRecoveryRequest,
    },
    Auth,
};
use rustsimhub_core::{
    recovery_channel::RecoveryChannel,
    recovery_transport::{Error as TransportError, Roles, SourceGuard},
};
use rustsimhub_storage::{RecoveryGrant, Store};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};
use zeroize::Zeroizing;

struct Fixture {
    _data: tempfile::TempDir,
    _identity: tempfile::TempDir,
    _store: Store,
    auth: Auth,
    grant: RecoveryGrant,
}
fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let identity = tempfile::tempdir().unwrap();
    let backup = tempfile::tempdir().unwrap();
    for d in [&data, &identity, &backup] {
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = Store::open(data.path()).unwrap();
    let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[61; 32]);
    let challenge = auth
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&challenge.id, &owner.sign(&challenge.transcript).to_bytes())
        .unwrap();
    drop(auth);
    drop(store);
    let store = Store::migrate_recovery_grants(data.path(), backup.path()).unwrap();
    let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
    let challenge = auth
        .login_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    let token = auth
        .login_finish(&challenge.id, &owner.sign(&challenge.transcript).to_bytes())
        .unwrap();
    let grant = auth
        .grant_recovery_key(
            token.expose(),
            SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes(),
            40000,
            0,
            "00000000-0000-4000-8000-000000000001".into(),
        )
        .unwrap();
    Fixture {
        _data: data,
        _identity: identity,
        _store: store,
        auth,
        grant,
    }
}
struct Verifier {
    calls: Arc<AtomicUsize>,
    nonces: Arc<Mutex<Vec<[u8; 32]>>>,
}
impl RecoveryRequestVerifierV1 for Verifier {
    fn verify_request(
        &self,
        context: &RecoveryProofContext,
        envelope: &[u8],
    ) -> rustsimhub_auth::Result<VerifiedRecoveryRequest> {
        if envelope.len() != 1 || ![0x11, 0x12].contains(&envelope[0]) {
            return Err(rustsimhub_auth::Error::Forbidden);
        }
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.nonces
            .lock()
            .unwrap()
            .push(*context.binding().transport_nonce());
        VerifiedRecoveryRequest::from_verified_adapter(
            context.binding().clone(),
            vec![0x55; 16],
            [envelope[0]; 16],
            RecoveryAction::ReadStatus,
            Zeroizing::new(b"fixture-status".to_vec()),
            Duration::from_secs(30),
        )
    }
}
async fn accept(listener: &UnixListener, expected_uid: u32) -> UnixStream {
    let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let cred = stream.peer_cred().unwrap();
    assert_eq!(cred.uid(), expected_uid);
    assert_eq!(cred.gid(), 1002);
    assert!(cred.pid().unwrap() > 0);
    stream
}

#[tokio::test]
async fn isolated_real_uid_source_and_owned_auth_channel() {
    assert_eq!(
        rustix::process::geteuid().as_raw(),
        1001,
        "run only through the isolated UID fixture runner"
    );
    assert_eq!(rustix::process::getegid().as_raw(), 1001);
    let status = fs::read_to_string("/proc/self/status").unwrap();
    assert!(status.contains("CapEff:\t0000000000000000"));
    assert!(status.contains("NoNewPrivs:\t1"));
    let directory = std::env::var("RUSTSIMHUB_UID_FIXTURE_DIR")
        .expect("explicit isolated fixture directory required");
    let directory = Path::new(&directory);
    let metadata = fs::metadata(directory).unwrap();
    assert_eq!(metadata.uid(), 1001);
    assert_eq!(metadata.gid(), 1002);
    assert_eq!(metadata.mode() & 0o7777, 0o2750);
    let mut f = fixture();
    let guard = SourceGuard::new(Roles::new(1004, 1005, 1003, 1002).unwrap()).unwrap();
    let listener = UnixListener::bind(directory.join("core.sock")).unwrap();
    fs::set_permissions(
        directory.join("core.sock"),
        fs::Permissions::from_mode(0o660),
    )
    .unwrap();
    let socket = fs::metadata(directory.join("core.sock")).unwrap();
    assert_eq!(socket.uid(), 1001);
    assert_eq!(socket.gid(), 1002);
    assert_eq!(socket.mode() & 0o777, 0o660);
    println!("UID_READY");
    for uid in [0, 1001, 1003, 1004, 1005, 1006] {
        let stream = accept(&listener, uid).await;
        assert!(matches!(guard.accept(stream), Err(TransportError::Denied)));
        println!("UID_CASE denied_{uid}");
    }
    let peer = guard.accept(accept(&listener, 1002).await).unwrap();
    assert!(matches!(
        RecoveryChannel::attach(&mut f.auth, peer),
        Err(rustsimhub_core::recovery_channel::Error::Auth(
            rustsimhub_auth::Error::Forbidden
        ))
    ));
    println!("UID_CASE no_provider");
    let calls = Arc::new(AtomicUsize::new(0));
    let nonces = Arc::new(Mutex::new(Vec::new()));
    f.auth
        .set_recovery_verifier(Some(Arc::new(Verifier {
            calls: calls.clone(),
            nonces: nonces.clone(),
        })))
        .unwrap();
    let peer = guard.accept(accept(&listener, 1002).await).unwrap();
    let context = peer.context().unwrap();
    assert_eq!(context.uid(), 1002);
    assert_eq!(context.gid(), 1002);
    assert!(context.pid() > 0);
    let first_nonce = *context.connection_nonce();
    assert_ne!(first_nonce, [0; 32]);
    let channel = RecoveryChannel::attach(&mut f.auth, peer).unwrap();
    let (channel, packet) = channel.read().await.unwrap();
    let verified = channel
        .prepare(&mut f.auth, f.grant.clone(), packet)
        .unwrap()
        .verify()
        .await
        .unwrap();
    let (channel, first) = verified.finish(&mut f.auth).unwrap();
    f.auth
        .check_recovery_capability(&first.capability, RecoveryAction::ReadStatus, &first.body)
        .unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(nonces.lock().unwrap()[0], first_nonce);
    println!("UID_CASE primary_authorized");
    assert!(matches!(
        guard.accept(accept(&listener, 1002).await),
        Err(TransportError::Denied)
    ));
    f.auth
        .check_recovery_capability(&first.capability, RecoveryAction::ReadStatus, &first.body)
        .unwrap();
    println!("UID_CASE occupied_slot_denied");
    drop(channel);
    assert!(matches!(
        f.auth.check_recovery_capability(
            &first.capability,
            RecoveryAction::ReadStatus,
            &first.body
        ),
        Err(rustsimhub_auth::Error::Forbidden)
    ));
    println!("UID_CASE drop_fenced");
    let peer = guard.accept(accept(&listener, 1002).await).unwrap();
    let next_nonce = *peer.context().unwrap().connection_nonce();
    assert_ne!(next_nonce, first_nonce);
    let channel = RecoveryChannel::attach(&mut f.auth, peer).unwrap();
    let (channel, packet) = channel.read().await.unwrap();
    let verified = channel
        .prepare(&mut f.auth, f.grant.clone(), packet)
        .unwrap()
        .verify()
        .await
        .unwrap();
    let (channel, next) = verified.finish(&mut f.auth).unwrap();
    f.auth
        .check_recovery_capability(&next.capability, RecoveryAction::ReadStatus, &next.body)
        .unwrap();
    assert!(f
        .auth
        .check_recovery_capability(&first.capability, RecoveryAction::ReadStatus, &first.body)
        .is_err());
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(nonces.lock().unwrap()[1], next_nonce);
    drop(channel);
    assert!(f
        .auth
        .check_recovery_capability(&next.capability, RecoveryAction::ReadStatus, &next.body)
        .is_err());
    println!("UID_CASE reconnect_fresh_nonce");
}
