// SPDX-License-Identifier: AGPL-3.0-only
//! One actual non-cfg(test) normal signaling Host at UID985, original Core
//! business controller at UID986 and actual native worker at UID984. Only the
//! private ModemManager/PCM/peer endpoints are synthetic external backends.
use super::*;
use rustsimhub_media::remote_signaling::{Host, Prepare};
#[tokio::test]
#[ignore = "explicit three-UID normal Core private TURN fixture"]
async fn actual_uid_edge_normal_host() {
    assert_eq!(rustix::process::geteuid().as_raw(), 985);
    no_capabilities();
    assert_eq!(
        std::env::var("RUSTSIMHUB_NORMAL_CORE_FIXTURE").as_deref(),
        Ok("1")
    );
    let server = tls::Server::start().await;
    let (mut peer, offer) = remote::Session::offer(server.registration()).await.unwrap();
    let key = tls::fingerprint(&offer);
    publish(
        "/fixture/edge/offer.json",
        serde_json::json!({"key":key,"address":server.address,"ca_der":server.ca_der()}),
    );
    let core = fixture_file("/fixture/core/ready.json").await;
    assert_eq!(core["normal_auth_and_calls_controller"], true);
    assert!(core.get("credentials").is_none());
    fixture_file("/fixture/media/ready.json").await;
    let lease: rustsimhub_media::remote_signaling::CallLease =
        serde_json::from_value(core["lease"].clone()).unwrap();
    // A real normally signed-in client lacking media.use cannot connect to the
    // worker or create a TURN allocation through this production host API.
    let denied = Prepare {
        token: core["denied_token"].as_str().unwrap().into(),
        lease: lease.clone(),
        relay_handle: "approved".into(),
        expected_peer: key,
        offer: offer.clone(),
    };
    assert!(Host::prepare(denied).await.is_err());
    let request = Prepare {
        token: core["token"].as_str().unwrap().into(),
        lease,
        relay_handle: "approved".into(),
        expected_peer: key,
        offer,
    };
    let (mut host, answer) = Host::prepare(request).await.unwrap();
    peer.accept_answer(&answer, tls::fingerprint(&answer))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            peer.service().await.unwrap();
            let status = host.poll().await.unwrap();
            assert!(!status.pcm_opened);
            if status.phase == Phase::Verified && peer.verified_peer().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let peer_key = peer.verified_peer().unwrap().fingerprint();
    let (mut authority, context, permit) = tls::permit(peer_key);
    peer.authorize(permit, peer_key).unwrap();
    let status = host.activate().await.unwrap();
    let serialized = serde_json::to_string(&status).unwrap();
    for name in ["token", "ticket", "peer_key", "binding", "relay_handle"] {
        assert!(!serialized.contains(name));
    }
    let mut sequence = 0;
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            authority.refresh(Some(&context), now_ms()).unwrap();
            peer.service().await.unwrap();
            peer.send(&rustsimhub_media::Frame {
                samples: [456; 320],
                sequence,
                at_ms: now_ms(),
            })
            .await
            .unwrap();
            sequence += 1;
            let status = host.poll().await.unwrap();
            if status.counters.sent > 0
                && status.counters.received > 0
                && peer.receive().unwrap().is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    publish(
        "/fixture/edge/revoke.json",
        serde_json::json!({"request_owner_revocation":true}),
    );
    let media = fixture_file("/fixture/media/done.json").await;
    let core = fixture_file("/fixture/core/done.json").await;
    assert_eq!(media["closed"], 1);
    assert_eq!(core["original_owner_revocation"], true);
    assert!(host.poll().await.is_err());
    assert!(host.activate().await.is_err());
    publish(
        "/fixture/edge/done.json",
        serde_json::json!({"uid":985,"normal_host_non_test_library":true,"normal_auth_and_original_call_lease":true,
        "actual_core_peer_uid":986,"forced_relay_opus_both_directions":true,"owner_revocation_stops_media":true,"credentials_exposed_by_host":false}),
    );
    drop(host);
    peer.close().await;
}
