// SPDX-License-Identifier: AGPL-3.0-only
use super::{framing::*, manifest::*, ports::*, Error};

fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn frame(kind: u8, id: u16, index: u16, count: u16, total: u16, data: &[u8]) -> Vec<u8> {
    let mut result = vec![b'R', b'B', 1, kind];
    for value in [id, index, count, total] {
        result.extend_from_slice(&value.to_le_bytes());
    }
    result.extend_from_slice(data);
    result
}

fn receive(
    assembler: &mut Assembler,
    budget: Budget,
    now: u64,
    bytes: &[u8],
) -> Result<Option<Message>, Error> {
    assembler.receive(
        Characteristic::AuthExchange,
        WriteOptions::REQUEST,
        budget,
        now,
        bytes,
    )
}

#[test]
fn frozen_single_and_two_fragment_vectors_are_consumed_in_one_connection() {
    let mut assembler = Assembler::new();
    let msg = receive(
        &mut assembler,
        Budget::INITIAL,
        10,
        &hex("524201010100000001000300616263"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(msg.bytes.as_slice(), b"abc");
    assert_eq!((msg.id, msg.kind), (1, InputKind::Auth));
    assert_eq!(
        receive(
            &mut assembler,
            Budget::INITIAL,
            20,
            &hex("5242010102000000020009006162636465666768"),
        ),
        Ok(None)
    );
    let msg = receive(
        &mut assembler,
        Budget::INITIAL,
        21,
        &hex("52420101020001000200090069"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(msg.bytes.as_slice(), b"abcdefghi");
    assert_eq!(msg.id, 2);
}

#[test]
fn authoritative_json_contract_and_vectors_match_the_actual_implementation() {
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/team/interfaces/t5-offline-v1.json"
    ))
    .unwrap();
    let gatt = &contract["gatt"];
    assert_eq!(gatt["service_uuid"], SERVICE_UUID);
    assert_eq!(gatt["object_root"], ROOT);
    for (name, ch) in [
        ("device_info", Characteristic::DeviceInfo),
        ("auth_exchange", Characteristic::AuthExchange),
        ("command", Characteristic::Command),
        ("result", Characteristic::Result),
    ] {
        assert_eq!(gatt["characteristics"][name]["uuid"], ch.uuid());
        assert_eq!(
            gatt["characteristics"][name]["flags"],
            serde_json::json!(ch.flags())
        );
    }
    for (name, value) in [
        ("message_bytes", MESSAGE_BYTES),
        ("fragments", MAX_FRAGMENTS),
        ("frame_header_bytes", HEADER_BYTES),
        ("initial_attribute_bytes", 20),
        ("attribute_bytes_cap", 244),
        ("assembly_ms", ASSEMBLY_MS as usize),
        ("queued_results", QUEUED_RESULTS),
        ("dbus_call_ms", DBUS_CALL_MS as usize),
    ] {
        assert_eq!(gatt["limits"][name], value, "{name}");
    }
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/team/interfaces/t5-vectors-v1.json"
    ))
    .unwrap();
    let mut assembler = Assembler::new();
    for row in vectors["gatt_frames"].as_array().unwrap() {
        assert_eq!(row["attribute_bytes"], Budget::INITIAL.attribute_bytes());
        let actual = receive(
            &mut assembler,
            Budget::INITIAL,
            0,
            &hex(row["hex"].as_str().unwrap()),
        )
        .unwrap();
        if row["complete_hex"].is_null() {
            assert!(actual.is_none());
        } else {
            assert_eq!(
                actual.unwrap().bytes.as_slice(),
                hex(row["complete_hex"].as_str().unwrap()).as_slice()
            );
        }
    }
}

#[test]
fn every_payload_length_round_trips_at_initial_and_negotiated_budgets() {
    for mtu in [23, 24, 64, 247, 517, u16::MAX] {
        let budget = Budget::negotiated_att_mtu(mtu).unwrap();
        for len in 1..=MESSAGE_BYTES {
            let expected: Vec<_> = (0..len).map(|i| (i % 251) as u8).collect();
            let mut outbox = Outbox::new();
            let mut assembler = Assembler::new();
            outbox
                .enqueue(
                    OutputKind::Auth,
                    OpaqueBytes::new(expected.clone()).unwrap(),
                    budget,
                )
                .unwrap();
            let mut completed = None;
            let mut count = 0;
            while let Some(out) = outbox.front() {
                assert!(out.bytes.len() <= budget.attribute_bytes());
                assert_eq!(out.characteristic, Characteristic::AuthExchange);
                assert_eq!(outbox.front().unwrap().bytes, out.bytes);
                let result = receive(&mut assembler, budget, count, &out.bytes).unwrap();
                if result.is_some() {
                    assert!(completed.is_none());
                    completed = result;
                }
                outbox.acknowledge(out.ticket).unwrap();
                count += 1;
            }
            assert_eq!(count as usize, len.div_ceil(budget.payload_bytes()));
            assert!(count <= MAX_FRAGMENTS as u64);
            assert_eq!(completed.unwrap().bytes.as_slice(), expected.as_slice());
        }
    }
}

