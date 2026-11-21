// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed candidate operations on a separately connected original Root observer.
//! Does not start Root/SDK, acquire a reference, create identity or sign anything.
use super::*;
use crate::nkn_statistics::{Error as SignerError, SubscriptionSigner};
use rustsimhub_usage_stats::{
    check_in::{AttemptBudget, StatisticsLease},
    protocol::{CheckInIntent, StatsPublicKey},
    store::FileStore,
    transaction::PreparedSubscription,
};
use serde::{Deserialize, Serialize};

pub enum TransactionCall<'a> {
    Preflight,
    Submit(&'a PreparedSubscription),
    Query {
        intent: CheckInIntent,
        digest: [u8; 32],
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionConclusion {
    Nonce(u64),
    Accepted,
    Confirmed,
    Unknown,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledPolicy {
    format: u8,
    source_commit: String,
    engine_sha256: String,
    core_pin: StatsPublicKey,
    statistics_public_key: StatsPublicKey,
    interface: String,
    seed_rpc: Vec<String>,
    #[serde(default)]
    status_protocol: u8,
    #[serde(default)]
    usage_query_protocol: u8,
    #[serde(default)]
    usage_transaction_protocol: u8,
}
pub(super) fn policy_key(raw: &[u8], device: [u8; 32]) -> Result<Option<StatsPublicKey>> {
    let p: InstalledPolicy = serde_json::from_slice(raw).map_err(|_| PortError::Protocol)?;
    let hex = |s: &str, n: usize| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if p.format != 1
        || !hex(&p.source_commit, 40)
        || !hex(&p.engine_sha256, 64)
        || p.core_pin.as_bytes() != &device
        || p.statistics_public_key.as_bytes() == &[0; 32]
        || p.statistics_public_key == p.core_pin
        || !["usb0", "wlan0"].contains(&p.interface.as_str())
        || p.seed_rpc.is_empty()
        || p.seed_rpc.len() > 3
        || p.seed_rpc
            .iter()
            .any(|s| s.len() > 256 || !s.starts_with("https://"))
        || p.status_protocol > 1
        || p.usage_query_protocol > 1
        || p.usage_transaction_protocol > 1
        || p.usage_transaction_protocol == 1 && p.status_protocol != 1
    {
        return Err(PortError::Protocol);
    }
    Ok((p.usage_transaction_protocol == 1).then_some(p.statistics_public_key))
}

#[derive(Serialize)]
struct Request<'a> {
    version: u8,
    operation_id: String,
    participation_generation: u64,
    public_key: StatsPublicKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent: Option<&'a CheckInIntent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction: Option<String>,
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    let value = String::deserialize(d)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(serde::de::Error::custom("invalid statistics digest"));
    }
    Ok(Some(value))
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    version: u8,
    operation_id: String,
    participation_generation: u64,
    public_key: StatsPublicKey,
    status: Verdict,
    #[serde(default, deserialize_with = "present")]
    nonce: Option<u64>,
    #[serde(default, deserialize_with = "present")]
    intent: Option<CheckInIntent>,
    #[serde(default, deserialize_with = "digest")]
    digest: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    Ready,
    Accepted,
    Confirmed,
    Unknown,
    Unavailable,
    Denied,
}

impl TransactionCall<'_> {
    fn op(&self) -> u8 {
        match self {
            Self::Preflight => 10,
            Self::Submit(_) => 11,
            Self::Query { .. } => 12,
        }
    }
    fn intent(&self) -> Option<CheckInIntent> {
        match self {
            Self::Preflight => None,
            Self::Submit(p) => Some(p.intent()),
            Self::Query { intent, .. } => Some(*intent),
        }
    }
    fn digest(&self) -> Option<[u8; 32]> {
        match self {
            Self::Preflight => None,
            Self::Submit(p) => Some(p.digest()),
            Self::Query { digest, .. } => Some(*digest),
        }
    }
    fn request(&self, id: String, generation: u64, key: StatsPublicKey) -> Result<Vec<u8>> {
        if id == "00000000-0000-0000-0000-000000000000"
            || generation == 0
            || generation >= i64::MAX as u64
            || key.as_bytes() == &[0; 32]
            || self.intent().is_some_and(|in_| in_.public_key() != key)
        {
            return Err(PortError::Denied);
        }
        let intent = self.intent();
        let transaction = match self {
            Self::Submit(p) => {
                if p.transaction_bytes().is_empty() || p.transaction_bytes().len() > 768 {
                    return Err(PortError::Protocol);
                }
                Some(hex(p.transaction_bytes()))
            }
            _ => None,
        };
        let raw = serde_json::to_vec(&Request {
            version: 1,
            operation_id: id,
            participation_generation: generation,
            public_key: key,
            intent: intent.as_ref(),
            digest: self.digest().as_ref().map(|d| hex(d)),
            transaction,
        })
        .map_err(|_| PortError::Protocol)?;
        if raw.is_empty() || raw.len() > 2048 {
            return Err(PortError::Protocol);
        }
        Ok(raw)
    }
    fn reply(
        &self,
        raw: &[u8],
        id: &str,
        generation: u64,
        key: StatsPublicKey,
    ) -> Result<TransactionConclusion> {
        if raw.is_empty() || raw.len() > 2048 {
            return Err(PortError::Protocol);
        }
        let v: Reply = serde_json::from_slice(raw).map_err(|_| PortError::Protocol)?;
        if v.version != 1
            || v.operation_id != id
            || v.participation_generation != generation
            || v.public_key != key
            || v.intent != self.intent()
            || v.digest != self.digest().as_ref().map(|d| hex(d))
            || self.op() != 10 && v.nonce.is_some()
        {
            return Err(PortError::Protocol);
        }
        match v.status {
            Verdict::Ready if self.op() == 10 => v
                .nonce
                .map(TransactionConclusion::Nonce)
                .ok_or(PortError::Protocol),
            Verdict::Accepted if self.op() == 11 && v.nonce.is_none() => {
                Ok(TransactionConclusion::Accepted)
            }
            Verdict::Confirmed if self.op() == 12 => Ok(TransactionConclusion::Confirmed),
            Verdict::Unknown if self.op() != 10 && v.nonce.is_none() => {
                Ok(TransactionConclusion::Unknown)
            }
            Verdict::Unavailable if v.nonce.is_none() => Err(PortError::Unavailable),
            Verdict::Denied if v.nonce.is_none() => Err(PortError::Denied),
            _ => Err(PortError::Protocol),
        }
    }
}

