// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-path, read-only disk configuration/data observation. Not an update
//! authorization: effective systemd state and signed program bindings must be
//! verified by the maintenance action while this guard is still held.
use crate::{manifest_hash, Error};
use rustsimhub_core_launcher::Mode;
use serde::Serialize;
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
mod database;
pub mod product;
mod programs;
pub(crate) mod recovery;
pub(crate) mod retained;
pub(crate) mod running;
mod systemd;
use database::Database;
pub use programs::{
    admit_stage, Admission, SignedPrograms, SignedVersionCheck, SignedVersionReport,
};

#[cfg(feature = "email-production-candidate")]
pub use programs::{migrate_email4, migrate_email4_retained};

type Result<T> = std::result::Result<T, Error>;
const STATE: &str = "var/lib/rustsimhub/rustsimhub-v1/core/state";
const BASE_PATH: &str = "etc/systemd/system/rustsimhub-core.service";
const BASE: &str =
    include_str!("../../../firmware/system/overlay/etc/systemd/system/rustsimhub-core.service");

#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    size: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Metadata> for Stamp {
    fn from(m: Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            links: m.nlink(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}

fn denied_io<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|_| Error::Io)
}

fn private_file(path: &Path, uid: u32, mode: u32, limit: u64) -> Result<File> {
    let f = denied_io(
        OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(path),
    )?;
    let m = denied_io(f.metadata())?;
    if !m.is_file()
        || m.uid() != uid
        || m.mode() & 0o7777 != mode
        || m.nlink() != 1
        || m.len() > limit
    {
        return Err(Error::Denied);
    }
    Ok(f)
}

struct Observation {
    path: PathBuf,
    stamp: Option<Stamp>,
}
impl Observation {
    fn capture(path: &Path) -> Result<Self> {
        let stamp = match fs::symlink_metadata(path) {
            Ok(m) => Some(m.into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Error::Io),
        };
        Ok(Self {
            path: path.into(),
            stamp,
        })
    }
    fn revalidate(&self) -> Result<()> {
        if Self::capture(&self.path)?.stamp != self.stamp {
            return Err(Error::Busy);
        }
        Ok(())
    }
}

/// Remember every existing parent node, refusing links/unsafe ownership. Only
/// private tests may supply another root or UID; the production entry is fixed.
fn parents(
    root: &Path,
    relative: &Path,
    uids: &[u32],
    observations: &mut Vec<Observation>,
) -> Result<bool> {
    let mut path = root.to_path_buf();
    let mut parts = vec![PathBuf::new()];
    parts.extend(
        relative
            .parent()
            .ok_or(Error::Denied)?
            .components()
            .map(|c| PathBuf::from(c.as_os_str())),
    );
    for part in parts {
        path.push(part);
        let observed = Observation::capture(&path)?;
        if let Some(m) = &observed.stamp {
            if m.mode & 0o170000 != 0o040000 || !uids.contains(&m.uid) || m.mode & 0o6022 != 0 {
                return Err(Error::Denied);
            }
        } else {
            observations.push(observed);
            return Ok(false);
        }
        observations.push(observed);
    }
    Ok(true)
}

#[derive(Serialize)]
pub struct RootDigest {
    path: &'static str,
    sha256: String,
}
struct RootFile {
    _file: File,
    observed: Observation,
}

