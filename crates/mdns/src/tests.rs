// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

const NAME: &str = "rustsimhub-0087";
const AP: Ipv4Addr = Ipv4Addr::new(192, 168, 100, 1);

fn query(names: &[(&str, u16, u16)], flags: u16) -> Vec<u8> {
    let mut p = vec![0x12, 0x34];
    p.extend_from_slice(&flags.to_be_bytes());
    p.extend_from_slice(&(names.len() as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    for (name, qtype, qclass) in names {
        for label in name.split('.') {
            p.push(label.len() as u8);
            p.extend_from_slice(label.as_bytes());
        }
        p.push(0);
        p.extend_from_slice(&qtype.to_be_bytes());
        p.extend_from_slice(&qclass.to_be_bytes());
    }
    p
}

#[test]
fn answers_only_our_a_question_with_the_given_address() {
    let r = answer(&query(&[("rustsimhub-0087.local", 1, 1)], 0), NAME, AP).unwrap();
    assert!(!r.unicast);
    // Header: response, authoritative, one answer, no questions echoed.
    assert_eq!(&r.packet[..12], &[0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
    assert!(r
        .packet
        .ends_with(&[0, 1, 0x80, 1, 0, 0, 0, 120, 0, 4, 192, 168, 100, 1]));
    // Case-insensitive, ANY also answered.
    assert!(answer(&query(&[("RustSimHub-0087.LOCAL", 255, 1)], 0), NAME, AP).is_some());
}

#[test]
fn unicast_reply_only_when_every_matching_question_asks_for_it() {
    let qu = answer(&query(&[("rustsimhub-0087.local", 1, 0x8001)], 0), NAME, AP).unwrap();
    assert!(qu.unicast);
    let mixed = answer(
        &query(
            &[
                ("rustsimhub-0087.local", 1, 0x8001),
                ("rustsimhub-0087.local", 255, 1),
            ],
            0,
        ),
        NAME,
        AP,
    )
    .unwrap();
    assert!(!mixed.unicast);
}

#[test]
fn ignores_other_names_types_responses_and_bad_addresses() {
    for q in [
        query(&[("other.local", 1, 1)], 0),
        query(&[("rustsimhub-0088.local", 1, 1)], 0),
        query(&[("rustsimhub-0087.local.evil", 1, 1)], 0),
        query(&[("x.rustsimhub-0087.local", 1, 1)], 0),
        query(&[("rustsimhub-0087.local", 28, 1)], 0),
        query(&[("rustsimhub-0087.local", 1, 3)], 0),
        query(&[("rustsimhub-0087.local", 1, 1)], 0x8400), // a response, not a query
        query(&[("rustsimhub-0087.local", 1, 1)], 0x2800), // non-zero opcode
    ] {
        assert!(answer(&q, NAME, AP).is_none());
    }
    let q = query(&[("rustsimhub-0087.local", 1, 1)], 0);
    assert!(answer(&q, NAME, Ipv4Addr::UNSPECIFIED).is_none());
    assert!(answer(&q, NAME, Ipv4Addr::LOCALHOST).is_none());
    assert!(answer(&q, "evil", AP).is_none());
}

#[test]
fn malformed_truncated_oversized_and_looping_packets_are_ignored() {
    let good = query(&[("rustsimhub-0087.local", 1, 1)], 0);
    for cut in 0..good.len() {
        assert!(answer(&good[..cut], NAME, AP).is_none(), "cut {cut}");
    }
    assert!(answer(&vec![0; MAX_PACKET + 1], NAME, AP).is_none());
    let mut zero = good.clone();
    zero[5] = 0;
    assert!(answer(&zero, NAME, AP).is_none());
    let mut many = good.clone();
    many[4] = 0xff;
    assert!(answer(&many, NAME, AP).is_none());
    // A self-referencing or forward compression pointer never loops.
    let mut looped = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    looped.extend_from_slice(&[0xC0, 12, 0, 1, 0, 1]);
    assert!(answer(&looped, NAME, AP).is_none());
    let mut forward = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    forward.extend_from_slice(&[0xC0, 20, 0, 1, 0, 1]);
    assert!(answer(&forward, NAME, AP).is_none());
    // Reserved label type 0x40/0x80.
    let mut reserved = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    reserved.extend_from_slice(&[0x40, 0, 0, 1, 0, 1]);
    assert!(answer(&reserved, NAME, AP).is_none());
}

#[test]
fn a_backward_compression_pointer_to_our_name_is_accepted() {
    // Question 1: our name in full; question 2: pointer back to offset 12.
    let mut p = query(&[("rustsimhub-0087.local", 28, 1)], 0);
    p[5] = 2;
    p.extend_from_slice(&[0xC0, 12, 0, 1, 0, 1]);
    assert!(answer(&p, NAME, AP).is_some());
}

#[test]
fn device_name_and_address_rules() {
    assert!(valid_device_name("rustsimhub-0087"));
    assert!(valid_device_name("rustsimhub-ab12"));
    for bad in [
        "rustsimhub-087",
        "rustsimhub-00870",
        "rustsimhub-AB12",
        "RustSimHub-0087",
        "rustsimhub-00.7",
        "",
    ] {
        assert!(!valid_device_name(bad), "{bad}");
    }
    assert!(announceable(AP));
    assert!(announceable(Ipv4Addr::new(10, 55, 0, 2)));
    assert!(announceable(Ipv4Addr::new(169, 254, 1, 1)));
    assert!(!announceable(Ipv4Addr::new(8, 8, 8, 8)));
}
