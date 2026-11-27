//! A complete manifest is verified before root and head publication: a full
//! one as it stands, a delta as the tree its chain resolves to.

use super::{TurnCheckpointRefusal, guard::TurnRunGuard, pages, row::Plan};
use crate::{Result, durable_task::checkpoint};
use af_chunk_store::Resolved;
use af_wire::durable_task::CheckpointEntry;
use std::collections::{BTreeMap, BTreeSet};

pub(super) async fn validate(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    digest: &str,
) -> Result<()> {
    let mut tree = ancestors(owner, plan).await?;
    let mut digests = Vec::with_capacity(plan.page_ids.len());
    let mut previous = String::new();
    for id in &plan.page_ids {
        let saved = pages::selected(owner, plan, id).await?;
        // `decode` holds the page number to its stored ordinal, which
        // `pages_allocate.sql` numbers from the plan's page order.
        let page = pages::decode(&saved)?;
        pages::validate(owner, plan, &page, pages::Stored::Verified).await?;
        digests.push((
            id.to_string(),
            saved.digest.ok_or(TurnCheckpointRefusal::Incompatible)?,
        ));
        for entry in page.entries {
            // Order runs across pages: no path at or before the last one.
            if entry.path() <= previous.as_str() {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
            entry.path().clone_into(&mut previous);
            apply(&mut tree, entry)?;
        }
    }
    let mut contents = Contents::default();
    let resolved: Vec<_> = tree.into_entries().into_values().collect();
    contents.append(&resolved)?;
    af_validate::checkpoint_links(
        contents
            .links
            .iter()
            .map(|(path, target)| (path.as_str(), target.as_str())),
    )
    .map_err(super::root::invalid("turn manifest link graph"))?;
    complete(plan, digest, &digests, contents.entries, contents.bytes)
}

/// The tree the generations `plan`'s pages are a delta through resolve to;
/// empty for a full manifest. Each page was checked when its generation
/// committed, so only its bytes are checked again here.
async fn ancestors(owner: &mut TurnRunGuard<'_>, plan: &Plan) -> Result<Resolved> {
    let mut tree = Resolved::default();
    if plan.parent_id.is_some() {
        checkpoint::chain::pages(&mut owner.tx, &plan.id, |page| {
            let page = pages::decode(&pages::Page {
                ordinal: page.page_number,
                digest: page.digest,
                body: page.body,
            })?;
            page.entries
                .into_iter()
                .try_for_each(|entry| apply(&mut tree, entry))
        })
        .await?;
    }
    Ok(tree)
}

/// Applies `entry` over `tree`, which holds no more entries than an expanded
/// tree may.
fn apply(tree: &mut Resolved, entry: CheckpointEntry) -> Result<()> {
    tree.apply(entry)
        .map_err(|_bound| TurnCheckpointRefusal::Limit.into())
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

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