#[test]
fn count_total_and_index_are_verified_before_allocating_or_appending() {
    for invalid in [
        frame(1, 0, 0, 1, 1, b"a"),
        frame(1, 2, 0, 1, 1, b"a"),
        frame(1, 1, 1, 2, 9, b"a"),
        frame(1, 1, 0, 0, 1, b"a"),
        frame(1, 1, 0, 1, 0, b"a"),
        frame(1, 1, 0, 257, 2048, b"abcdefgh"),
        frame(1, 1, 0, 256, 2049, b"abcdefgh"),
        frame(1, 1, 0, 1, 9, b"abcdefgh"),
        frame(1, 1, 0, 2, 9, b"abcdefg"),
        frame(1, 1, 0, 1, 8, b"abcdefg"),
        frame(1, 1, 0, 1, 1, b"ab"),
    ] {
        let mut assembler = Assembler::new();
        assert_eq!(
            receive(&mut assembler, Budget::INITIAL, 0, &invalid),
            Err(Error::Protocol)
        );
        assert!(assembler.is_closed());
        assert_eq!(
            receive(
                &mut assembler,
                Budget::INITIAL,
                1,
                &frame(1, 1, 0, 1, 1, b"a")
            ),
            Err(Error::StaleContext)
        );
    }
}

#[test]
fn unknown_magic_version_kind_empty_and_oversized_frames_are_rejected() {
    let valid = frame(1, 1, 0, 1, 1, b"a");
    let mut cases = vec![vec![], valid[..12].to_vec(), vec![0; 245]];
    for (index, byte) in [(0, 0), (1, b'C'), (2, 2), (3, 0), (3, 3), (3, 255)] {
        let mut bad = valid.clone();
        bad[index] = byte;
        cases.push(bad);
    }
    for bad in cases {
        let mut assembler = Assembler::new();
        assert_eq!(
            receive(&mut assembler, Budget::INITIAL, 0, &bad),
            Err(Error::Protocol)
        );
    }
}

#[test]
fn write_modes_offsets_prepare_authorize_and_wrong_characteristic_are_rejected() {
    let valid = frame(1, 1, 0, 1, 1, b"a");
    for options in [
        WriteOptions {
            kind: WriteType::Command,
            ..WriteOptions::REQUEST
        },
        WriteOptions {
            kind: WriteType::Reliable,
            ..WriteOptions::REQUEST
        },
        WriteOptions {
            offset: 1,
            ..WriteOptions::REQUEST
        },
        WriteOptions {
            offset: u16::MAX,
            ..WriteOptions::REQUEST
        },
        WriteOptions {
            prepare_authorize: true,
            ..WriteOptions::REQUEST
        },
    ] {
        let mut assembler = Assembler::new();
        assert_eq!(
            assembler.receive(
                Characteristic::AuthExchange,
                options,
                Budget::INITIAL,
                0,
                &valid
            ),
            Err(Error::Protocol)
        );
    }
    for characteristic in [
        Characteristic::DeviceInfo,
        Characteristic::Command,
        Characteristic::Result,
    ] {
        let mut assembler = Assembler::new();
        assert_eq!(
            assembler.receive(
                characteristic,
                WriteOptions::REQUEST,
                Budget::INITIAL,
                0,
                &valid
            ),
            Err(Error::Protocol)
        );
    }
    let mut assembler = Assembler::new();
    let command = frame(2, 1, 0, 1, 1, b"a");
    assert_eq!(
        assembler
            .receive(
                Characteristic::Command,
                WriteOptions::REQUEST,
                Budget::INITIAL,
                0,
                &command
            )
            .unwrap()
            .unwrap()
            .kind,
        InputKind::Command
    );
}

#[test]
fn duplicate_skip_changed_kind_count_length_and_parallel_message_close_assembly() {
    let first = frame(1, 1, 0, 3, 17, b"abcdefgh");
    for invalid in [
        first.clone(),
        frame(1, 1, 2, 3, 17, b"q"),
        frame(2, 1, 1, 3, 17, b"ijklmnop"),
        frame(1, 1, 1, 2, 16, b"ijklmnop"),
        frame(1, 1, 1, 3, 18, b"ijklmnop"),
        frame(1, 2, 0, 1, 1, b"a"),
    ] {
        let mut assembler = Assembler::new();
        assert_eq!(
            receive(&mut assembler, Budget::INITIAL, 0, &first),
            Ok(None)
        );
        let characteristic = if invalid[3] == 2 {
            Characteristic::Command
        } else {
            Characteristic::AuthExchange
        };
        assert_eq!(
            assembler.receive(
                characteristic,
                WriteOptions::REQUEST,
                Budget::INITIAL,
                1,
                &invalid
            ),
            Err(Error::Protocol)
        );
        assert!(assembler.is_closed());
    }
}

