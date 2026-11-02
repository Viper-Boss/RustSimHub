// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Action, Context, Purpose, Session};
use wasm_bindgen::prelude::*;
fn js(error: crate::Error) -> JsValue {
    JsValue::from_str(&error.to_string())
}
fn key(bytes: &[u8]) -> std::result::Result<[u8; 32], JsValue> {
    bytes.try_into().map_err(|_| js(crate::Error::Context))
}
#[wasm_bindgen]
pub struct RecoverySessionClient {
    inner: Session,
}
#[wasm_bindgen]
impl RecoverySessionClient {
    #[wasm_bindgen(constructor)]
    pub fn new(
        device_key: &[u8],
        client_key: &[u8],
        purpose: u8,
        generation: u64,
        nonce: &[u8],
        now_ms: u64,
        valid_until_ms: u64,
    ) -> std::result::Result<Self, JsValue> {
        let purpose = match purpose {
            1 => Purpose::BleRecovery,
            2 => Purpose::NknControl,
            _ => return Err(js(crate::Error::Context)),
        };
        let context = Context::new(
            purpose,
            key(device_key)?,
            key(client_key)?,
            generation,
            key(nonce)?,
        )
        .map_err(js)?;
        Ok(Self {
            inner: Session::client(context, now_ms, valid_until_ms).map_err(js)?,
        })
    }
    pub fn write_handshake(&mut self, now_ms: u64) -> std::result::Result<Vec<u8>, JsValue> {
        self.inner.write_handshake(now_ms).map_err(js)
    }
    pub fn write_client_hello(&mut self, now_ms: u64) -> std::result::Result<Vec<u8>, JsValue> {
        self.inner.write_client_hello(now_ms).map_err(js)
    }
    pub fn read_handshake(&mut self, wire: &[u8], now_ms: u64) -> std::result::Result<(), JsValue> {
        self.inner.read_handshake(wire, now_ms).map_err(js)
    }
    pub fn open_device_binding(
        &mut self,
        wire: &[u8],
        now_ms: u64,
    ) -> std::result::Result<(), JsValue> {
        self.inner.open_binding(wire, now_ms).map_err(js)
    }
    pub fn binding_message(&mut self, now_ms: u64) -> std::result::Result<Vec<u8>, JsValue> {
        self.inner.binding_message(now_ms).map_err(js)
    }
    pub fn seal_client_binding(
        &mut self,
        signature: &[u8],
        now_ms: u64,
    ) -> std::result::Result<Vec<u8>, JsValue> {
        self.inner.seal_binding(signature, now_ms).map_err(js)
    }
    pub fn seal_request(
        &mut self,
        id: &[u8],
        action: u8,
        body: &[u8],
        now_ms: u64,
    ) -> std::result::Result<Vec<u8>, JsValue> {
        let id = match id.try_into() {
            Ok(id) => id,
            Err(_) => {
                self.inner.close();
                return Err(js(crate::Error::Record));
            }
        };
        let action = match Action::try_from(action) {
            Ok(action) => action,
            Err(error) => {
                self.inner.close();
                return Err(js(error));
            }
        };
        self.inner
            .seal_request(id, action, body, now_ms)
            .map_err(js)
    }
    pub fn open_response(
        &mut self,
        wire: &[u8],
        now_ms: u64,
    ) -> std::result::Result<Vec<u8>, JsValue> {
        self.inner
            .open_response(wire, now_ms)
            .map(|plain| plain.to_vec())
            .map_err(js)
    }
    pub fn is_bound(&self) -> bool {
        self.inner.is_bound()
    }
    pub fn open_binding_ack(
        &mut self,
        wire: &[u8],
        now_ms: u64,
    ) -> std::result::Result<(), JsValue> {
        self.inner.open_binding_ack(wire, now_ms).map_err(js)
    }
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
    pub fn close(&mut self) {
        self.inner.close();
    }
}
