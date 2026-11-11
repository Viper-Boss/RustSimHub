// SPDX-License-Identifier: AGPL-3.0-only
//! In-process evidence from actual executor syscalls, never package JSON.
use super::*;
use std::{cell::RefCell, collections::BTreeMap};

/// Metadata projection only. A projection cannot construct OwnedRootChanges.
#[derive(Clone, Debug)]
pub struct OwnedNodeFact {
    pub path: String,
    pub before: Option<Stamp>,
    pub after: Option<Stamp>,
    pub sha256: Option<String>,
    pub directory: bool,
    pub temporary: bool,
}
struct Node {
    fact: OwnedNodeFact,
    held: Option<File>,
}
#[derive(Clone, Debug)]
pub struct OwnedDirectoryVersion {
    pub path: String,
    pub stamp: Stamp,
}
#[derive(Default)]
pub(super) struct Recorder(
    RefCell<BTreeMap<String, Node>>,
    RefCell<BTreeMap<String, (Stamp, File)>>,
);
impl Recorder {
    fn put(&self, fact: OwnedNodeFact, held: Option<File>) -> Result<()> {
        let mut nodes = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        if nodes.len() >= 1024 && !nodes.contains_key(&fact.path) {
            return Err(Error::Invalid);
        }
        let mut fact = fact;
        if let Some(old) = nodes.get(&fact.path) {
            fact.before = old.fact.before.clone();
        }
        nodes.insert(fact.path.clone(), Node { fact, held });
        Ok(())
    }
}
impl Root {
    pub(super) fn record_file(
        &self,
        path: &str,
        before: Option<&Stamp>,
        file: &FileSnapshot,
        temporary: bool,
    ) -> Result<()> {
        self.owned.put(
            OwnedNodeFact {
                path: path.into(),
                before: before.cloned(),
                after: Some(file.stamp.clone()),
                sha256: Some(super::super::material_sha256(&file.bytes)),
                directory: false,
                temporary,
            },
            Some(file.file.try_clone().map_err(|_| Error::Io)?),
        )?;
        self.record_parent(path)
    }
    pub(super) fn record_directory(&self, path: &str, file: &File) -> Result<()> {
        let m = file.metadata().map_err(|_| Error::Io)?;
        if !m.is_dir() {
            return Err(Error::Conflict);
        }
        self.owned.put(
            OwnedNodeFact {
                path: path.into(),
                before: None,
                after: Some(m.into()),
                sha256: None,
                directory: true,
                temporary: false,
            },
            Some(file.try_clone().map_err(|_| Error::Io)?),
        )?;
        self.record_parent(path)
    }
    pub(super) fn record_removed(
        &self,
        path: &str,
        before: Option<&Stamp>,
        temporary: bool,
    ) -> Result<()> {
        if self.read(path, MAX_FILE)?.is_some() {
            return Err(Error::Conflict);
        }
        self.owned.put(
            OwnedNodeFact {
                path: path.into(),
                before: before.cloned(),
                after: None,
                sha256: None,
                directory: false,
                temporary,
            },
            None,
        )?;
        self.record_parent(path)
    }
    fn record_parent(&self, path: &str) -> Result<()> {
        let (file, _, parent) = self.parent(path)?;
        let stamp = file.metadata().map_err(|_| Error::Io)?.into();
        self.owned
            .1
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?
            .insert(parent, (stamp, file));
        Ok(())
    }
    pub(crate) fn owned_parent_versions(&self) -> Result<Vec<OwnedDirectoryVersion>> {
        let parents = self.owned.1.try_borrow().map_err(|_| Error::Busy)?;
        let mut result = Vec::with_capacity(parents.len());
        for (path, (stamp, held)) in parents.iter() {
            let now = self.open_dir(path)?;
            if &Stamp::from(now.metadata().map_err(|_| Error::Io)?) != stamp
                || &Stamp::from(held.metadata().map_err(|_| Error::Io)?) != stamp
            {
                return Err(Error::Conflict);
            }
            result.push(OwnedDirectoryVersion {
                path: path.clone(),
                stamp: stamp.clone(),
            });
        }
        Ok(result)
    }
    /// Validate the retained descriptors and exact current nodes before lending
    /// facts. Directory timestamps change on child writes; identities and
    /// permissions stay pinned, with entry deltas checked by the live fence.
    pub(crate) fn owned_facts(&self) -> Result<Vec<OwnedNodeFact>> {
        self.check_root()?;
        let nodes = self.owned.0.try_borrow().map_err(|_| Error::Busy)?;
        let mut facts = Vec::with_capacity(nodes.len());
        for (path, node) in nodes.iter() {
            match &node.fact.after {
                None => {
                    if self.read(path, MAX_FILE)?.is_some() {
                        return Err(Error::Conflict);
                    }
                }
                Some(expected) if node.fact.directory => {
                    let (parent, name, _) = self.parent(path)?;
                    let named = File::from(
                        rustix::fs::openat(
                            &parent,
                            &name,
                            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                            Mode::empty(),
                        )
                        .map_err(|_| Error::Conflict)?,
                    );
                    let now: Stamp = named.metadata().map_err(|_| Error::Io)?.into();
                    let held: Stamp = node
                        .held
                        .as_ref()
                        .ok_or(Error::Conflict)?
                        .metadata()
                        .map_err(|_| Error::Io)?
                        .into();
                    if !expected.identity(&now)
                        || !expected.identity(&held)
                        || now.mode != expected.mode
                        || now.uid != expected.uid
                        || now.gid != expected.gid
                    {
                        return Err(Error::Conflict);
                    }
                }
                Some(expected) => {
                    let current = self.read(path, MAX_FILE)?.ok_or(Error::Conflict)?;
                    let held: Stamp = node
                        .held
                        .as_ref()
                        .ok_or(Error::Conflict)?
                        .metadata()
                        .map_err(|_| Error::Io)?
                        .into();
                    if &current.stamp != expected
                        || &held != expected
                        || Some(super::super::material_sha256(&current.bytes)) != node.fact.sha256
                    {
                        return Err(Error::Conflict);
                    }
                }
            }
            facts.push(node.fact.clone());
        }
        Ok(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Root) {
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let root = Root::isolated(d.path()).unwrap();
        (d, root)
    }
    #[test]
    fn evidence_tracks_actual_inodes_and_refuses_an_equal_byte_substitution() {
        let (d, root) = fixture();
        root.ensure_dir("/etc/example", 0o755).unwrap();
        root.replace("/etc/example/config", None, b"first", 0o600)
            .unwrap();
        let old = root.read("/etc/example/config", 100).unwrap().unwrap();
        let new = root
            .replace("/etc/example/config", Some(&old), b"second", 0o600)
            .unwrap();
        let facts = root.owned_facts().unwrap();
        let fact = facts
            .iter()
            .find(|f| f.path == "/etc/example/config")
            .unwrap();
        assert!(fact.before.is_none());
        assert_eq!(fact.after.as_ref().unwrap().inode, new.stamp.inode);
        let next = d.path().join("etc/example/other");
        std::fs::write(&next, b"second").unwrap();
        std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::rename(next, d.path().join("etc/example/config")).unwrap();
        assert_eq!(root.owned_facts().err(), Some(Error::Conflict));
    }
    #[test]
    fn rejected_intent_cleanup_records_only_the_exact_staged_file_as_removed() {
        let (_d, root) = fixture();
        root.ensure_dir("/etc/example", 0o755).unwrap();
        assert_eq!(
            root.replace_with_intent(
                "/etc/example/config",
                None,
                b"never published",
                0o600,
                |_| Err(Error::Denied)
            )
            .err(),
            Some(Error::Denied)
        );
        let facts = root.owned_facts().unwrap();
        assert!(facts.iter().any(|f| f.temporary && f.after.is_none()));
        assert!(root.read("/etc/example/config", 100).unwrap().is_none());
    }
}
