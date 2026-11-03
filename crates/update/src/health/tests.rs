// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn fixture() -> (ExpectedCandidate, ExpectedFence, UntrustedObservation) {
    let payloads: Vec<_> = crate::FILES
        .iter()
        .map(|n| {
            if *n == "Opus-COPYING.txt" {
                return b"synthetic license".to_vec();
            }
            let mut b = vec![0; 64];
            b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
            b[16] = 2;
            b[18] = 183;
            b[52] = 64;
            b
        })
        .collect();
    let manifest = Manifest {
        format: 1,
        kind: "application".into(),
        board: crate::BOARD.into(),
        arch: "aarch64-linux".into(),
        epoch: 12,
        schema_min: 1,
        schema_max: 3,
        source_sha256: "ab".repeat(32),
        development_fixtures: false,
        files: crate::FILES
            .iter()
            .zip(&payloads)
            .map(|(n, b)| crate::Entry {
                name: n.to_string(),
                bytes: b.len() as u64,
                sha256: hash_hex(b),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    let signature = key.sign(&crate::signing_bytes(&bytes).unwrap()).to_bytes();
    let candidate = ExpectedCandidate::verify(
        &bytes,
        &signature,
        &key.verifying_key().to_bytes(),
        2,
        11,
        &payloads,
    )
    .unwrap();
    assert!(ExpectedCandidate::verify(
        &bytes,
        &[0; 64],
        &key.verifying_key().to_bytes(),
        2,
        11,
        &payloads
    )
    .is_err());
    let mut corrupt = payloads.clone();
    corrupt[0][0] ^= 1;
    assert!(ExpectedCandidate::verify(
        &bytes,
        &signature,
        &key.verifying_key().to_bytes(),
        2,
        11,
        &corrupt
    )
    .is_err());
    assert!(ExpectedCandidate::verify(
        &bytes,
        &signature,
        &key.verifying_key().to_bytes(),
        2,
        12,
        &payloads
    )
    .is_err());
    let fence = ExpectedFence {
        boot: "01".repeat(32),
        root_sha256: "02".repeat(32),
        data_generation: 7,
        challenge: "03".repeat(32),
    };
    let observation = UntrustedObservation {
        manifest_sha256: candidate.manifest_hash.clone(),
        source_sha256: manifest.source_sha256,
        epoch: 12,
        boot: fence.boot.clone(),
        challenge: fence.challenge.clone(),
        at_ms: 100,
        until_ms: 1000,
        root_sha256: fence.root_sha256.clone(),
        data_generation: 7,
        data_schema: 2,
        data_compatible: true,
        processes: [
            ("rustsimhub-core", 986),
            ("rustsimhub-edge", 985),
            ("rustsimhub-audio-alsa", 984),
        ]
        .iter()
        .enumerate()
        .map(|(i, (name, uid))| ProcessObservation {
            role: name.to_string(),
            pid: 100 + i as u32,
            start_ticks: 1,
            uid: *uid,
            exe_sha256: hash_hex(&payloads[0]),
            no_new_privileges: true,
            effective_capabilities: 0,
        })
        .collect(),
    };
    (candidate, fence, observation)
}
#[test]
fn even_forged_perfect_receipt_cannot_authorize_confirm() {
    let (c, f, o) = fixture();
    let result = c.correlate(&f, &o, 200).unwrap();
    assert!(result.candidate_consistent && result.missing_trusted_producer);
    assert!(!result.may_confirm && !result.authorization_verified);
}
#[test]
fn wrong_candidate_source_epoch_manifest_rejected() {
    for i in 0..3 {
        let (c, f, mut o) = fixture();
        match i {
            0 => o.source_sha256 = "ff".repeat(32),
            1 => o.epoch += 1,
            _ => o.manifest_sha256 = "ff".repeat(32),
        };
        assert!(matches!(
            c.correlate(&f, &o, 200),
            Err(Rejection::Candidate)
        ));
    }
}
#[test]
fn old_boot_challenge_expiry_future_and_oversized_ttl_rejected() {
    for i in 0..6 {
        let (c, f, mut o) = fixture();
        match i {
            0 => o.boot = "ff".repeat(32),
            1 => o.challenge = "ff".repeat(32),
            2 => o.until_ms = 200,
            3 => o.at_ms = 201,
            4 => o.until_ms = 5101,
            _ => {
                o.at_ms = 0;
                o.until_ms = 200;
            }
        };
        assert!(c.correlate(&f, &o, 200).is_err());
    }
}
#[test]
fn live_process_with_wrong_root_data_or_schema_is_not_healthy() {
    for i in 0..4 {
        let (c, f, mut o) = fixture();
        match i {
            0 => o.root_sha256 = "ff".repeat(32),
            1 => o.data_generation += 1,
            2 => o.data_compatible = false,
            _ => o.data_schema = 4,
        };
        assert!(matches!(
            c.correlate(&f, &o, 200),
            Err(Rejection::RootOrData)
        ));
    }
}
#[test]
fn alive_wrong_uid_hash_caps_duplicate_pid_or_role_rejected() {
    for i in 0..8 {
        let (c, f, mut o) = fixture();
        match i {
            0 => o.processes[0].uid = 0,
            1 => o.processes[0].exe_sha256 = "ff".repeat(32),
            2 => o.processes[0].effective_capabilities = 1,
            3 => o.processes[0].no_new_privileges = false,
            4 => o.processes[0].pid = 101,
            5 => o.processes[0].start_ticks = 0,
            6 => o.processes[0].role = "rustsimhub-edge".into(),
            _ => {
                o.processes.pop();
            }
        };
        assert!(matches!(
            c.correlate(&f, &o, 200),
            Err(Rejection::Processes)
        ));
    }
}
#[cfg(target_os = "linux")]
#[test]
fn kernel_probe_reads_real_executable_and_refuses_missing_process() {
    let observed = probe_process(std::process::id(), "rustsimhub-core").unwrap();
    assert_eq!(
        observed.exe_sha256,
        hash_hex(&std::fs::read(std::env::current_exe().unwrap()).unwrap())
    );
    assert!(observed.start_ticks > 0);
    assert!(probe_process(0, "rustsimhub-core").is_err());
    assert!(probe_process(u32::MAX, "rustsimhub-core").is_err());
    assert!(probe_process(std::process::id(), "../../etc/passwd").is_err());
}