#[test]
fn deadline_is_hard_not_extended_by_fragments_and_expires_without_new_input() {
    for final_time in [15099, 15100] {
        let mut assembler = Assembler::new();
        assert_eq!(
            receive(
                &mut assembler,
                Budget::INITIAL,
                100,
                &frame(1, 1, 0, 3, 17, b"abcdefgh")
            ),
            Ok(None)
        );
        assert_eq!(
            receive(
                &mut assembler,
                Budget::INITIAL,
                10000,
                &frame(1, 1, 1, 3, 17, b"ijklmnop")
            ),
            Ok(None)
        );
        let final_result = receive(
            &mut assembler,
            Budget::INITIAL,
            final_time,
            &frame(1, 1, 2, 3, 17, b"q"),
        );
        if final_time == 15099 {
            assert!(final_result.unwrap().is_some());
        } else {
            assert_eq!(final_result, Err(Error::Deadline));
        }
    }
    let mut assembler = Assembler::new();
    assert_eq!(
        receive(
            &mut assembler,
            Budget::INITIAL,
            1,
            &frame(1, 1, 0, 2, 9, b"abcdefgh")
        ),
        Ok(None)
    );
    assert_eq!(assembler.expire(15000), Ok(()));
    assert_eq!(assembler.expire(15001), Err(Error::Deadline));
    assert!(assembler.is_closed());
}

#[test]
fn monotonic_clock_regression_and_deadline_overflow_close_the_connection() {
    let valid = frame(1, 1, 0, 1, 1, b"a");
    let mut assembler = Assembler::new();
    assembler.expire(2).unwrap();
    assert_eq!(
        receive(&mut assembler, Budget::INITIAL, 1, &valid),
        Err(Error::Protocol)
    );
    let mut assembler = Assembler::new();
    assert_eq!(
        receive(&mut assembler, Budget::INITIAL, u64::MAX, &valid),
        Err(Error::Deadline)
    );
}

#[test]
fn negotiated_budget_is_frozen_until_current_message_finishes() {
    let large = Budget::negotiated_att_mtu(247).unwrap();
    let mut assembler = Assembler::new();
    assert_eq!(
        receive(
            &mut assembler,
            Budget::INITIAL,
            0,
            &frame(1, 1, 0, 2, 9, b"abcdefgh")
        ),
        Ok(None)
    );
    assert_eq!(
        receive(&mut assembler, large, 1, &frame(1, 1, 1, 2, 9, b"i"))
            .unwrap()
            .unwrap()
            .bytes
            .as_slice(),
        b"abcdefghi"
    );
    let data = vec![b'z'; 232];
    assert!(
        receive(&mut assembler, large, 2, &frame(1, 2, 0, 1, 232, &data))
            .unwrap()
            .is_some()
    );
    let mut assembler = Assembler::new();
    let data = vec![b'z'; 232];
    assert_eq!(
        receive(&mut assembler, large, 0, &frame(1, 1, 0, 2, 233, &data)),
        Ok(None)
    );
    // Current message retains its 232-byte payload size despite new budget=20.
    assert!(receive(
        &mut assembler,
        Budget::INITIAL,
        1,
        &frame(1, 1, 1, 2, 233, b"x")
    )
    .unwrap()
    .is_some());
}

#[test]
fn full_id_space_never_wraps_or_reuses_a_message_identifier() {
    let mut assembler = Assembler::new();
    let mut outbox = Outbox::new();
    for id in 1..=u16::MAX {
        assert_eq!(
            outbox.enqueue(
                OutputKind::Auth,
                OpaqueBytes::new(vec![42]).unwrap(),
                Budget::INITIAL
            ),
            Ok(id)
        );
        let outgoing = outbox.front().unwrap();
        let incoming = receive(
            &mut assembler,
            Budget::INITIAL,
            u64::from(id),
            &outgoing.bytes,
        )
        .unwrap()
        .unwrap();
        assert_eq!(incoming.id, id);
        outbox.acknowledge(outgoing.ticket).unwrap();
    }
    assert_eq!(
        outbox.enqueue(
            OutputKind::Auth,
            OpaqueBytes::new(vec![42]).unwrap(),
            Budget::INITIAL
        ),
        Err(Error::Exhausted)
    );
    assert_eq!(
        receive(
            &mut assembler,
            Budget::INITIAL,
            65536,
            &frame(1, 1, 0, 1, 1, b"a")
        ),
        Err(Error::Exhausted)
    );
}