#[derive(Clone, Copy)]
struct Registration {
    generation: u32,
    boot_deadline: u64,
    nonce: [u8; 32],
    policy: [u8; 32],
    pid: u32,
}
fn io_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
    )
}
fn check(budget: &AttemptBudget, deadline: Instant, fence: &dyn Fn() -> Result<()>) -> Result<()> {
    if budget.cancelled() || budget.expired() || Instant::now() >= deadline {
        return Err(PortError::Deadline);
    }
    fence()?;
    if budget.cancelled() || budget.expired() || Instant::now() >= deadline {
        return Err(PortError::Deadline);
    }
    Ok(())
}
fn read(
    stream: &mut UnixStream,
    raw: &mut [u8],
    budget: &AttemptBudget,
    deadline: Instant,
    fence: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let mut offset = 0;
    while offset < raw.len() {
        check(budget, deadline, fence)?;
        stream
            .set_read_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(25))
                    .max(Duration::from_nanos(1)),
            ))
            .map_err(|_| PortError::Io)?;
        match stream.read(&mut raw[offset..]) {
            Ok(0) => return Err(PortError::Io),
            Ok(n) => offset += n,
            Err(e) if io_error(&e) => continue,
            Err(_) => return Err(PortError::Io),
        }
    }
    Ok(())
}
struct Wire<'a> {
    registration: Registration,
    call: &'a TransactionCall<'a>,
    raw: &'a [u8],
    id: &'a str,
    generation: u64,
    key: StatsPublicKey,
}
struct IoBudget<'a> {
    budget: &'a AttemptBudget,
    deadline: Instant,
    fence: &'a dyn Fn() -> Result<()>,
    signer: &'a SubscriptionSigner,
    store: &'a FileStore,
    reference: Option<ReferenceContext<'a>>,
}
#[derive(Clone, Copy)]
struct ReferenceContext<'a> {
    controller: &'a Controller,
    shared: &'a Arc<Shared>,
    epoch: u64,
    registration: Registration,
    lease: [u8; 16],
}
fn write_now(stream: &mut UnixStream, raw: &[u8]) -> std::io::Result<usize> {
    // This dedicated observer has no cloned/shared file description. Never
    // wait on a socket with Auth/ledger/reference locks held.
    stream.set_nonblocking(true)?;
    let result = stream.write(raw);
    stream.set_nonblocking(false)?;
    result
}
fn check_persisted(store: &FileStore, call: &TransactionCall<'_>) -> Result<()> {
    let state = store.load().map_err(|_| PortError::Protocol)?;
    let Some(intent) = call.intent() else {
        return if state.windows.iter().all(|w| w.pending.is_none()) {
            Ok(())
        } else {
            Err(PortError::Denied)
        };
    };
    let window = &state.windows[intent.window().index()];
    if window.period != Some(intent.renewal_period())
        || window.pending.as_ref().map(|p| p.digest) != call.digest()
    {
        return Err(PortError::Denied);
    }
    Ok(())
}
impl IoBudget<'_> {
    fn write(
        &self,
        stream: &mut UnixStream,
        raw: &[u8],
        call: &TransactionCall<'_>,
    ) -> Result<std::io::Result<usize>> {
        self.signer
            .with_current_action(self.store, self.budget, |auth| {
                check_persisted(self.store, call)?;
                if let Some(reference) = self.reference {
                    let state = reference
                        .controller
                        .run
                        .try_lock()
                        .map_err(|_| PortError::Unavailable)?;
                    let run = state.as_ref().ok_or(PortError::Denied)?;
                    if reference.controller.closed.load(Ordering::Acquire)
                        || run.statistics_epoch != reference.epoch
                        || run.references[2].is_none()
                        || run.check_in.as_ref() != Some(&reference.lease)
                        || !Arc::ptr_eq(&run.source.shared, reference.shared)
                        || run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
                        || !reference.shared.local_live()
                        || reference.shared.generation.load(Ordering::Acquire)
                            != reference.registration.generation
                        || reference.shared.observed_pid.load(Ordering::Acquire)
                            != reference.registration.pid
                        || auth
                            .check_nkn_root_authority(&reference.shared.authority)
                            .is_err()
                    {
                        return Err(PortError::Denied);
                    }
                    // Keep the exact reference epoch locked through this one
                    // nonblocking write; a completed OFF/reacquire cannot pass it.
                    Ok(write_now(stream, raw))
                } else {
                    // Private codec tests use real Auth/permission but do not
                    // claim the installed Root registration/kernel proof.
                    Ok(write_now(stream, raw))
                }
            })
            .map_err(signer_error)?
    }
}
fn exchange(
    stream: &mut UnixStream,
    wire: Wire<'_>,
    io: IoBudget<'_>,
) -> Result<TransactionConclusion> {
    let Wire {
        registration,
        call,
        raw,
        id,
        generation,
        key,
    } = wire;
    let (budget, deadline, fence) = (io.budget, io.deadline, io.fence);
    let mut frame = Vec::with_capacity(91 + raw.len());
    frame.extend_from_slice(b"RSCN\x01");
    frame.push(call.op());
    frame.extend_from_slice(&registration.generation.to_be_bytes());
    frame.extend_from_slice(&registration.boot_deadline.to_be_bytes());
    frame.extend_from_slice(&registration.nonce);
    frame.extend_from_slice(&registration.policy);
    frame.extend_from_slice(b"RSTX\x01");
    frame.extend_from_slice(&(raw.len() as u32).to_be_bytes());
    frame.extend_from_slice(raw);
    let mut offset = 0;
    while offset < frame.len() {
        check(budget, deadline, fence)?;
        stream
            .set_write_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(25))
                    .max(Duration::from_nanos(1)),
            ))
            .map_err(|_| PortError::Io)?;
        match io.write(stream, &frame[offset..], call)? {
            Ok(0) => return Err(PortError::Io),
            Ok(n) => offset += n,
            Err(e) if io_error(&e) => continue,
            Err(_) => return Err(PortError::Io),
        }
    }
    let mut base = [0; 87];
    read(stream, &mut base, budget, deadline, fence)?;
    let obs = parse_reply(
        &base,
        registration.generation,
        registration.boot_deadline,
        &registration.nonce,
        &registration.policy,
    )?;
    if obs.root != 1 || obs.engine != 2 || obs.pid != registration.pid {
        return Err(PortError::Denied);
    }
    let mut header = [0; 9];
    read(stream, &mut header, budget, deadline, fence)?;
    let len = u32::from_be_bytes(header[5..].try_into().map_err(|_| PortError::Protocol)?) as usize;
    if &header[..5] != b"RSTX\x01" || len == 0 || len > 2048 {
        return Err(PortError::Protocol);
    }
    let mut body = vec![0; len];
    read(stream, &mut body, budget, deadline, fence)?;
    // This is a one-shot observer: require EOF and reject appended frames. Do
    // not half-close the write end; Root treats observer EOF as cancellation.
    loop {
        check(budget, deadline, fence)?;
        stream
            .set_read_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(25))
                    .max(Duration::from_nanos(1)),
            ))
            .map_err(|_| PortError::Io)?;
        match stream.read(&mut [0; 1]) {
            Ok(0) => break,
            Ok(_) => return Err(PortError::Protocol),
            Err(e) if io_error(&e) => continue,
            Err(_) => return Err(PortError::Io),
        }
    }
    check(budget, deadline, fence)?;
    call.reply(&body, id, generation, key)
}

