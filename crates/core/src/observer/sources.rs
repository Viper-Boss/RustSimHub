// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
pub(super) const MAX_OBJECTS: usize = 4096;
pub(super) const BATCH: usize = 16;
pub(super) struct Sources {
    nodes: HashMap<String, (u64, Option<[u8; 32]>)>,
    next: u64,
    cursor: Option<String>,
    paths: Vec<OwnedObjectPath>,
    refreshed: Option<Instant>,
    restore_pending: bool,
    restore_nodes: bool,
    retired_before_restore: HashSet<String>,
}
impl Default for Sources {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            next: 0,
            cursor: None,
            paths: Vec::new(),
            refreshed: None,
            restore_pending: true,
            restore_nodes: true,
            retired_before_restore: HashSet::new(),
        }
    }
}
impl Sources {
    pub fn needs_restore(&self) -> bool {
        self.restore_pending
    }
    pub fn restore(&mut self, snapshot: rustsimhub_storage::SourceSnapshot) -> Result<()> {
        if !self.restore_pending || snapshot.sources.len() > MAX_OBJECTS {
            return Err("source restore denied".into());
        }
        self.next = self.next.max(snapshot.maximum_incarnation);
        for row in snapshot.sources {
            let (path, number) = row
                .source
                .rsplit_once('#')
                .ok_or("invalid persisted source")?;
            let number = number.parse::<u64>()?;
            if number == 0
                || number > self.next
                || path.len() > 224
                || !path.starts_with("/org/freedesktop/ModemManager1/SMS/")
            {
                return Err("invalid persisted source".into());
            }
            if self.restore_nodes && !self.retired_before_restore.contains(path) {
                self.nodes
                    .insert(path.to_owned(), (number, Some(row.content_hash)));
            }
        }
        self.restore_pending = false;
        self.retired_before_restore.clear();
        Ok(())
    }
    pub fn reset_uncertain(&mut self) {
        self.reset();
        self.restore_nodes = false;
    }
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.cursor = None;
        self.paths.clear();
        self.refreshed = None;
    }
    pub fn retire(&mut self, path: &str) {
        if self.restore_pending {
            if self.retired_before_restore.len() < MAX_OBJECTS {
                self.retired_before_restore.insert(path.to_owned());
            } else {
                self.restore_nodes = false;
            }
        }
        self.nodes.remove(path);
        self.refreshed = None;
    }
    pub fn incomplete(&mut self, path: &str) {
        // A complete object returning to an incomplete state may have been reused.
        if self.nodes.get(path).is_some_and(|(_, hash)| hash.is_some()) {
            self.retire(path);
        }
    }
    pub fn refresh(&mut self, mut paths: Vec<OwnedObjectPath>) -> Result<()> {
        if paths.len() > MAX_OBJECTS {
            return Err("object limit".into());
        }
        paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        paths.dedup();
        if paths.iter().any(|p| {
            p.as_str().len() > 224
                || !p
                    .as_str()
                    .starts_with("/org/freedesktop/ModemManager1/SMS/")
        }) {
            return Err("invalid SMS path".into());
        }
        let present: HashSet<&str> = paths.iter().map(|p| p.as_str()).collect();
        self.nodes.retain(|p, _| present.contains(p.as_str()));
        self.paths = paths;
        self.refreshed = Some(Instant::now());
        Ok(())
    }
    pub fn list_due(&self) -> bool {
        self.refreshed
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(10))
    }
    pub fn next_batch(&mut self) -> (Vec<OwnedObjectPath>, bool) {
        let paths = &self.paths;
        let start = self
            .cursor
            .as_ref()
            .map_or(0, |c| paths.partition_point(|p| p.as_str() <= c.as_str()));
        let start = if start >= paths.len() { 0 } else { start };
        let end = (start + BATCH).min(paths.len());
        let batch = paths[start..end].to_vec();
        let more = end < paths.len();
        self.cursor = if more {
            batch.last().map(ToString::to_string)
        } else {
            None
        };
        (batch, more)
    }
    #[cfg(test)]
    pub fn batch(&mut self, paths: Vec<OwnedObjectPath>) -> Result<(Vec<OwnedObjectPath>, bool)> {
        self.refresh(paths)?;
        Ok(self.next_batch())
    }
    pub fn bind(&mut self, sms: &mut IncomingSms) -> Result<()> {
        let hash: [u8; 32] = Sha256::digest(serde_json::to_vec(&(
            &sms.sender,
            &sms.timestamp,
            &sms.text,
            &sms.data,
        ))?)
        .into();
        let path = sms.source.clone();
        let existing = self.nodes.get(&path).copied();
        let incarnation = match existing {
            Some((i, Some(h))) if h == hash => i,
            _ => {
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or("source generation overflow")?;
                self.next
            }
        };
        if existing.is_none() && self.nodes.len() >= MAX_OBJECTS {
            return Err("source limit".into());
        }
        self.nodes.insert(path.clone(), (incarnation, Some(hash)));
        sms.source = format!("{path}#{incarnation}");
        Ok(())
    }
}