struct Material {
    path: &'static str,
    expected: Option<Vec<u8>>,
    mode: u32,
}
fn text(value: &str) -> Vec<u8> {
    value.replace("\r\n", "\n").into_bytes()
}
fn forwarding() -> Vec<Material> {
    let exec = BASE
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))
        .expect("compiled Core command");
    let command = exec.replace("--serve-device ", "--serve-device-forwarding-managed ");
    macro_rules! config {
        ($name:literal,$path:literal,$mode:expr) => {
            Material {
                path: $path,
                expected: Some(text(include_str!(concat!(
                    "../../../firmware/system/forwarding/",
                    $name
                )))),
                mode: $mode,
            }
        };
    }
    vec![
        config!("rustsimhub-forwarder.service","etc/systemd/system/rustsimhub-forwarder.service",0o644),
        config!("rustsimhub-forwarder-reconcile.service","etc/systemd/system/rustsimhub-forwarder-reconcile.service",0o644),
        config!("rustsimhub-forwarder-status.service","etc/systemd/system/rustsimhub-forwarder-status.service",0o644),
        config!("rustsimhub-forwarder-boot.service","etc/systemd/system/rustsimhub-forwarder-boot.service",0o644),
        config!("46-rustsimhub-forwarding.rules","etc/polkit-1/rules.d/46-rustsimhub-forwarding.rules",0o644),
        config!("forwarder.json","etc/rustsimhub/forwarder.json",0o600),
        config!("tmpfiles.conf","etc/tmpfiles.d/rustsimhub-forwarding.conf",0o644),
        config!("20-forwarding.conf","etc/systemd/system/rustsimhub-app.target.d/20-forwarding.conf",0o644),
        Material { path:"etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf", mode:0o644,
            expected:Some(format!("# SPDX-License-Identifier: AGPL-3.0-only\n[Service]\nExecStart=\nExecStart={command} 983\n").into_bytes()) },
    ]
}
fn setup_ap() -> Vec<Material> {
    macro_rules! config {
        ($name:literal,$path:literal) => {
            Material {
                path: $path,
                expected: Some(text(include_str!(concat!(
                    "../../../firmware/system/setup-ap/",
                    $name
                )))),
                mode: 0o644,
            }
        };
    }
    vec![
        Material {
            path: "usr/libexec/rustsimhub/rustsimhub-setup-ap",
            expected: None,
            mode: 0o755,
        },
        config!(
            "rustsimhub-setup-ap.service",
            "etc/systemd/system/rustsimhub-setup-ap.service"
        ),
        config!(
            "rustsimhub-setup-edge.service",
            "etc/systemd/system/rustsimhub-setup-edge.service"
        ),
        config!(
            "rustsimhub-setup-ap.tmpfiles",
            "etc/tmpfiles.d/rustsimhub-setup-ap.conf"
        ),
        config!(
            "50-setup-ap-core.conf",
            "etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf"
        ),
        config!(
            "50-setup-ap-target.conf",
            "etc/systemd/system/rustsimhub-app.target.d/50-setup-ap.conf"
        ),
        config!("contract.json", "etc/rustsimhub/setup-ap-contract.json"),
        Material {
            path: "usr/libexec/rustsimhub/rustsimhub-core-launcher",
            expected: None,
            mode: 0o755,
        },
    ]
}

/// Typed guards must remain alive until admission/mutation has finished.
/// A serialized report by itself cannot retain a writer fence.
pub struct MaintenanceSnapshot {
    product_qualified: bool,
    root_mode: Mode,
    contract: Option<String>,
    root_digests: Vec<RootDigest>,
    _root_files: Vec<RootFile>,
    observations: Vec<Observation>,
    // Release outbox before primary: normal Core startup acquires primary first.
    outbox: Option<Database>,
    business: Database,
}

/// Original build-info from programs whose signed bytes still need to be
/// verified by the maintenance caller. No pathname/argv is accepted here.
#[derive(Serialize)]
pub struct ProgramMetadata<'a> {
    pub core: &'a str,
    pub worker: Option<&'a str>,
}

/// Keep the snapshot borrowed while using its comparison. This is a version
/// preflight only, not a mutation capability: signature/effective-unit checks
/// are still required. Serialization cannot preserve a writer lock.
pub struct LiveVersionCheck<'a> {
    snapshot: &'a MaintenanceSnapshot,
    report: rustsimhub_core_launcher::UpgradeReport,
}
impl LiveVersionCheck<'_> {
    pub fn report(&self) -> &rustsimhub_core_launcher::UpgradeReport {
        &self.report
    }
    pub fn revalidate(&self) -> Result<()> {
        self.snapshot.revalidate()
    }
}

#[derive(Serialize)]
pub struct MaintenanceReport<'a> {
    format: u16,
    authorization_verified: bool,
    effective_systemd_verified: bool,
    signed_programs_verified: bool,
    root_mode: Mode,
    root_material: &'a [RootDigest],
    business_schema: i64,
    outbox: Option<database::Version>,
}