fn signer_error(e: SignerError) -> PortError {
    match e {
        SignerError::Deadline => PortError::Deadline,
        SignerError::Busy => PortError::Unavailable,
        SignerError::Protocol | SignerError::Store => PortError::Protocol,
        SignerError::Denied => PortError::Denied,
    }
}
impl Controller {
    /// Candidate finite operation on an already acquired statistics reference.
    /// Every IO poll rechecks the original real Auth/consent/store and reference
    /// epoch. No caller-supplied proof callback or endpoint exists in this API.
    pub fn statistics_transaction(
        &self,
        signer: &SubscriptionSigner,
        store: &FileStore,
        lease: &StatisticsLease,
        call: TransactionCall<'_>,
        budget: &AttemptBudget,
    ) -> Result<TransactionConclusion> {
        if !signer.shares_auth(&self.auth) {
            return Err(PortError::Denied);
        }
        let generation = signer.check_current(store, budget).map_err(signer_error)?;
        check_persisted(store, &call)?;
        let key = store.identity().public_key();
        let (shared, epoch) = {
            let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
            let run = state.as_ref().ok_or(PortError::Unavailable)?;
            if self.closed.load(Ordering::Acquire)
                || run.references[2].is_none()
                || run.check_in.as_ref() != Some(lease.id())
                || run.statistics_epoch == 0
                || run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
                || run.source.shared.transaction_key != Some(key)
            {
                return Err(PortError::Unavailable);
            }
            (run.source.shared.clone(), run.statistics_epoch)
        };
        let registration = Registration {
            generation: shared.generation.load(Ordering::Acquire),
            boot_deadline: shared.boot_deadline,
            nonce: shared.nonce,
            policy: shared.policy,
            pid: shared.observed_pid.load(Ordering::Acquire),
        };
        if registration.generation == 0 || registration.pid <= 1 {
            return Err(PortError::Unavailable);
        }
        let id = uuid::Uuid::from_bytes(*lease.id()).to_string();
        let raw = call.request(id.clone(), generation, key)?;
        let limit = match call {
            TransactionCall::Query { .. } => Duration::from_secs(5),
            _ => Duration::from_secs(2),
        };
        let deadline = budget
            .deadline()
            .min(shared.deadline)
            .min(Instant::now() + limit);
        let fence = || {
            if signer.check_current(store, budget).map_err(signer_error)? != generation {
                return Err(PortError::Denied);
            }
            check_persisted(store, &call)?;
            {
                let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
                let run = state.as_ref().ok_or(PortError::Denied)?;
                if self.closed.load(Ordering::Acquire)
                    || run.statistics_epoch != epoch
                    || run.references[2].is_none()
                    || run.check_in.as_ref() != Some(lease.id())
                    || !Arc::ptr_eq(&run.source.shared, &shared)
                    || run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
                {
                    return Err(PortError::Denied);
                }
            }
            if shared.generation.load(Ordering::Acquire) != registration.generation
                || shared.observed_pid.load(Ordering::Acquire) != registration.pid
            {
                return Err(PortError::Denied);
            }
            shared.authority()
        };
        check(budget, deadline, &fence)?;
        let mut stream = connect()?;
        let result = exchange(
            &mut stream,
            Wire {
                registration,
                call: &call,
                raw: &raw,
                id: &id,
                generation,
                key,
            },
            IoBudget {
                budget,
                deadline,
                fence: &fence,
                signer,
                store,
                reference: Some(ReferenceContext {
                    controller: self,
                    shared: &shared,
                    epoch,
                    registration,
                    lease: *lease.id(),
                }),
            },
        );
        let _ = stream.shutdown(Shutdown::Both); // ONLY this separately opened observer.
        result
    }
}

#[cfg(test)]
pub(super) mod tests;
