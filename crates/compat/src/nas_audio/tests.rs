// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_media::{Authority, Context, Direction, IoError, SAMPLES};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Default)]
struct State {
    closed: usize,
    writes: Vec<Vec<i16>>,
    reads: usize,
}
struct Device(Rc<RefCell<State>>);
impl Pcm for Device {
    fn read(&mut self, out: &mut [i16]) -> Result<usize, IoError> {
        self.0.borrow_mut().reads += 1;
        for (i, sample) in out.iter_mut().enumerate() {
            *sample = i as i16 - 160;
        }
        Ok(out.len())
    }
    fn write(&mut self, data: &[i16]) -> Result<usize, IoError> {
        self.0.borrow_mut().writes.push(data.to_vec());
        Ok(data.len())
    }
    fn recover(&mut self, _: Direction) -> Result<(), IoError> {
        Ok(())
    }
    fn close(&mut self) {
        self.0.borrow_mut().closed += 1;
    }
}
fn permit() -> (Authority, Permit) {
    let ctx = Context {
        boot: "a".repeat(64),
        backend: "fixture".into(),
        line: 1,
        call: "b".repeat(36),
        call_revision: 1,
        lease_generation: 1,
        principal: "c".repeat(36),
        principal_generation: 1,
        peer_key: [7; 32],
        lease_until_ms: 30_000,
        active: true,
        media_scope: true,
        control_scope: true,
    };
    let mut authority = Authority::new();
    let ticket = authority.issue(ctx.clone(), 0).unwrap();
    let p = authority.consume(ticket, &ctx, &ctx.peer_key, 0).unwrap();
    (authority, p)
}
#[test]
fn exact_le_conversion_roundtrip_uses_real_pump_and_bounded_queue() {
    let (_authority, p) = permit();
    let state = Rc::new(RefCell::new(State::default()));
    let mut port =
        NasAudioPortV1::open(Device(state.clone()), p, Format::PCM16_MONO, || 0).unwrap();
    let mut bytes = [0; FRAME_BYTES];
    for c in bytes.as_chunks_mut::<2>().0.iter_mut() {
        c.copy_from_slice(&(-32768_i16).to_le_bytes());
    }
    for seq in 0..6 {
        port.push(&bytes, seq, 0).unwrap();
    }
    assert_eq!(port.stats().dropped_frames, 2);
    port.step().unwrap();
    assert_eq!(state.borrow().writes[0], vec![-32768; SAMPLES]);
    let frame = port.pull().unwrap().unwrap();
    assert_eq!(&frame.bytes[..4], &[0x60, 0xff, 0x61, 0xff]);
    assert_eq!(frame.sequence, 0);
    assert_eq!(frame.at_ms, 0);
    port.close();
    assert!(port.push(&bytes, 9, 0).is_err());
    assert!(port.pull().is_err());
    drop(port);
    assert_eq!(state.borrow().closed, 1);
}
#[test]
fn unsupported_formats_close_device_without_starting_io() {
    for i in 0..4 {
        let (_a, p) = permit();
        let state = Rc::new(RefCell::new(State::default()));
        let mut f = Format::PCM16_MONO;
        match i {
            0 => f.rate = 8000,
            1 => f.channels = 2,
            2 => f.little_endian_signed_16 = false,
            _ => f.frame_bytes = 320,
        };
        assert!(NasAudioPortV1::open(Device(state.clone()), p, f, || 0).is_err());
        assert_eq!(state.borrow().closed, 1);
        assert_eq!(state.borrow().reads, 0);
    }
}
#[test]
fn malformed_duplicate_stale_future_frames_rejected() {
    let (_a, p) = permit();
    let state = Rc::new(RefCell::new(State::default()));
    let mut port = NasAudioPortV1::open(Device(state), p, Format::PCM16_MONO, || 100).unwrap();
    assert!(port.push(&[0; 639], 1, 100).is_err());
    assert!(port.push(&[0; 640], 1, 101).is_err());
    assert!(port.push(&[0; 640], 1, 40).is_err());
    port.push(&[0; 640], 1, 100).unwrap();
    assert!(port.push(&[0; 640], 1, 100).is_err());
}
#[test]
fn actual_permit_revocation_prevents_all_io() {
    let (mut a, p) = permit();
    let state = Rc::new(RefCell::new(State::default()));
    let mut port =
        NasAudioPortV1::open(Device(state.clone()), p, Format::PCM16_MONO, || 0).unwrap();
    a.revoke();
    assert!(port.step().is_err());
    assert!(port.pull().is_err());
    assert!(port.push(&[0; 640], 0, 0).is_err());
    assert_eq!(state.borrow().reads, 0);
    assert_eq!(state.borrow().closed, 1);
}
#[test]
fn expiry_during_external_io_and_backwards_clock_close_session() {
    for reverse in [false, true] {
        let (_a, p) = permit();
        let state = Rc::new(RefCell::new(State::default()));
        let now = Rc::new(Cell::new(100));
        let clock = now.clone();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let mut port =
            NasAudioPortV1::open(Device(state.clone()), p, Format::PCM16_MONO, move || {
                let n = count.get();
                count.set(n + 1);
                if n >= 2 {
                    if reverse {
                        0
                    } else {
                        1000
                    }
                } else {
                    clock.get()
                }
            })
            .unwrap();
        assert!(port.step().is_err());
        assert_eq!(state.borrow().reads, 1);
        assert!(state.borrow().writes.is_empty());
        assert_eq!(state.borrow().closed, 1);
    }
}

#[test]
fn go_request_vectors_parse_with_actual_wire_parser() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../tools/nas-adapter/control/request-vectors.json"
    ))
    .unwrap();
    for value in vectors {
        let bytes = serde_json::to_vec(&value).unwrap();
        let parsed = rustsimhub_wire::parse_auth_request(&bytes).unwrap();
        assert_eq!(parsed.protocol, 2);
    }
}