impl MaintenanceSnapshot {
    pub fn compare_versions<'a>(
        &'a self,
        selected: ProgramMetadata<'_>,
        candidate: ProgramMetadata<'_>,
        fallback: ProgramMetadata<'_>,
    ) -> Result<LiveVersionCheck<'a>> {
        self.revalidate()?;
        let schema = u32::try_from(self.business.version.format).map_err(|_| Error::Schema)?;
        let request = serde_json::json!({"format":1,"target_arch":"aarch64", "root_mode":self.root_mode,
            "root_contract":self.contract,"data":{"business_schema":schema,"outbox":self.outbox.as_ref().map(|v|v.version)},
            "selected":selected,"candidate":candidate,"fallback":fallback});
        let report = rustsimhub_core_launcher::check_upgrade(
            &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
        )
        .map_err(|_| Error::Invalid)?;
        self.revalidate()?;
        Ok(LiveVersionCheck {
            snapshot: self,
            report,
        })
    }

    pub fn observe() -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        Self::observe_at(Path::new("/"), 0, 986)
    }

    fn observe_at(root: &Path, root_uid: u32, data_uid: u32) -> Result<Self> {
        Self::observe_qualified(root, root_uid, data_uid, false)
    }
    fn observe_qualified(root: &Path, root_uid: u32, data_uid: u32, product: bool) -> Result<Self> {
        let mut observations = vec![];
        let mut files = vec![];
        let mut digests = vec![];
        let mut material = |spec: &Material| -> Result<bool> {
            let exists = parents(root, Path::new(spec.path), &[root_uid], &mut observations)?;
            let path = root.join(spec.path);
            let observed = Observation::capture(&path)?;
            if !exists || observed.stamp.is_none() {
                observations.push(observed);
                return Ok(false);
            }
            let limit = if spec.expected.is_some() {
                256 * 1024
            } else {
                24 * 1024 * 1024
            };
            let mut file = private_file(&path, root_uid, spec.mode, limit)?;
            let mut bytes = Vec::new();
            denied_io(file.by_ref().take(limit + 1).read_to_end(&mut bytes))?;
            if bytes.is_empty()
                || bytes.len() as u64 > limit
                || spec.expected.as_ref().is_some_and(|v| v != &bytes)
            {
                return Err(Error::Integrity);
            }
            if observed.stamp.as_ref() != Some(&denied_io(file.metadata())?.into()) {
                return Err(Error::Busy);
            }
            digests.push(RootDigest {
                path: spec.path,
                sha256: manifest_hash(&bytes),
            });
            files.push(RootFile {
                _file: file,
                observed,
            });
            Ok(true)
        };
        if !material(&Material {
            path: BASE_PATH,
            expected: Some(text(BASE)),
            mode: 0o644,
        })? {
            return Err(Error::Denied);
        }
        let fw = forwarding();
        let fw_count = fw
            .iter()
            .map(&mut material)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|v| *v)
            .count();
        let mut ap = setup_ap();
        if product {
            for material in &mut ap {
                if material.path == "usr/libexec/rustsimhub/rustsimhub-core-launcher"
                    && fs::symlink_metadata(root.join(material.path))
                        .is_ok_and(|m| m.mode() & 0o7777 == 0o555)
                {
                    material.mode = 0o555;
                }
            }
        }
        let ap_count = ap
            .iter()
            .map(&mut material)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|v| *v)
            .count();
        if ![0, fw.len()].contains(&fw_count)
            || ![0, ap.len()].contains(&ap_count)
            || (ap_count > 0 && fw_count == 0)
        {
            return Err(Error::Integrity);
        }
        let root_mode = if ap_count > 0 {
            Mode::SetupAp
        } else if fw_count > 0 {
            Mode::Forwarding
        } else {
            Mode::Device
        };
        check_core_overrides_qualified(root, root_uid, &mut observations, product)?;
        let contract = (ap_count > 0).then(|| {
            String::from_utf8(text(include_str!(
                "../../../firmware/system/setup-ap/contract.json"
            )))
            .expect("compiled contract")
        });
        let directory = root.join(STATE);
        if !parents(
            root,
            Path::new(STATE).join("store.sqlite3").as_path(),
            &[root_uid, data_uid],
            &mut observations,
        )? {
            return Err(Error::Denied);
        }
        // Incomplete restore is not a current, authoritative observation.
        absent(&directory.join(".restore-pending"), &mut observations)?;
        let business = Database::observe(
            &directory,
            "store.sqlite3",
            data_uid,
            true,
            512 * 1024 * 1024,
        )?;
        if business.version.application_id != 1381189698 {
            return Err(Error::Integrity);
        }
        let forwarding = directory.join("forwarding");
        let observed = Observation::capture(&forwarding)?;
        let outbox = if let Some(m) = &observed.stamp {
            if m.mode & 0o170000 != 0o040000 || m.mode & 0o7777 != 0o700 || m.uid != data_uid {
                return Err(Error::Denied);
            }
            if denied_io(fs::read_dir(&forwarding))?.next().is_none() {
                None
            } else {
                Some(Database::observe(
                    &forwarding,
                    "outbox.sqlite3",
                    data_uid,
                    false,
                    32 * 1024 * 1024,
                )?)
            }
        } else {
            None
        };
        observations.push(observed);
        let snapshot = Self {
            product_qualified: product,
            root_mode,
            contract,
            root_digests: digests,
            _root_files: files,
            observations,
            outbox,
            business,
        };
        snapshot.revalidate()?;
        Ok(snapshot)
    }

    pub fn revalidate(&self) -> Result<()> {
        for observed in &self.observations {
            observed.revalidate()?;
        }
        for file in &self._root_files {
            file.observed.revalidate()?;
        }
        self.business.revalidate()?;
        if let Some(outbox) = &self.outbox {
            outbox.revalidate()?;
        }
        Ok(())
    }

    pub fn root_contract(&self) -> Option<&str> {
        self.contract.as_deref()
    }

    pub fn report(&self) -> MaintenanceReport<'_> {
        MaintenanceReport {
            format: 1,
            authorization_verified: false,
            effective_systemd_verified: false,
            signed_programs_verified: false,
            root_mode: self.root_mode,
            root_material: &self.root_digests,
            business_schema: self.business.version.format,
            outbox: self.outbox.as_ref().map(|v| v.version),
        }
    }
}

