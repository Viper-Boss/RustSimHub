// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[tokio::test]
async fn waits_for_acknowledgement_before_next_indication() {
    let (local, remote) = Socket::pair(Domain::UNIX, Type::SEQPACKET, None).unwrap();
    local.set_nonblocking(true).unwrap();
    remote.set_nonblocking(true).unwrap();
    let stream = Arc::new(AsyncFd::new(local).unwrap());
    let remote = AsyncFd::new(remote).unwrap();
    let reply = Reply {
        stream,
        frames: vec![vec![7, 8], vec![9]],
        ready: true,
    };
    let device = async {
        let mut first = [0; 3];
        let n = remote
            .async_io(tokio::io::Interest::READABLE, |mut fd| fd.read(&mut first))
            .await
            .unwrap();
        assert_eq!(&first[..n], &[7, 8]);
        assert_eq!(
            remote.get_ref().read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        remote.get_ref().write_all(&[1]).unwrap();
        let n = remote
            .async_io(tokio::io::Interest::READABLE, |mut fd| fd.read(&mut first))
            .await
            .unwrap();
        assert_eq!(&first[..n], &[9]);
        remote.get_ref().write_all(&[1]).unwrap();
    };
    let (result, ()) = tokio::join!(send(&reply), device);
    result.unwrap();
}

use crate::bluez_control::tests::{Bus, Calls};
use rustsimhub_airsim::base64url;
use serde_json::{json, Value as Json};
use zbus::zvariant::{OwnedObjectPath, Value};

const ATT_MTU: u16 = 185;

fn options(device: &str) -> Options {
    HashMap::from([
        (
            "device".into(),
            Value::from(ObjectPath::try_from(device).unwrap())
                .try_into()
                .unwrap(),
        ),
        ("link".into(), Value::from("LE").try_into().unwrap()),
        ("mtu".into(), OwnedValue::from(ATT_MTU)),
    ])
}

#[tokio::test]
async fn private_bus_runs_ios_wire_flow_and_rejects_another_sender_and_peer() {
    timeout(Duration::from_secs(10), async {
        let bus = Bus::new().await;
        let bluez = bus.bluez(Arc::new(Calls::default()), true).await;
        let imposter = bus.connect().await;
        let server = bus.connect().await;
        let name = server.unique_name().unwrap().as_str();
        let control = BluezControl::bind(
            server.clone(),
            bluez.unique_name().unwrap().as_str(),
            Adapter::Hci0,
        )
        .await
        .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("state");
        let (sender, mut replies) = mpsc::channel(1);
        let context = Arc::new(Context {
            scope: control.callback_scope(),
            state: Mutex::new(State {
                disk: store::Disk::open(&state_path).unwrap(),
                deadline: Some(Instant::now() + voble::PAIRING_WINDOW),
                accepting: true,
                peer: None,
            }),
            changed: Notify::new(),
            revoked: Notify::new(),
            replies: sender,
        });
        export(&server, &context, Arc::new(Notify::new()))
            .await
            .unwrap();
        registration(&control, false, true).await.unwrap();
        registration(&control, true, true).await.unwrap();
        let objects = Proxy::new(&bluez, name, ROOT, "org.freedesktop.DBus.ObjectManager")
            .await
            .unwrap();
        let exported: HashMap<OwnedObjectPath, HashMap<String, Options>> =
            objects.call("GetManagedObjects", &()).await.unwrap();
        assert_eq!(exported.len(), 7);
        let path = format!("{SERVICE}/pairing");
        let pairing = Proxy::new(&bluez, name, path.as_str(), "org.bluez.GattCharacteristic1")
            .await
            .unwrap();
        let not_bluez = Proxy::new(
            &imposter,
            name,
            path.as_str(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        let device = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
        let other = "/org/bluez/hci0/dev_11_22_33_44_55_66";
        let acquire = "AcquireNotify";
        let path_ctl_rx = format!("{SERVICE}/control_rx");
        let path_ctl_tx = format!("{SERVICE}/control_tx");
        let path_audio_up = format!("{SERVICE}/audio_up");
        let path_audio_down = format!("{SERVICE}/audio_down");
        let proxy_ctl_rx = Proxy::new(
            &bluez,
            name,
            path_ctl_rx.as_str(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        let proxy_ctl_tx = Proxy::new(
            &bluez,
            name,
            path_ctl_tx.as_str(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        let proxy_audio_up = Proxy::new(
            &bluez,
            name,
            path_audio_up.as_str(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        let proxy_audio_down = Proxy::new(
            &bluez,
            name,
            path_audio_down.as_str(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .unwrap();
        // iOS is permitted to subscribe before pairing starts.
        let (_ctl_fd, _): (OwnedFd, u16) = proxy_ctl_tx
            .call(acquire, &(options(device),))
            .await
            .unwrap();
        let (_audio_fd, _): (OwnedFd, u16) = proxy_audio_down
            .call(acquire, &(options(device),))
            .await
            .unwrap();
        let control_frame = voble::fragment_control(b"test command", 1, 20)
            .unwrap()
            .remove(0);
        assert!(proxy_ctl_rx
            .call::<_, _, ()>("WriteValue", &(control_frame.clone(), options(device)))
            .await
            .is_err());
        assert!(proxy_audio_up
            .call::<_, _, ()>("WriteValue", &(vec![1u8; 20], options(device)))
            .await
            .is_err());
        assert!(not_bluez
            .call::<_, _, (OwnedFd, u16)>(acquire, &(options(device),))
            .await
            .is_err());
        let (remote, mtu): (OwnedFd, u16) =
            pairing.call(acquire, &(options(device),)).await.unwrap();
        assert_eq!(mtu, ATT_MTU);
        assert!(pairing
            .call::<_, _, ()>(
                "WriteValue",
                &(vec![0u8; MAX_GATT_VALUE + 1], options(device)),
            )
            .await
            .is_err());
        assert!(pairing
            .call::<_, _, (OwnedFd, u16)>(acquire, &(options(other),))
            .await
            .is_err());
        let begin = voble::fragment(br#"{"op":"begin"}"#, 4, 20).unwrap();
        assert!(pairing
            .call::<_, _, ()>("WriteValue", &(begin[0].clone(), options(other)))
            .await
            .is_err());
        assert!(not_bluez
            .call::<_, _, ()>("WriteValue", &(begin[0].clone(), options(device)))
            .await
            .is_err());
        let remote = Socket::from(std::os::fd::OwnedFd::from(remote));
        remote.set_nonblocking(true).unwrap();
        let phone = AsyncFd::new(remote).unwrap();
        let begin = exchange(
            &pairing,
            &mut replies,
            &phone,
            device,
            10,
            json!({"op":"begin"}),
        )
        .await;
        assert_eq!(begin["op"], "begin");
        assert_eq!(begin["version"], 1);
        assert_eq!(begin["android_public"].as_str().unwrap().len(), 43);
        let mut request = json!({
            "op":"prepare", "challenge_id":begin["challenge_id"],
            "iphone_public":"WGmv9FBUlzLLqu1eXfmzCm2jHLDidCutWtShp2jxpnY",
            "iphone_nonce":base64url(&[3;32]),
        });
        let prepare = exchange(&pairing, &mut replies, &phone, device, 11, request.clone()).await;
        let code = prepare["sas"].as_str().unwrap();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        context.approve(code);
        request["op"] = json!("confirm");
        request["sas"] = json!(code);
        let confirm = exchange(&pairing, &mut replies, &phone, device, 12, request).await;
        assert_eq!(confirm["ready"], true);
        assert_eq!(confirm["confirmation_tag"].as_str().unwrap().len(), 22);
        let (control_tx, mut control_rx) = mpsc::channel(1);
        let (audio_tx, mut audio_rx) = mpsc::channel(1);
        {
            let mut state = context.state.lock().unwrap();
            let active = state.peer.as_mut().unwrap();
            active.calls = Some(control_tx);
            active.microphone = Some(audio_tx);
        }
        assert!(proxy_ctl_rx
            .call::<_, _, ()>("WriteValue", &(control_frame.clone(), options(other)))
            .await
            .is_err());
        proxy_ctl_rx
            .call::<_, _, ()>("WriteValue", &(control_frame, options(device)))
            .await
            .unwrap();
        assert_eq!(control_rx.recv().await.unwrap(), b"test command");
        assert!(proxy_audio_up
            .call::<_, _, ()>("WriteValue", &(vec![1u8; 20], options(other)))
            .await
            .is_err());
        proxy_audio_up
            .call::<_, _, ()>("WriteValue", &(vec![1u8; 20], options(device)))
            .await
            .unwrap();
        assert_eq!(audio_rx.recv().await.unwrap().bytes, vec![1u8; 20]);
        let pairing_link = context
            .state
            .lock()
            .unwrap()
            .peer
            .as_ref()
            .unwrap()
            .link(Channel::Pairing)
            .unwrap()
            .clone();
        let other_link = context.output(&pairing_link, Channel::Control).unwrap();
        assert!(context.revoke(&other_link).is_err());
        assert!(context.state.lock().unwrap().disk.credential().is_some());
        let stream = context.stream().unwrap();
        assert!(
            timeout(Duration::from_millis(10), wait_idle_end(stream.as_ref()))
                .await
                .is_err()
        );
        drop(phone);
        wait_idle_end(stream.as_ref()).await;
        context.remove(stream.as_ref().unwrap());
        assert!(context.stream().unwrap().is_none());
        assert!(context.revoke(&pairing_link).is_err());
        let (_new_fd, _): (OwnedFd, u16) =
            pairing.call(acquire, &(options(device),)).await.unwrap();
        let new_origin = context
            .state
            .lock()
            .unwrap()
            .peer
            .as_ref()
            .unwrap()
            .link(Channel::Pairing)
            .unwrap()
            .clone();
        assert!(context.revoke(&pairing_link).is_err());
        context.revoke(&new_origin).unwrap();
        assert!(context.state.lock().unwrap().disk.credential().is_none());
        let record: Json =
            serde_json::from_slice(&std::fs::read(state_path.join("state.json")).unwrap()).unwrap();
        assert_eq!(record.get("current"), Some(&Json::Null));
        assert_eq!(record.get("previous"), Some(&Json::Null));
        context.close();
        assert!(pairing
            .call::<_, _, (OwnedFd, u16)>(acquire, &(options(device),))
            .await
            .is_err());
        registration(&control, true, false).await.unwrap();
        registration(&control, false, false).await.unwrap();
        server.close().await.unwrap();
    })
    .await
    .unwrap();
}

async fn exchange(
    proxy: &Proxy<'_>,
    replies: &mut mpsc::Receiver<Reply>,
    phone: &AsyncFd<Socket>,
    device: &str,
    id: u16,
    payload: Json,
) -> Json {
    let bytes = serde_json::to_vec(&payload).unwrap();
    let frames = voble::fragment(&bytes, id, MAX_GATT_VALUE).unwrap();
    if payload["op"] == "prepare" {
        // CoreBluetooth hands BlueZ a long write when this frame exceeds ATT_MTU - 3.
        assert_eq!(frames.len(), 1);
        assert!(frames[0].len() > usize::from(ATT_MTU - 3));
    }
    for frame in frames {
        proxy
            .call::<_, _, ()>("WriteValue", &(frame, options(device)))
            .await
            .unwrap();
    }
    let reply = replies.recv().await.unwrap();
    let receive = async {
        let mut fragments = Fragments::default();
        loop {
            let mut value = [0; (ATT_MTU - 3) as usize];
            let len = phone
                .async_io(tokio::io::Interest::READABLE, |mut fd| fd.read(&mut value))
                .await
                .unwrap();
            phone.get_ref().write_all(&[1]).unwrap();
            if let Some(data) = fragments.push(&value[..len], Instant::now()).unwrap() {
                return serde_json::from_slice::<Json>(&data).unwrap();
            }
        }
    };
    let (result, response) = tokio::join!(send(&reply), receive);
    result.unwrap();
    response
}
