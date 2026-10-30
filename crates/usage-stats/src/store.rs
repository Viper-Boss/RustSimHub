//! Bounded, private, atomic persistence. Opening never creates a new identity.

use crate::identity::{StatisticsIdentity, IDENTITY_DOMAIN};
use crate::protocol::{StatsPublicKey, Window, RENEW_INTERVAL_SECS};
use crate::query::QueryCache;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zeroize::Zeroizing;

pub const STORE_FORMAT_VERSION: u32 = 1;
pub const MAX_STORE_BYTES: u64 = 128 * 1024;
const DATA_FILE: &str = "state.json";
const LOCK_FILE: &str = "writer.lock";

/// Reserved persistence fields for a later, separately reviewed confirmation
/// state machine. Recording a digest here does not submit a transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingTransaction {
    pub digest: [u8; 32],
    pub prepared_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastSuccess {
    pub period: u64,
    pub confirmed_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowState {
    pub window: Window,
    pub period: Option<u64>,
    pub pending: Option<PendingTransaction>,
    pub last_success: Option<LastSuccess>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredState {
    pub disabled_generation: u64,
    pub windows: [WindowState; 3],
    pub cache: QueryCache,
}

impl Default for StoredState {
    fn default() -> Self {
        Self {
            disabled_generation: 0,
            windows: Window::ALL.map(|window| WindowState {
                window,
                period: None,
                pending: None,
                last_success: None,
            }),
            cache: QueryCache::default(),
        }
    }
}

impl StoredState {
    pub fn validate(&self) -> Result<(), StoreError> {
        for (expected, state) in Window::ALL.into_iter().zip(&self.windows) {
            if state.window != expected
                || state.period.is_some_and(|period| {
                    period
                        .checked_add(1)
                        .and_then(|next| next.checked_mul(RENEW_INTERVAL_SECS))
                        .is_none()
                })
                || state
                    .pending
                    .as_ref()
                    .is_some_and(|pending| state.period.is_none() || pending.digest == [0; 32])
                || state.last_success.as_ref().is_some_and(|last| {
                    last.period
                        .checked_add(1)
                        .and_then(|next| next.checked_mul(RENEW_INTERVAL_SECS))
                        .is_none()
                        || state.period.is_none_or(|period| last.period > period)
                })
            {
                return Err(StoreError::InvalidState);
            }
        }
        self.cache.validate().map_err(|_| StoreError::InvalidState)
    }
}

/// Errors deliberately omit private file contents and parser input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    Io(std::io::ErrorKind),
    AlreadyExists,
    Busy,
    UnsafePath,
    UnsafePermissions,
    TooLarge,
    Corrupt,
    UnsupportedVersion,
    WrongProduct,
    WrongIdentity,
    InvalidState,
    UnsupportedPlatform,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "statistics store error: {self:?}")
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.kind())
    }
}

// This type is private: only the protected on-disk envelope may contain a seed.
// The byte arrays, serialized output and read input are zeroized on drop.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct PrivateSeed([u8; 32]);

impl Drop for PrivateSeed {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format_version: u32,
    product_domain: String,
    public_key: StatsPublicKey,
    seed: PrivateSeed,
    state: StoredState,
}

pub struct FileStore {
    directory: platform::Directory,
    // Keep the advisory writer lock alive for the entire handle lifetime.
    lock: File,
    identity: StatisticsIdentity,
}