#[test]
fn logical_result_queue_is_bounded_and_requires_exact_confirmation() {
    let mut queue = Outbox::new();
    for id in 1..=4 {
        assert_eq!(
            queue.enqueue(
                OutputKind::Result,
                OpaqueBytes::new(vec![id as u8; 9]).unwrap(),
                Budget::INITIAL
            ),
            Ok(id)
        );
    }
    assert_eq!(queue.len(), QUEUED_RESULTS);
    assert_eq!(
        queue.enqueue(
            OutputKind::Result,
            OpaqueBytes::new(vec![0]).unwrap(),
            Budget::INITIAL
        ),
        Err(Error::Busy)
    );
    let first = queue.front().unwrap();
    assert_eq!(first.characteristic, Characteristic::Result);
    assert_eq!(first.bytes[3], 3);
    assert_eq!(queue.front().unwrap().bytes, first.bytes);
    queue.acknowledge(first.ticket).unwrap();
    assert_eq!(queue.acknowledge(first.ticket), Err(Error::StaleContext));
    assert_eq!(queue.len(), 4); // First logical message has a final fragment left.
    let last = queue.front().unwrap();
    queue.acknowledge(last.ticket).unwrap();
    assert_eq!(queue.len(), 3);
    assert_eq!(
        queue.enqueue(
            OutputKind::Result,
            OpaqueBytes::new(vec![0]).unwrap(),
            Budget::INITIAL
        ),
        Ok(5)
    );
    queue.close();
    assert!(queue.is_empty());
    assert!(queue.front().is_none());
    assert_eq!(queue.acknowledge(last.ticket), Err(Error::StaleContext));
    assert_eq!(
        queue.enqueue(
            OutputKind::Result,
            OpaqueBytes::new(vec![0]).unwrap(),
            Budget::INITIAL
        ),
        Err(Error::StaleContext)
    );
}

#[test]
fn bounded_bytes_peer_and_epoch_debug_never_include_identifiers_or_payload() {
    assert_eq!(OpaqueBytes::new(vec![]), Err(Error::InvalidInput));
    assert_eq!(
        OpaqueBytes::new(vec![0; MESSAGE_BYTES + 1]),
        Err(Error::InvalidInput)
    );
    let data = OpaqueBytes::new(b"do-not-log-synthetic-secret".to_vec()).unwrap();
    assert!(!format!("{data:?}").contains("synthetic"));
    let peer =
        Peer::from_backend_callback(":1.42", "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF").unwrap();
    assert_eq!(peer.owner(), ":1.42");
    assert!(peer.device_path().starts_with("/org/bluez/"));
    assert!(!format!("{peer:?}").contains("AA_BB"));
    let epoch = Epoch::new([7; 16], 1).unwrap();
    assert_eq!(format!("{epoch:?}"), "Epoch(..)");
    assert_eq!(Epoch::new([0; 16], 1), Err(Error::InvalidInput));
    assert_eq!(Epoch::new([1; 16], 0), Err(Error::InvalidInput));
    for owner in ["org.bluez", ":1", ":1.", ":.1", ":1.2\n", ":1.a-b"] {
        assert!(Peer::from_backend_callback(owner, "/org/bluez/hci0/dev_X").is_err());
    }
    for path in [
        "/",
        "/elsewhere",
        "/org/bluez/",
        "/org/bluez/../x",
        "/org/bluez//x",
    ] {
        assert!(Peer::from_backend_callback(":1.2", path).is_err());
    }
}

#[test]
fn fixed_manifest_and_budget_match_the_frozen_contract() {
    assert_eq!(Budget::INITIAL.attribute_bytes(), 20);
    assert_eq!(Budget::INITIAL.payload_bytes(), 8);
    assert_eq!(Budget::negotiated_att_mtu(23).unwrap(), Budget::INITIAL);
    assert_eq!(
        Budget::negotiated_att_mtu(247).unwrap().attribute_bytes(),
        244
    );
    assert_eq!(
        Budget::negotiated_att_mtu(u16::MAX)
            .unwrap()
            .attribute_bytes(),
        244
    );
    for mtu in 0..23 {
        assert_eq!(Budget::negotiated_att_mtu(mtu), Err(Error::InvalidInput));
    }
    assert_eq!(Characteristic::ALL.len(), 4);
    assert!(DEVICE_INFO.len() <= 256);
    assert!(ROOT.starts_with("/org/rustsimhub/"));
    assert!(ADVERTISEMENT_PATH.starts_with(ROOT));
    assert!(SERVICE_PATH.starts_with(ROOT));
    for characteristic in Characteristic::ALL {
        assert!(characteristic.path().starts_with(SERVICE_PATH));
        assert_eq!(characteristic.uuid().len(), 36);
        assert!(!characteristic.flags().contains(&"write-without-response"));
    }
    assert_eq!(DBUS_CALL_MS, 5000);
}
