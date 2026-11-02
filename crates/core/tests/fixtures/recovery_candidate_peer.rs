// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic radio peer uses A's actual native client, actual D characteristic
//! callbacks and per-peer AcquireNotify descriptors. Never a crypto echo port.
use super::*;
use rustsimhub_ble::manifest::Characteristic;
use rustsimhub_recovery_session::{Action, Context, Purpose, Session};
use std::{collections::HashMap, time::Instant};
use zbus::{
    zvariant::{OwnedFd, OwnedObjectPath, OwnedValue, Value},
    Connection, Proxy,
};
pub fn uuid(n: u8) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}
fn options(write: bool) -> HashMap<String, OwnedValue> {
    let mut result = HashMap::from([(
        "device".into(),
        Value::from(OwnedObjectPath::try_from(backend::DEVICE).unwrap())
            .try_into()
            .unwrap(),
    )]);
    if write {
        result.insert("type".into(), Value::from("request").try_into().unwrap());
    } else {
        result.insert("mtu".into(), OwnedValue::from(23u16));
        result.insert("link".into(), Value::from("LE").try_into().unwrap());
    }
    result
}
struct Gatt<'a> {
    bus: &'a Connection,
    name: String,
    auth: tokio::net::UnixDatagram,
    result: tokio::net::UnixDatagram,
    next_input: u16,
    next_output: u16,
}
impl<'a> Gatt<'a> {
    async fn new(bus: &'a Connection) -> Self {
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(root().join("backend/service.json")).unwrap())
                .unwrap();
        let name = value["name"].as_str().unwrap().to_owned();
        let mut sockets = Vec::new();
        for kind in [Characteristic::AuthExchange, Characteristic::Result] {
            let p = Proxy::new(
                bus,
                name.as_str(),
                kind.path(),
                "org.bluez.GattCharacteristic1",
            )
            .await
            .unwrap();
            let (fd, mtu): (OwnedFd, u16) =
                p.call("AcquireNotify", &(options(false),)).await.unwrap();
            assert_eq!(mtu, 23);
            let socket = std::os::unix::net::UnixDatagram::from(std::os::fd::OwnedFd::from(fd));
            socket.set_nonblocking(true).unwrap();
            sockets.push(tokio::net::UnixDatagram::from_std(socket).unwrap());
        }
        let result = sockets.pop().unwrap();
        let auth = sockets.pop().unwrap();
        Self {
            bus,
            name,
            auth,
            result,
            next_input: 1,
            next_output: 1,
        }
    }
    async fn exchange(&mut self, auth: bool, bytes: &[u8]) -> Result<Vec<u8>, ()> {
        let input = self.next_input;
        self.next_input += 1;
        let kind = if auth {
            Characteristic::AuthExchange
        } else {
            Characteristic::Command
        };
        let proxy = Proxy::new(
            self.bus,
            self.name.as_str(),
            kind.path(),
            "org.bluez.GattCharacteristic1",
        )
        .await
        .map_err(|_| ())?;
        let count = bytes.len().div_ceil(8);
        assert!((1..=256).contains(&count));
        for (index, chunk) in bytes.chunks(8).enumerate() {
            let mut frame = vec![b'R', b'B', 1, if auth { 1 } else { 2 }];
            for n in [input, index as u16, count as u16, bytes.len() as u16] {
                frame.extend_from_slice(&n.to_le_bytes());
            }
            frame.extend_from_slice(chunk);
            let reply = tokio::time::timeout(
                Duration::from_secs(5),
                proxy.call::<_, _, ()>("WriteValue", &(frame, options(true))),
            )
            .await
            .map_err(|_| ())?;
            reply.map_err(|error| { eprintln!("candidate_write_failed auth={auth} input={input} fragment={index} error={error}"); })?;
        }
        let socket = if auth { &self.auth } else { &self.result };
        let mut value = Vec::new();
        let mut index = 0;
        let mut dimensions = None;
        loop {
            let mut frame = [0u8; 21];
            let len = tokio::time::timeout(Duration::from_secs(5), socket.recv(&mut frame))
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
            if len == 0 {
                eprintln!("candidate_notify_eof auth={auth} input={input}");
                return Err(());
            }
            assert!((13..=20).contains(&len));
            assert_eq!(&frame[..4], &[b'R', b'B', 1, if auth { 1 } else { 3 }]);
            let numbers = (0..4)
                .map(|i| u16::from_le_bytes([frame[4 + i * 2], frame[5 + i * 2]]))
                .collect::<Vec<_>>();
            assert_eq!(numbers[0], self.next_output);
            assert_eq!(numbers[1], index);
            let pair = (numbers[2], numbers[3]);
            assert!((1..=256).contains(&pair.0));
            assert!(pair.1 <= 2048);
            if let Some(old) = dimensions {
                assert_eq!(pair, old);
            } else {
                dimensions = Some(pair);
            }
            value.extend_from_slice(&frame[12..len]);
            index += 1;
            if index == numbers[2] {
                assert_eq!(value.len(), numbers[3] as usize);
                self.next_output += 1;
                return Ok(value);
            }
        }
    }
}
struct Client {
    session: Session,
    clock: Instant,
    next_request: u8,
}
impl Client {
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().try_into().unwrap()
    }
    fn seal(&mut self, action: Action, body: &serde_json::Value) -> Vec<u8> {
        let id = uuid::Uuid::parse_str(&uuid(self.next_request)).unwrap();
        self.next_request += 1;
        self.session
            .seal_request(
                *id.as_bytes(),
                action,
                &serde_json::to_vec(body).unwrap(),
                self.now(),
            )
            .unwrap()
    }
    async fn send(
        &mut self,
        gatt: &mut Gatt<'_>,
        action: Action,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ()> {
        let request = self.seal(action, body);
        self.send_wire(gatt, &request).await
    }
    async fn send_wire(
        &mut self,
        gatt: &mut Gatt<'_>,
        wire: &[u8],
    ) -> Result<serde_json::Value, ()> {
        let reply = gatt.exchange(false, wire).await?;
        let plaintext = self
            .session
            .open_response(&reply, self.now())
            .map_err(|_| ())?;
        let result: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
        assert!(!plaintext
            .windows(b"SYNTHETIC_NATIVE_PASSWORD".len())
            .any(|w| w == b"SYNTHETIC_NATIVE_PASSWORD"));
        Ok(result)
    }
}
pub struct PeerHold<'a> {
    gatt: Gatt<'a>,
}
impl PeerHold<'_> {
    pub async fn closed(self) {
        for socket in [&self.gatt.auth, &self.gatt.result] {
            let mut byte = [0];
            let result = tokio::time::timeout(Duration::from_secs(2), socket.recv(&mut byte))
                .await
                .unwrap();
            match result {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::NotConnected
                            | std::io::ErrorKind::BrokenPipe
                    ) => {}
                other => panic!("notification descriptor must close without late bytes: {other:?}"),
            }
        }
    }
}
pub async fn run(bus: &Connection) -> PeerHold<'_> {
    wait("core/public.json").await;
    let public: serde_json::Value =
        serde_json::from_slice(&fs::read(root().join("core/public.json")).unwrap()).unwrap();
    let mut pin: [u8; 32] = serde_json::from_value(public["pin"].clone()).unwrap();
    if case() == "wrong_pin" {
        pin = SigningKey::from_bytes(&[63; 32]).verifying_key().to_bytes();
    }
    let key = SigningKey::from_bytes(&[62; 32]);
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).unwrap();
    let context = Context::new(
        Purpose::BleRecovery,
        pin,
        key.verifying_key().to_bytes(),
        public["generation"].as_u64().unwrap(),
        nonce,
    )
    .unwrap();
    let mut client = Client {
        session: Session::client(context, 0, 300_000).unwrap(),
        clock: Instant::now(),
        next_request: 10,
    };
    let mut gatt = Gatt::new(bus).await;
    let hello = client.session.write_client_hello(client.now()).unwrap();
    let noise = gatt.exchange(true, &hello).await.unwrap();
    if case() == "wrong_pin" {
        assert!(client.session.read_handshake(&noise, client.now()).is_err());
        return PeerHold { gatt };
    }
    client.session.read_handshake(&noise, client.now()).unwrap();
    let third = client.session.write_handshake(client.now()).unwrap();
    let binding = gatt.exchange(true, &third).await.unwrap();
    client.session.open_binding(&binding, client.now()).unwrap();
    let signature = key
        .sign(&client.session.binding_message(client.now()).unwrap())
        .to_bytes();
    let binding = client
        .session
        .seal_binding(&signature, client.now())
        .unwrap();
    let ack = gatt.exchange(true, &binding).await.unwrap();
    client.session.open_binding_ack(&ack, client.now()).unwrap();
    let body =
        serde_json::json!({"ssid":"SYNTHETIC_NATIVE","passphrase":"SYNTHETIC_NATIVE_PASSWORD"});
    let mut begin = client.seal(Action::BeginWifi, &body);
    if case() == "tamper" {
        begin[0] ^= 1;
        assert!(client.send_wire(&mut gatt, &begin).await.is_err());
        return PeerHold { gatt };
    }
    let reply = client.send_wire(&mut gatt, &begin).await.unwrap();
    assert_eq!(reply["state"], "starting");
    assert_eq!(reply["settings_saved"], false);
    let trial = reply["trial_id"].as_str().unwrap().to_owned();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let reply = client
                .send(
                    &mut gatt,
                    Action::QueryOperation,
                    &serde_json::json!({"target_request_id":uuid(10)}),
                )
                .await
                .unwrap();
            assert_eq!(reply["lookup"], "found");
            if reply["operation"]["state"] == "ready" {
                break;
            }
            assert_eq!(reply["operation"]["state"], "starting");
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    if case() == "disconnect" {
        return PeerHold { gatt };
    }
    if case() == "replay" {
        assert!(client.send_wire(&mut gatt, &begin).await.is_err());
        return PeerHold { gatt };
    }
    if case() == "revoke_before_confirm" {
        write("control/revoke", &serde_json::json!(true));
        wait("core/revoked").await;
    }
    let action = if case() == "cancel" {
        Action::CancelWifi
    } else {
        Action::ConfirmWifi
    };
    let reply = client
        .send(&mut gatt, action, &serde_json::json!({"trial_id":trial}))
        .await;
    if case().starts_with("revoke_") {
        assert!(reply.is_err());
    } else {
        let reply = reply.unwrap();
        assert_eq!(
            reply["state"],
            if case() == "cancel" {
                "rolled_back"
            } else {
                "saved"
            }
        );
        assert_eq!(reply["authoritative"], true);
        assert_eq!(reply["settings_saved"], case() == "confirm");
    }
    // Retain this descriptor until Core has completed the post-write check.
    tokio::time::sleep(Duration::from_millis(100)).await;
    PeerHold { gatt }
}