impl fmt::Debug for FileStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileStore")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl FileStore {
    /// Creates only a new directory. The parent must already exist. A failure
    /// never overwrites an existing store or silently replaces its identity.
    pub fn create(
        path: impl AsRef<Path>,
        identity: StatisticsIdentity,
    ) -> Result<Self, StoreError> {
        let directory = platform::Directory::create(path.as_ref())?;
        let lock = directory.lock(true)?;
        let store = Self {
            directory,
            lock,
            identity,
        };
        store.write(&StoredState::default(), true)?;
        Ok(store)
    }

    /// Opens an existing private store, and requires the caller's expected
    /// public identity. Corruption is an error; no new key is generated.
    pub fn open(path: impl AsRef<Path>, expected: StatsPublicKey) -> Result<Self, StoreError> {
        let directory = platform::Directory::open(path.as_ref())?;
        let lock = directory.lock(false)?;
        let envelope = read_envelope(&directory)?;
        validate_envelope(&envelope, expected)?;
        let identity = StatisticsIdentity::from_seed(envelope.seed.0);
        Ok(Self {
            directory,
            lock,
            identity,
        })
    }

    pub fn identity(&self) -> &StatisticsIdentity {
        &self.identity
    }

    pub fn load(&self) -> Result<StoredState, StoreError> {
        self.directory.verify_lock(&self.lock)?;
        let envelope = read_envelope(&self.directory)?;
        validate_envelope(&envelope, self.identity.public_key())?;
        Ok(envelope.state)
    }

    pub fn save(&self, state: &StoredState) -> Result<(), StoreError> {
        // Validate the old envelope too. A removed or corrupted store cannot be
        // repaired by overwriting it with the currently held in-memory key.
        self.load()?;
        self.write(state, false)
    }

    fn write(&self, state: &StoredState, new: bool) -> Result<(), StoreError> {
        state.validate()?;
        self.directory.verify_lock(&self.lock)?;
        let envelope = Envelope {
            format_version: STORE_FORMAT_VERSION,
            product_domain: IDENTITY_DOMAIN.to_owned(),
            public_key: self.identity.public_key(),
            seed: PrivateSeed(*self.identity.private_seed()),
            state: state.clone(),
        };
        let bytes =
            Zeroizing::new(serde_json::to_vec(&envelope).map_err(|_| StoreError::InvalidState)?);
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(StoreError::TooLarge);
        }
        self.directory.atomic_write(&bytes, new)
    }
}

