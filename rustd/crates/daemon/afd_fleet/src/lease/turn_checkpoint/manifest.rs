//! A complete full manifest is verified before root and head publication.

use super::{TurnCheckpointRefusal, guard::TurnLeaseGuard, pages, row::Plan};
use crate::Result;
use af_wire::task::CheckpointEntry;
use std::collections::{BTreeMap, BTreeSet};

pub(super) async fn validate(
    owner: &mut TurnLeaseGuard<'_>,
    plan: &Plan,
    digest: &str,
) -> Result<()> {
    let mut digests = Vec::with_capacity(plan.page_ids.len());
    let mut contents = Contents::default();
    for (number, id) in plan.page_ids.iter().enumerate() {
        let saved = pages::selected(owner, plan, id).await?;
        let page = pages::decode(&saved)?;
        if usize::try_from(page.page_number).ok() != Some(number) {
            return Err(TurnCheckpointRefusal::Incompatible.into());
        }
        pages::validate(owner, plan, &page).await?;
        digests.push((
            id.to_string(),
            saved.digest.ok_or(TurnCheckpointRefusal::Incompatible)?,
        ));
        contents.append(&page.entries)?;
    }
    af_validate::checkpoint_links(
        contents
            .links
            .iter()
            .map(|(path, target)| (path.as_str(), target.as_str())),
    )
    .map_err(super::root::invalid("turn manifest link graph"))?;
    complete(plan, digest, &digests, contents.entries, contents.bytes)
}

#[derive(Default)]
struct Contents {
    directories: BTreeSet<String>,
    links: BTreeMap<String, String>,
    previous: String,
    entries: i32,
    bytes: u64,
}

impl Contents {
    fn append(&mut self, entries: &[CheckpointEntry]) -> Result<()> {
        for entry in entries {
            if entry.path() <= self.previous.as_str() {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
            if let Some((parent, _)) = entry.path().rsplit_once('/')
                && !self.directories.contains(parent)
            {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
            entry.path().clone_into(&mut self.previous);
            match entry {
                CheckpointEntry::Directory { path } => {
                    self.directories.insert(path.clone());
                }
                CheckpointEntry::File(file) => {
                    self.bytes = self
                        .bytes
                        .checked_add(file.size)
                        .ok_or(TurnCheckpointRefusal::Limit)?;
                }
                CheckpointEntry::Symlink { path, target } => {
                    self.links.insert(path.clone(), target.clone());
                }
                CheckpointEntry::Remove { .. } => {
                    return Err(TurnCheckpointRefusal::Incompatible.into());
                }
            }
            self.entries = self
                .entries
                .checked_add(1)
                .ok_or(TurnCheckpointRefusal::Limit)?;
        }
        Ok(())
    }
}

fn complete(
    plan: &Plan,
    digest: &str,
    digests: &[(String, String)],
    entries: i32,
    bytes: u64,
) -> Result<()> {
    let root = af_crypto::checkpoint_digest::manifest_root(
        digests
            .iter()
            .map(|(id, digest)| (id.as_str(), digest.as_str())),
    );
    if root != digest
        || entries != plan.entry_count
        || i64::try_from(bytes).ok() != Some(plan.logical_bytes)
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}
