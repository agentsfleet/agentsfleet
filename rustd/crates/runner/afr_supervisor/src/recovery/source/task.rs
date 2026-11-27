//! A durable task's checkpoint as its restore reads it: the retained base,
//! every chain link and the plan's own pages, each checked against its root
//! before any of it is applied, then each object under the task's pin.

use af_chunk_store::Resolved;
use af_core::id::Uuid7;
use af_wire::durable_task::{CHECKPOINT_PAGE_BYTES, CheckpointEntry, CheckpointReadQuery};
use zeroize::Zeroizing;

use super::{Custody, links_inside};
use crate::recovery::Session;
use crate::{Result, error};

/// A durable task's restore custody, and what its tree's read found beside
/// the tree.
#[derive(Debug)]
pub(in crate::recovery) struct TaskReading<'s, 'a> {
    /// The task's restore pin and the calls made under it.
    pub(in crate::recovery) session: &'s mut Session<'a>,
    /// The plan's own history objects, once its tree is read.
    pub(in crate::recovery) history: Vec<String>,
    /// The retained base's paths, once its tree is read.
    pub(in crate::recovery) base_paths: Vec<String>,
}

impl<'s, 'a> TaskReading<'s, 'a> {
    /// A restore read under `session`'s pin, nothing read yet.
    pub(in crate::recovery) const fn new(session: &'s mut Session<'a>) -> Self {
        Self {
            session,
            history: Vec::new(),
            base_paths: Vec::new(),
        }
    }
}

impl Custody for TaskReading<'_, '_> {
    /// The plan's tree; custody is proved once more after, so a tree with no
    /// chunk to read lands only under a live pin.
    async fn tree(&mut self) -> Result<Resolved> {
        let (tree, history, base_paths) = self.session.manifest().await?;
        links_inside(&tree)?;
        self.session.renew().await?;
        self.history = history;
        self.base_paths = base_paths;
        Ok(tree)
    }

    /// Custody is proved once more after the window lands in memory, so the
    /// next step starts under a live pin. Each chunk is dropped as it is
    /// copied out, so the window is never held twice.
    async fn read(&mut self, objects: &[String]) -> Result<Vec<Zeroizing<Vec<u8>>>> {
        let chunks = self.session.download_batch(objects).await?;
        self.session.renew().await?;
        Ok(chunks
            .into_iter()
            .map(|chunk| Zeroizing::new(chunk.expose().to_vec()))
            .collect())
    }
}

impl Session<'_> {
    /// The tree the plan resolves to: its retained base, then every link of
    /// its chain, then its own pages, each checked against its root first;
    /// with the plan's own history objects and the base's paths.
    async fn manifest(&mut self) -> Result<(Resolved, Vec<String>, Vec<String>)> {
        let mut entries = Resolved::default();
        let mut bytes = 0_usize;
        let base = self.plan.base_page_ids.clone();
        if !base.is_empty() {
            let expected = self
                .plan
                .base_manifest_root
                .clone()
                .ok_or_else(|| error::config("checkpoint base root is absent"))?;
            let history = self
                .pages(&base, &expected, &mut entries, &mut bytes)
                .await?;
            if !history.is_empty() {
                return Err(error::config("retained base contains private history"));
            }
        }
        let base_paths = entries.entries().keys().cloned().collect();
        for link in self.plan.chain.clone() {
            // An ancestor's history is its own generation's, never this
            // one's; each manifest of the chain is held to the page bound on
            // its own, as its commit held it.
            let _history = self
                .pages(&link.page_ids, &link.manifest_root, &mut entries, &mut 0)
                .await?;
        }
        let ids = self.plan.page_ids.clone();
        let expected = self
            .plan
            .manifest_root
            .clone()
            .ok_or_else(|| error::config("checkpoint manifest is incomplete"))?;
        let history = self
            .pages(&ids, &expected, &mut entries, &mut bytes)
            .await?;
        entries
            .entries()
            .values()
            .try_fold(0_u64, |sum, entry| {
                sum.checked_add(match entry {
                    CheckpointEntry::File(file) => file.size,
                    CheckpointEntry::Directory { .. }
                    | CheckpointEntry::Symlink { .. }
                    | CheckpointEntry::Remove { .. } => 0,
                })
            })
            .ok_or_else(|| error::config("checkpoint expanded size overflow"))?;
        Ok((entries, history, base_paths))
    }

    pub(in crate::recovery) async fn pages(
        &mut self,
        ids: &[String],
        expected: &str,
        entries: &mut Resolved,
        bytes: &mut usize,
    ) -> Result<Vec<String>> {
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let mut digests = Vec::new();
        let mut history = Vec::new();
        let mut previous: Option<String> = None;
        for (position, id) in ids.iter().enumerate() {
            self.renew().await?;
            let page = self
                .plane
                .durable_task_checkpoint_page(
                    self.run,
                    &checkpoint,
                    &Uuid7::parse(id)?,
                    &CheckpointReadQuery {
                        fencing_token: self.fence,
                        pin_id: self.plan.pin.id.clone(),
                        starting_after: None,
                    },
                )
                .await?;
            garde::Validate::validate(&page)?;
            let encoded = serde_json::to_vec(&page).map_err(error::encode)?;
            *bytes = bytes
                .checked_add(encoded.len())
                .ok_or_else(|| error::config("checkpoint manifest size overflow"))?;
            if usize::try_from(page.page_number).unwrap_or(usize::MAX) != position
                || encoded.len() > CHECKPOINT_PAGE_BYTES
                || *bytes > 64 * CHECKPOINT_PAGE_BYTES
            {
                return Err(error::config(
                    "checkpoint manifest exceeds its ordered bounds",
                ));
            }
            digests.push((id.as_str(), af_core::etag::sha256_hex(&encoded)));
            for entry in page.entries {
                if previous.as_deref().is_some_and(|path| path >= entry.path()) {
                    return Err(error::config("checkpoint manifest paths are not ordered"));
                }
                previous = Some(entry.path().to_owned());
                entries.apply(entry)?;
            }
            history.extend(page.history_objects);
            if history.len() > 256 {
                return Err(error::config("checkpoint history object bound exceeded"));
            }
        }
        if af_crypto::checkpoint_digest::manifest_root(
            digests.iter().map(|(id, digest)| (*id, digest.as_str())),
        ) != expected
        {
            return Err(error::config("checkpoint manifest root mismatch"));
        }
        Ok(history)
    }
}