fn read_envelope(directory: &platform::Directory) -> Result<Envelope, StoreError> {
    let mut file = directory.data_file()?;
    if file.metadata()?.len() > MAX_STORE_BYTES {
        return Err(StoreError::TooLarge);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    Read::by_ref(&mut file)
        .take(MAX_STORE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err(StoreError::TooLarge);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)
}

fn validate_envelope(envelope: &Envelope, expected: StatsPublicKey) -> Result<(), StoreError> {
    if envelope.format_version != STORE_FORMAT_VERSION {
        return Err(StoreError::UnsupportedVersion);
    }
    if envelope.product_domain != IDENTITY_DOMAIN {
        return Err(StoreError::WrongProduct);
    }
    if envelope.public_key != expected
        || StatisticsIdentity::from_seed(envelope.seed.0).public_key() != expected
    {
        return Err(StoreError::WrongIdentity);
    }
    envelope.state.validate()
}

fn temporary_name() -> Result<String, StoreError> {
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| StoreError::Io(std::io::ErrorKind::Other))?;
    let suffix: String = random
        .into_iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(format!(".state-{suffix}.tmp"))
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use rustix::fs::{self, AtFlags, Mode, OFlags};
    use std::ffi::OsString;
    use std::os::unix::fs::MetadataExt;
    use std::path::Component;

    pub(super) struct Directory(File);

    fn errno(error: rustix::io::Errno) -> StoreError {
        std::io::Error::from_raw_os_error(error.raw_os_error()).into()
    }

    fn flags() -> OFlags {
        OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK
    }

    fn check_file(file: &File) -> Result<(), StoreError> {
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Err(StoreError::UnsafePath);
        }
        if metadata.mode() & 0o7777 != 0o600
            || metadata.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(StoreError::UnsafePermissions);
        }
        Ok(())
    }

    fn parent(path: &Path) -> Result<(File, OsString), StoreError> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut parts = Vec::new();
        for component in absolute.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => parts.push(name.to_owned()),
                _ => return Err(StoreError::UnsafePath),
            }
        }
        let name = parts.pop().ok_or(StoreError::UnsafePath)?;
        let mut directory = File::from(
            fs::open(
                "/",
                flags() | OFlags::DIRECTORY | OFlags::RDONLY,
                Mode::empty(),
            )
            .map_err(errno)?,
        );
        for component in parts {
            directory = File::from(
                fs::openat(
                    &directory,
                    component,
                    flags() | OFlags::DIRECTORY | OFlags::RDONLY,
                    Mode::empty(),
                )
                .map_err(errno)?,
            );
        }
        Ok((directory, name))
    }

    impl Directory {
        pub(super) fn create(path: &Path) -> Result<Self, StoreError> {
            let (parent, name) = parent(path)?;
            fs::mkdirat(&parent, &name, Mode::from_raw_mode(0o700)).map_err(|error| {
                if error == rustix::io::Errno::EXIST {
                    StoreError::AlreadyExists
                } else {
                    errno(error)
                }
            })?;
            parent.sync_all()?;
            Self::from_file(File::from(
                fs::openat(
                    &parent,
                    name,
                    flags() | OFlags::DIRECTORY | OFlags::RDONLY,
                    Mode::empty(),
                )
                .map_err(errno)?,
            ))
        }

        pub(super) fn open(path: &Path) -> Result<Self, StoreError> {
            let (parent, name) = parent(path)?;
            Self::from_file(File::from(
                fs::openat(
                    &parent,
                    name,
                    flags() | OFlags::DIRECTORY | OFlags::RDONLY,
                    Mode::empty(),
                )
                .map_err(errno)?,
            ))
        }

        fn from_file(file: File) -> Result<Self, StoreError> {
            let metadata = file.metadata()?;
            if !metadata.is_dir() {
                return Err(StoreError::UnsafePath);
            }
            if metadata.mode() & 0o7777 != 0o700
                || metadata.uid() != rustix::process::geteuid().as_raw()
            {
                return Err(StoreError::UnsafePermissions);
            }
            Ok(Self(file))
        }

        pub(super) fn lock(&self, create: bool) -> Result<File, StoreError> {
            let create = if create {
                OFlags::CREATE | OFlags::EXCL
            } else {
                OFlags::empty()
            };
            let file = File::from(
                fs::openat(
                    &self.0,
                    LOCK_FILE,
                    flags() | OFlags::RDWR | create,
                    Mode::from_raw_mode(0o600),
                )
                .map_err(errno)?,
            );
            check_file(&file)?;
            FileExt::try_lock_exclusive(&file).map_err(|error| {
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    StoreError::Busy
                } else {
                    error.into()
                }
            })?;
            self.verify_lock(&file)?;
            Ok(file)
        }

        pub(super) fn verify_lock(&self, lock: &File) -> Result<(), StoreError> {
            Self::from_file(self.0.try_clone()?)?;
            check_file(lock)?;
            let current = File::from(
                fs::openat(&self.0, LOCK_FILE, flags() | OFlags::RDONLY, Mode::empty())
                    .map_err(errno)?,
            );
            check_file(&current)?;
            let left = current.metadata()?;
            let right = lock.metadata()?;
            if (left.dev(), left.ino()) != (right.dev(), right.ino()) {
                return Err(StoreError::UnsafePath);
            }
            Ok(())
        }

        pub(super) fn data_file(&self) -> Result<File, StoreError> {
            let file = File::from(
                fs::openat(&self.0, DATA_FILE, flags() | OFlags::RDONLY, Mode::empty())
                    .map_err(errno)?,
            );
            check_file(&file)?;
            Ok(file)
        }

        pub(super) fn atomic_write(&self, bytes: &[u8], new: bool) -> Result<(), StoreError> {
            let name = temporary_name()?;
            let mut temporary = File::from(
                fs::openat(
                    &self.0,
                    &name,
                    flags() | OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
                    Mode::from_raw_mode(0o600),
                )
                .map_err(errno)?,
            );
            let result = (|| {
                check_file(&temporary)?;
                temporary.write_all(bytes)?;
                temporary.sync_all()?;
                if new {
                    fs::renameat_with(
                        &self.0,
                        &name,
                        &self.0,
                        DATA_FILE,
                        fs::RenameFlags::NOREPLACE,
                    )
                    .map_err(errno)?;
                } else {
                    self.data_file()?;
                    fs::renameat(&self.0, &name, &self.0, DATA_FILE).map_err(errno)?;
                }
                self.0.sync_all()?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::unlinkat(&self.0, name, AtFlags::empty());
            }
            result
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Component, PathBuf};

    // Refuse reparse points rather than following a junction/symlink. Windows
    // inherits the caller's ACL; this backend makes no POSIX permission claim.
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const REPARSE_POINT: u32 = 0x0000_0400;
    pub(super) struct Directory(PathBuf);

    fn check_path(path: &Path) -> Result<(), StoreError> {
        let mut prefix = PathBuf::new();
        for component in path.components() {
            if component == Component::ParentDir {
                return Err(StoreError::UnsafePath);
            }
            prefix.push(component.as_os_str());
            let metadata = fs::symlink_metadata(&prefix)?;
            if metadata.file_attributes() & REPARSE_POINT != 0 {
                return Err(StoreError::UnsafePath);
            }
        }
        Ok(())
    }

    fn check_file(file: &File) -> Result<(), StoreError> {
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & REPARSE_POINT != 0 {
            return Err(StoreError::UnsafePath);
        }
        Ok(())
    }

    impl Directory {
        pub(super) fn create(path: &Path) -> Result<Self, StoreError> {
            let path = std::path::absolute(path)?;
            let parent = path.parent().ok_or(StoreError::UnsafePath)?;
            check_path(parent)?;
            fs::create_dir(&path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::AlreadyExists
                } else {
                    error.into()
                }
            })?;
            Self::open(&path)
        }

        pub(super) fn open(path: &Path) -> Result<Self, StoreError> {
            let path = std::path::absolute(path)?;
            check_path(&path)?;
            if !fs::symlink_metadata(&path)?.is_dir() {
                return Err(StoreError::UnsafePath);
            }
            Ok(Self(path))
        }

        pub(super) fn lock(&self, create: bool) -> Result<File, StoreError> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(create)
                .custom_flags(OPEN_REPARSE_POINT)
                .open(self.0.join(LOCK_FILE))?;
            check_file(&file)?;
            FileExt::try_lock_exclusive(&file).map_err(|error| {
                if matches!(error.kind(), std::io::ErrorKind::WouldBlock)
                    || error.raw_os_error() == Some(33)
                {
                    StoreError::Busy
                } else {
                    error.into()
                }
            })?;
            self.verify_lock(&file)?;
            Ok(file)
        }

        pub(super) fn verify_lock(&self, lock: &File) -> Result<(), StoreError> {
            check_path(&self.0)?;
            check_path(&self.0.join(LOCK_FILE))?;
            check_file(lock)
        }

        pub(super) fn data_file(&self) -> Result<File, StoreError> {
            check_path(&self.0)?;
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(OPEN_REPARSE_POINT)
                .open(self.0.join(DATA_FILE))?;
            check_file(&file)?;
            Ok(file)
        }

        pub(super) fn atomic_write(&self, bytes: &[u8], new: bool) -> Result<(), StoreError> {
            check_path(&self.0)?;
            if !new {
                self.data_file()?;
            }
            let path = self.0.join(temporary_name()?);
            let result = (|| {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .custom_flags(OPEN_REPARSE_POINT)
                    .open(&path)?;
                check_file(&file)?;
                file.write_all(bytes)?;
                file.sync_all()?;
                drop(file);
                // std's Windows rename replaces the destination atomically.
                // Directory fsync and POSIX ownership checks are unavailable.
                if new && self.0.join(DATA_FILE).try_exists()? {
                    return Err(StoreError::AlreadyExists);
                }
                fs::rename(&path, self.0.join(DATA_FILE))?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(path);
            }
            result
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::*;
    pub(super) struct Directory;
    impl Directory {
        pub(super) fn create(_: &Path) -> Result<Self, StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
        pub(super) fn open(_: &Path) -> Result<Self, StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
        pub(super) fn lock(&self, _: bool) -> Result<File, StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
        pub(super) fn verify_lock(&self, _: &File) -> Result<(), StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
        pub(super) fn data_file(&self) -> Result<File, StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
        pub(super) fn atomic_write(&self, _: &[u8], _: bool) -> Result<(), StoreError> {
            Err(StoreError::UnsupportedPlatform)
        }
    }
}