fn absent(path: &Path, observations: &mut Vec<Observation>) -> Result<()> {
    let observation = Observation::capture(path)?;
    if observation.stamp.is_some() {
        return Err(Error::Busy);
    }
    observations.push(observation);
    Ok(())
}

fn check_core_overrides_qualified(
    root: &Path,
    uid: u32,
    observations: &mut Vec<Observation>,
    product: bool,
) -> Result<()> {
    for (directory, allow_known) in [
        ("etc/systemd/system/rustsimhub-core.service.d", true),
        (
            "etc/systemd/system.control/rustsimhub-core.service.d",
            false,
        ),
        (
            "run/systemd/system.control/rustsimhub-core.service.d",
            false,
        ),
        ("run/systemd/system/rustsimhub-core.service.d", false),
        (
            "usr/local/lib/systemd/system/rustsimhub-core.service.d",
            false,
        ),
        ("usr/lib/systemd/system/rustsimhub-core.service.d", false),
    ] {
        let relative = Path::new(directory).join(".observation-placeholder");
        if parents(root, &relative, &[uid], observations)? {
            for entry in denied_io(fs::read_dir(root.join(directory)))? {
                let name = denied_io(entry)?.file_name();
                let name = name.to_str().ok_or(Error::Integrity)?;
                if name.ends_with(".conf")
                    && (!allow_known
                        || (!["30-forwarding.conf", "50-setup-ap.conf"].contains(&name)
                            && !(product
                                && ["50-product-profile.conf", "55-product-health.conf"]
                                    .contains(&name))))
                {
                    return Err(Error::Integrity);
                }
            }
        }
    }
    for directory in [
        "etc/systemd/system.control",
        "run/systemd/system.control",
        "run/systemd/transient",
        "run/systemd/generator.early",
        "run/systemd/system",
        "run/systemd/generator",
        "run/systemd/generator.late",
    ] {
        let relative = Path::new(directory).join("rustsimhub-core.service");
        parents(root, &relative, &[uid], observations)?;
        absent(&root.join(relative), observations)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
