//! Validate expanded base plus overlay, including links through other links.

use crate::{Result, durable_task::DurableTaskRefusal};
use af_wire::durable_task::{CheckpointEntry, CheckpointPage};
use std::collections::{BTreeMap, BTreeSet};

const MAX_LINKS: usize = 40;

struct Node {
    directory: bool,
    size: u64,
    target: Option<String>,
}

#[derive(Clone)]
struct Resolved {
    components: Vec<String>,
    substitutions: usize,
}

#[derive(Default)]
pub(super) struct Tree {
    nodes: BTreeMap<String, Node>,
}

impl Tree {
    pub(super) fn apply(&mut self, entry: CheckpointEntry) -> Result<()> {
        let (path, node) = match entry {
            CheckpointEntry::File(file) => (
                file.path,
                Node {
                    directory: false,
                    size: file.size,
                    target: None,
                },
            ),
            CheckpointEntry::Directory { path } => (
                path,
                Node {
                    directory: true,
                    size: 0,
                    target: None,
                },
            ),
            CheckpointEntry::Symlink { path, target } => (
                path,
                Node {
                    directory: false,
                    size: 0,
                    target: Some(target),
                },
            ),
            CheckpointEntry::Remove { path } => {
                self.nodes.remove(&path);
                let prefix = format!("{path}/");
                let descendants: Vec<_> = self
                    .nodes
                    .range(prefix.clone()..)
                    .take_while(|(path, _node)| path.starts_with(&prefix))
                    .map(|(path, _node)| path.clone())
                    .collect();
                for path in descendants {
                    self.nodes.remove(&path);
                }
                return Ok(());
            }
        };
        self.nodes.insert(path, node);
        // Bound expansion even before later pages can remove or replace entries.
        if self.nodes.len() > 400_000 {
            return Err(DurableTaskRefusal::LimitExhausted.into());
        }
        Ok(())
    }

    /// Applies one stored page of a generation the plan's pages are a delta
    /// through; each was checked whole when its generation committed, so only
    /// its bytes are checked again.
    pub(super) fn ancestor(&mut self, page: super::chain::ChainPage) -> Result<()> {
        let body = page
            .body
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
        if page.digest.as_deref() != Some(af_core::etag::sha256_hex(&body).as_str()) {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let contents: CheckpointPage =
            serde_json::from_slice(&body).map_err(af_core::json::Unreadable::from)?;
        contents
            .entries
            .into_iter()
            .try_for_each(|entry| self.apply(entry))
    }

    pub(super) fn verify(&self, expected_entries: u32, expected_bytes: u64) -> Result<()> {
        let bytes = self
            .nodes
            .values()
            .try_fold(0_u64, |sum, node| sum.checked_add(node.size))
            .ok_or(DurableTaskRefusal::LimitExhausted)?;
        if self.nodes.len() != usize::try_from(expected_entries).unwrap_or(usize::MAX)
            || bytes != expected_bytes
        {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let mut cache = BTreeMap::new();
        let mut visiting = BTreeSet::new();
        for (path, node) in &self.nodes {
            for (position, _separator) in path.match_indices('/') {
                if self
                    .nodes
                    .get(path.split_at(position).0)
                    .is_some_and(|parent| !parent.directory)
                {
                    return Err(DurableTaskRefusal::CheckpointIncompatible.into());
                }
            }
            if node.target.is_some() {
                self.resolve(path, &mut cache, &mut visiting, 0)?;
            }
        }
        Ok(())
    }

    fn resolve(
        &self,
        path: &str,
        cache: &mut BTreeMap<String, Resolved>,
        visiting: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<Resolved> {
        if let Some(resolved) = cache.get(path) {
            return Ok(resolved.clone());
        }
        if depth >= MAX_LINKS || !visiting.insert(path.to_owned()) {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let target = self
            .nodes
            .get(path)
            .and_then(|node| node.target.as_deref())
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
        let mut resolved: Vec<String> = path
            .rsplit_once('/')
            .map_or_default(|(parent, _name)| parent.split('/').map(str::to_owned).collect());
        let mut substitutions = 1_usize;
        for part in target.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    resolved
                        .pop()
                        .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
                }
                name => {
                    resolved.push(name.to_owned());
                    let current = resolved.join("/");
                    if self
                        .nodes
                        .get(&current)
                        .is_some_and(|node| node.target.is_some())
                    {
                        let link = self.resolve(&current, cache, visiting, depth + 1)?;
                        substitutions = substitutions
                            .checked_add(link.substitutions)
                            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
                        if substitutions > MAX_LINKS {
                            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
                        }
                        resolved = link.components;
                    }
                }
            }
        }
        visiting.remove(path);
        let resolved = Resolved {
            components: resolved,
            substitutions,
        };
        cache.insert(path.to_owned(), resolved.clone());
        Ok(resolved)
    }
}

#[cfg(test)]
#[path = "effective_tests.rs"]
mod tests;
