// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit disposable-container target. Never run by ordinary cargo test.
#![forbid(unsafe_code)]
use rustsimhub_ble::{
    core_port::{CoreEndpoint, UnixSecurePort},
    framing::{InputKind, OpaqueBytes},
    ports::{Epoch, Peer, SecurePortV1, SecureReply},
    Error,
};
use std::{path::PathBuf, time::Duration};
fn message() -> OpaqueBytes {
    OpaqueBytes::new(b"opaque-input".to_vec()).unwrap()
}
fn peer() -> Peer {
    Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF").unwrap()
}
#[tokio::test]
async fn installed_roles_and_opaque_io() {
    let case = std::env::var("PORT_CASE").expect("explicit fixture required");
    let path = PathBuf::from(std::env::var_os("PORT_SOCKET").unwrap());
    let endpoint = CoreEndpoint::from_installed_roles(path, 1001, 1002, 1002).unwrap();
    let result = UnixSecurePort::connect(endpoint).await;
    if matches!(
        case.as_str(),
        "wrong_kernel" | "bad_mode" | "symlink" | "extra_group" | "wrong_local"
    ) {
        assert!(
            matches!(result, Err(Error::Unavailable)),
            "role/metadata must deny before input"
        );
        println!("PORT_CASE {case} pass");
        return;
    }
    let mut port = result.unwrap();
    let epoch = Epoch::new([12; 16], 1).unwrap();
    if case == "roundtrip" {
        let reply = port
            .receive(epoch, peer(), InputKind::Auth, message())
            .await
            .unwrap();
        assert!(matches!(reply, SecureReply::Handshake(ref b) if b.as_slice() == b"opaque-reply"));
        let reply = port
            .receive(epoch, peer(), InputKind::Command, message())
            .await
            .unwrap();
        assert!(
            matches!(reply, SecureReply::Application(ref b) if b.as_slice() == b"opaque-reply")
        );
        port.disconnected(epoch, peer()).await.unwrap();
    } else {
        let fence = port.fence();
        let mut io = port.receive(epoch, peer(), InputKind::Auth, message());
        if case == "cancel" || case == "fence" {
            assert!(tokio::time::timeout(Duration::from_millis(100), &mut io)
                .await
                .is_err());
            if case == "fence" {
                fence.close();
                assert!(tokio::time::timeout(Duration::from_millis(500), &mut io)
                    .await
                    .unwrap()
                    .is_err());
            }
            drop(io);
        } else {
            let reply = tokio::time::timeout(Duration::from_secs(6), &mut io)
                .await
                .unwrap();
            assert_eq!(
                reply.unwrap_err(),
                if case == "timeout" {
                    Error::Deadline
                } else {
                    Error::Protocol
                }
            );
            drop(io);
        }
        assert_eq!(
            port.receive(epoch, peer(), InputKind::Auth, message())
                .await
                .unwrap_err(),
            Error::StaleContext
        );
        // No transparent reconnect. A different context cannot replace it.
        assert_eq!(
            port.receive(
                Epoch::new([12; 16], 2).unwrap(),
                peer(),
                InputKind::Auth,
                message()
            )
            .await
            .unwrap_err(),
            Error::StaleContext
        );
        assert_eq!(
            port.disconnected(Epoch::new([12; 16], 2).unwrap(), peer())
                .await,
            Err(Error::StaleContext)
        );
        port.disconnected(epoch, peer()).await.unwrap();
    }
    println!("PORT_CASE {case} pass");
}
