// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Public, synthetic tests linked to Cargo's exact production library artifacts.
use rusqlite::{Connection, OpenFlags};
use rustsimhub_auth::{Auth, ClientKey, Identity, ProofExpectation};
use rustsimhub_domain::{CallAction, LineEpoch, SmsState};
use rustsimhub_policy::{Kind, Principal};
use rustsimhub_storage::{
    CallObservation, CallRequest, IncomingSms, LineObservation, LineStatus, SendContext,
    SmsRequest, Store,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
mod business;
mod identity;
mod outbox;

static NEXT: AtomicUsize = AtomicUsize::new(1);
const PHONE: &str = "+12025550123";
fn mkdir(path: &Path) {
    fs::create_dir(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn write(path: &Path, value: &[u8]) {
    fs::write(path, value).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn hash(path: &Path) -> [u8; 32] {
    Sha256::digest(fs::read(path).unwrap()).into()
}
fn readonly(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn closed_readonly(path: &Path) -> Connection {
    // Only completed offline snapshots, never a live database with WAL frames.
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    if wal.exists() {
        assert_eq!(fs::metadata(wal).unwrap().len(), 0);
    }
    let mut encoded = String::new();
    for byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Connection::open_with_flags(
        format!("file:{encoded}?immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap()
}
fn manifest(path: &Path) -> BTreeMap<PathBuf, [u8; 32]> {
    let mut out = BTreeMap::new();
    for item in fs::read_dir(path).unwrap() {
        let path = item.unwrap().path();
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.is_dir() {
            out.extend(manifest(&path));
        } else if metadata.is_file() {
            out.insert(path.clone(), hash(&path));
        } else {
            assert!(metadata.file_type().is_symlink());
        }
    }
    out
}
fn logical(path: &Path) -> BTreeMap<String, [u8; 32]> {
    logical_connection(readonly(path))
}
fn logical_closed(path: &Path) -> BTreeMap<String, [u8; 32]> {
    logical_connection(closed_readonly(path))
}
fn logical_connection(db: Connection) -> BTreeMap<String, [u8; 32]> {
    use rusqlite::types::ValueRef;
    assert_eq!(
        db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let tables = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT GLOB 'sqlite_*' ORDER BY name").unwrap()
        .query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    let mut result = BTreeMap::new();
    for name in tables {
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let mut stmt = db.prepare(&format!("SELECT * FROM {quoted}")).unwrap();
        let count = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        let mut encoded = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let mut bytes = Vec::new();
            for index in 0..count {
                let (tag, value) = match row.get_ref(index).unwrap() {
                    ValueRef::Null => (0, Vec::new()),
                    ValueRef::Integer(n) => (1, n.to_le_bytes().to_vec()),
                    ValueRef::Real(n) => (2, n.to_bits().to_le_bytes().to_vec()),
                    ValueRef::Text(v) => (3, v.to_vec()),
                    ValueRef::Blob(v) => (4, v.to_vec()),
                };
                bytes.push(tag);
                bytes.extend((value.len() as u64).to_le_bytes());
                bytes.extend(value);
            }
            encoded.push(bytes);
        }
        encoded.sort();
        let mut h = Sha256::new();
        for row in encoded {
            h.update((row.len() as u64).to_le_bytes());
            h.update(row);
        }
        result.insert(name, h.finalize().into());
    }
    result
}
struct Fixture {
    root: PathBuf,
    state: PathBuf,
    identity: PathBuf,
    owner: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(std::env::var_os("RSH_RECOVERY_SYNTHETIC_ROOT").unwrap());
        let root = base.join(format!("case-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
        mkdir(&root);
        let state = root.join("state");
        let identity = root.join("identity");
        let owner = root.join("owner");
        for p in [&state, &identity, &owner] {
            mkdir(p);
        }
        Self {
            root,
            state,
            identity,
            owner,
        }
    }
    fn dir(&self, name: &str) -> PathBuf {
        let p = self.root.join(name);
        mkdir(&p);
        p
    }
    fn open(&self) -> (Store, ClientKey, Principal, [u8; 32]) {
        let key = ClientKey::create(&self.owner).unwrap();
        let identity = Identity::open(&self.identity).unwrap();
        let pin = identity.public_key();
        let store = Store::open(&self.state).unwrap();
        store.handle().bind_identity(pin).unwrap();
        let owner = store.handle().bootstrap_owner(key.public_key()).unwrap();
        (store, key, owner, pin)
    }
    fn backup(&self, store: &Store, name: &str) -> PathBuf {
        let path = self.dir(name);
        store.handle().backup(&path).unwrap();
        path
    }
}
fn observe(store: &Store, fingerprint: [u8; 32]) -> SendContext {
    let line = store
        .handle()
        .observe_line(LineObservation {
            backend: "synthetic-recovery".into(),
            fingerprint: Some(fingerprint),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    SendContext {
        line,
        backend: "synthetic-recovery".into(),
        owner: ":1.70".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint,
    }
}
fn request(client: &str, id: &str, line: LineEpoch) -> SmsRequest {
    SmsRequest {
        client_id: client.into(),
        idempotency_key: id.into(),
        line,
        recipient: PHONE.into(),
        text: "synthetic offline backup test".into(),
    }
}
fn incoming(store: &Store, line: LineEpoch, id: usize) -> String {
    store
        .handle()
        .receive_sms(IncomingSms {
            line,
            source: format!("synthetic-{id}"),
            sender: PHONE.into(),
            timestamp: "2026-10-27T00:00:00Z".into(),
            text: format!("synthetic inbox {id}"),
            data: vec![],
        })
        .unwrap()
        .id
}
fn recover_owner(store: &Store, identity: &Path, owner: &ClientKey, pin: [u8; 32]) -> Principal {
    let mut auth = Auth::open(identity, store.handle()).unwrap();
    let challenge = auth.recovery_challenge(owner.public_key()).unwrap();
    let proof = owner
        .prove(&challenge, pin, ProofExpectation::RecoverOwner)
        .unwrap();
    auth.recovery_finish(&challenge.id, &proof).unwrap()
}
