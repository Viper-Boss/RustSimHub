// SPDX-License-Identifier: AGPL-3.0-only
//! Existing T5 shape, admitted through the original private Root context.
//! These metadata values never grant Root, member or business authority.
use super::*;
use serde::{Deserialize, Serialize};
fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub contract: u8,
    pub source: String,
    pub generation: u32,
    pub sequence: u32,
    #[serde(deserialize_with = "required")]
    pub age_ms: Option<u64>,
    pub enabled: String,
    pub service: String,
    pub path: String,
    pub dht: String,
    #[serde(deserialize_with = "required")]
    pub rx_bytes: Option<String>,
    #[serde(deserialize_with = "required")]
    pub tx_bytes: Option<String>,
    #[serde(deserialize_with = "required")]
    pub rtt_ms: Option<u32>,
    #[serde(deserialize_with = "required")]
    pub peer_count: Option<u32>,
    #[serde(deserialize_with = "required")]
    pub last_error: Option<String>,
}
impl Status {
    pub fn unavailable(code: &str) -> Self {
        Self {
            contract: 1,
            source: "unavailable".into(),
            generation: 0,
            sequence: 0,
            age_ms: None,
            enabled: "unknown".into(),
            service: "unknown".into(),
            path: "unknown".into(),
            dht: "unknown".into(),
            rx_bytes: None,
            tx_bytes: None,
            rtt_ms: None,
            peer_count: None,
            last_error: Some(code.into()),
        }
    }
    fn validate(&self) -> Result<()> {
        let one = |s: &str, allowed: &[&str]| allowed.contains(&s);
        let counter = |v: &Option<String>| {
            v.as_ref().is_none_or(|s| {
                s.len() <= 20 && s.parse::<u64>().is_ok_and(|n| n.to_string() == *s)
            })
        };
        if self.contract != 1
            || !one(&self.source, &["engine", "unavailable"])
            || !one(&self.enabled, &["on", "off", "unknown"])
            || !one(
                &self.service,
                &[
                    "stopped", "starting", "running", "stopping", "failed", "unknown",
                ],
            )
            || !one(&self.path, &["direct", "relay", "offline", "unknown"])
            || !one(&self.dht, &["connected", "disconnected", "unknown"])
            || !counter(&self.rx_bytes)
            || !counter(&self.tx_bytes)
            || self.rtt_ms.is_some_and(|n| n > 60000)
            || self.peer_count.is_some_and(|n| n > 100000)
            || self.last_error.as_ref().is_some_and(|s| {
                !one(
                    s,
                    &[
                        "engine_unavailable",
                        "deadline",
                        "protocol_error",
                        "stale",
                        "permission_denied",
                    ],
                )
            })
        {
            return Err(PortError::Protocol);
        }
        if self.source == "engine" {
            if self.generation == 0 || self.sequence == 0 || self.age_ms.is_none_or(|n| n > 5000) {
                return Err(PortError::Protocol);
            }
        } else if self.age_ms.is_some()
            || [&self.enabled, &self.service, &self.path, &self.dht]
                .iter()
                .any(|v| v.as_str() != "unknown")
            || self.rx_bytes.is_some()
            || self.tx_bytes.is_some()
            || self.rtt_ms.is_some()
            || self.peer_count.is_some()
        {
            return Err(PortError::Protocol);
        }
        Ok(())
    }
}
pub(super) struct Sample {
    pub status: Status,
    pub at: u64,
}
pub(super) fn read(stream: &mut UnixStream, deadline: Instant) -> Result<Sample> {
    let mut header = [0; 17];
    read_exact(stream, &mut header, deadline)?;
    if &header[..5] != b"RSRD\x01" {
        return Err(PortError::Protocol);
    }
    let at = u64::from_be_bytes(header[5..13].try_into().map_err(|_| PortError::Protocol)?);
    let length =
        u32::from_be_bytes(header[13..17].try_into().map_err(|_| PortError::Protocol)?) as usize;
    if length == 0 || length > 2048 || at == 0 {
        return Err(PortError::Protocol);
    }
    let mut raw = vec![0; length];
    read_exact(stream, &mut raw, deadline)?;
    let status: Status = serde_json::from_slice(&raw).map_err(|_| PortError::Protocol)?;
    status.validate()?;
    Ok(Sample { status, at })
}
/// A separate kernel-admitted observer connection. Losing it cannot close
/// the original owner FD, refresh Root evidence or stop another reference.
fn query(shared: &Shared, deadline: Instant) -> Result<(Observation, Sample)> {
    if !shared.sdk_status || shared.generation.load(Ordering::Acquire) == 0 {
        return Err(PortError::Unavailable);
    }
    shared.authority()?;
    let generation = shared.generation.load(Ordering::Acquire);
    let mut stream = connect()?;
    stream
        .set_write_timeout(Some(
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_nanos(1)),
        ))
        .map_err(|_| PortError::Io)?;
    let mut raw = [0u8; 82];
    raw[..5].copy_from_slice(b"RSCN\x01");
    raw[5] = 6;
    raw[6..10].copy_from_slice(&generation.to_be_bytes());
    raw[10..18].copy_from_slice(&shared.boot_deadline.to_be_bytes());
    raw[18..50].copy_from_slice(&shared.nonce);
    raw[50..82].copy_from_slice(&shared.policy);
    if stream.write(&raw).map_err(|_| PortError::Io)? != raw.len() {
        return Err(PortError::Protocol);
    }
    let mut base = [0; 87];
    read_exact(&mut stream, &mut base, deadline)?;
    let observation = parse_reply(
        &base,
        generation,
        shared.boot_deadline,
        &shared.nonce,
        &shared.policy,
    )?;
    let sample = read(&mut stream, deadline)?;
    if Instant::now() >= deadline {
        return Err(PortError::Deadline);
    }
    shared.authority()?;
    Ok((observation, sample))
}
fn read_exact(stream: &mut UnixStream, out: &mut [u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    while offset < out.len() {
        let now = Instant::now();
        if now >= deadline {
            return Err(PortError::Deadline);
        };
        stream
            .set_read_timeout(Some(deadline - now))
            .map_err(|_| PortError::Io)?;
        let n = stream.read(&mut out[offset..]).map_err(|_| PortError::Io)?;
        if n == 0 {
            return Err(PortError::Io);
        };
        offset += n;
    }
    Ok(())
}
#[derive(Clone, Serialize)]
pub struct Owners {
    pub manual: bool,
    pub remote: bool,
    pub statistics: bool,
}
#[derive(Serialize)]
pub struct Report {
    pub contract: u8,
    pub core_instance: String,
    pub owners: Owners,
    pub supervisor_state: &'static str,
    pub root_generation: Option<u32>,
    pub original_process_exit_verified: bool,
    pub status: Status,
}
impl Controller {
    /// Exact local ownership tickets only; no Root, SDK, credential or lease IO.
    pub fn owners(&self) -> Result<Owners> {
        let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        Ok(state.as_ref().map_or(
            Owners {
                manual: false,
                remote: false,
                statistics: false,
            },
            |r| Owners {
                manual: r.references[0].is_some(),
                remote: r.references[1].is_some(),
                statistics: r.references[2].is_some(),
            },
        ))
    }

    /// Current Owner read permission only; does not refresh recent login, create
    /// a proof, add references, drive cleanup, open an engine or connect Root OFF.
    pub fn read_status(&self, token: &str) -> Result<Report> {
        let instance = {
            let mut auth = self.auth.try_lock().map_err(|_| PortError::Unavailable)?;
            let p = auth
                .authorize(token, rustsimhub_policy::Scope::SystemManage)
                .map_err(|_| PortError::Denied)?;
            if p.kind != rustsimhub_policy::Kind::Owner || !p.active {
                return Err(PortError::Denied);
            }
            auth.observer_instance().to_owned()
        };
        let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        let mut report = Report {
            contract: 1,
            core_instance: instance,
            owners: Owners {
                manual: false,
                remote: false,
                statistics: false,
            },
            supervisor_state: "off",
            root_generation: None,
            original_process_exit_verified: false,
            status: Status::unavailable("engine_unavailable"),
        };
        let Some(run) = state.as_ref() else {
            return Ok(report);
        };
        report.owners = Owners {
            manual: run.references[0].is_some(),
            remote: run.references[1].is_some(),
            statistics: run.references[2].is_some(),
        };
        let snapshot = run.supervisor.snapshot();
        use rustsimhub_nkn_agent::Phase;
        report.supervisor_state = match snapshot.phase {
            Phase::Disabled => "off",
            Phase::Starting => "starting",
            Phase::Ready => "ready",
            Phase::WaitingRetry => "waiting_retry",
            Phase::Stopping => "stopping",
            Phase::Unknown => "unknown",
        };
        let shared = &run.source.shared;
        let generation = shared.generation.load(Ordering::Acquire);
        report.root_generation = (generation != 0).then_some(generation);
        report.original_process_exit_verified = snapshot.phase == Phase::Disabled
            && shared.observed_root.load(Ordering::Acquire) == 2
            && shared.observed_engine.load(Ordering::Acquire) == 3;
        if snapshot.phase != Phase::Ready || !shared.sdk_status || !shared.local_live() {
            return Ok(report);
        }
        let (observation, mut sample) = query(shared, Instant::now() + Duration::from_millis(750))?;
        if sample.status.source == "engine" {
            let age = boot_ms()?
                .checked_sub(sample.at)
                .and_then(|n| n.checked_add(sample.status.age_ms?))
                .ok_or(PortError::Protocol)?;
            if age > 5000 {
                report.status = Status::unavailable("stale");
                return Ok(report);
            }
            sample.status.age_ms = Some(age);
            sample.status.enabled = if snapshot.references > 0 { "on" } else { "off" }.into();
            // Actual Root-owned ready PID/cgroup evidence is distinct from DHT/path.
            sample.status.service = if observation.root == 1 && observation.engine == 2 {
                "running"
            } else {
                "unknown"
            }
            .into();
        }
        report.status = sample.status;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::os::unix::fs::PermissionsExt;
    fn sample() -> Status {
        Status {
            contract: 1,
            source: "engine".into(),
            generation: 2,
            sequence: 1,
            age_ms: Some(0),
            enabled: "unknown".into(),
            service: "unknown".into(),
            path: "unknown".into(),
            dht: "unknown".into(),
            rx_bytes: None,
            tx_bytes: None,
            rtt_ms: None,
            peer_count: None,
            last_error: None,
        }
    }
    #[test]
    fn t5_requires_all_fields_exact_enums_and_no_invented_unknown_counters() {
        let original = serde_json::to_value(sample()).unwrap();
        for key in original.as_object().unwrap().keys() {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<Status>(value).is_err());
        }
        for (key, value) in [
            ("source", serde_json::json!("mock")),
            ("rx_bytes", serde_json::json!("00")),
            ("tx_bytes", serde_json::json!("18446744073709551616")),
            ("peer_count", serde_json::json!(100001)),
            ("last_error", serde_json::json!("SYNTHETIC_PRIVATE")),
            ("generation", serde_json::json!(0)),
            ("age_ms", serde_json::json!(5001)),
        ] {
            let mut changed = original.clone();
            changed[key] = value;
            let parsed: Status = serde_json::from_value(changed).unwrap();
            assert!(parsed.validate().is_err());
        }
        let raw = serde_json::to_string(&sample()).unwrap();
        let duplicate = raw.replacen("\"contract\":1", "\"contract\":1,\"contract\":1", 1);
        assert!(serde_json::from_str::<Status>(&duplicate).is_err());
        let mut unavailable = Status::unavailable("engine_unavailable");
        unavailable.validate().unwrap();
        unavailable.rx_bytes = Some("0".into());
        assert!(unavailable.validate().is_err());
    }
    #[test]
    fn private_sample_reader_is_bounded_and_keeps_original_boot_age() {
        let raw = serde_json::to_vec(&sample()).unwrap();
        let mut frame = b"RSRD\x01".to_vec();
        frame.extend_from_slice(&123u64.to_be_bytes());
        frame.extend_from_slice(&(raw.len() as u32).to_be_bytes());
        frame.extend_from_slice(&raw);
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(&frame).unwrap();
        let result = read(&mut reader, Instant::now() + Duration::from_secs(1)).unwrap();
        assert_eq!(result.at, 123);
        assert_eq!(result.status, sample());
        for length in [0u32, 2049] {
            let (mut reader, mut writer) = UnixStream::pair().unwrap();
            let mut bad = b"RSRD\x01".to_vec();
            bad.extend_from_slice(&123u64.to_be_bytes());
            bad.extend_from_slice(&length.to_be_bytes());
            writer.write_all(&bad).unwrap();
            assert!(read(&mut reader, Instant::now() + Duration::from_secs(1)).is_err());
        }
    }
    #[test]
    fn actual_owner_off_reads_do_not_create_root_authority_or_claim_process_exit() {
        let state = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        for d in [state.path(), identity.path()] {
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = rustsimhub_storage::Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let owner = SigningKey::from_bytes(&[71; 32]);
        let challenge = auth
            .bootstrap_challenge(owner.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&challenge.id, &owner.sign(&challenge.transcript).to_bytes())
            .unwrap();
        let login = |auth: &mut Auth, key: &SigningKey| {
            let challenge = auth
                .login_challenge(key.verifying_key().to_bytes())
                .unwrap();
            auth.login_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
                .unwrap()
                .expose()
                .to_owned()
        };
        let token = login(&mut auth, &owner);
        let principal = store
            .handle()
            .principal(owner.verifying_key().to_bytes())
            .unwrap()
            .unwrap();
        let client = SigningKey::from_bytes(&[72; 32]);
        store
            .handle()
            .approve_client(
                principal,
                client.verifying_key().to_bytes(),
                rustsimhub_policy::Kind::Client,
                rustsimhub_policy::ALL_SCOPES,
            )
            .unwrap();
        let client_token = login(&mut auth, &client);
        let auth = Arc::new(Mutex::new(auth));
        let controller = Controller::new(auth.clone());
        for _ in 0..3 {
            let report = controller.read_status(&token).unwrap();
            assert!(report.root_generation.is_none());
            assert_eq!(report.status.source, "unavailable");
            assert!(!report.original_process_exit_verified);
            assert!(!report.owners.manual && !report.owners.remote && !report.owners.statistics);
            assert!(controller.snapshot().unwrap().is_none());
        }
        assert!(controller.read_status(&client_token).is_err());
        assert!(controller.read_status("untrusted").is_err());
        assert!(controller.snapshot().unwrap().is_none());
    }
}
