//! Reuse descriptor-relative file restoration, normalized ownership and authenticated chunks.

use super::super::{
    links,
    window::{self, Chunk},
};
use super::session::TurnSession;
use crate::{Result, error};
use af_chunk_store::Resolved;
use af_wire::{
    durable_task::{CheckpointEntry, CheckpointLink, CheckpointTransferPurpose},
    run::{CheckpointOperationId, TurnCheckpointPageRead},
};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use garde::Validate as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

impl TurnSession<'_, '_> {
    /// Restores the tree `chain`'s manifests, then `pages`, resolve to, each
    /// manifest checked against its root before any of it is applied.
    pub(super) async fn restore_files(
        &mut self,
        chain: &[CheckpointLink],
        pages: &[CheckpointOperationId],
        digest: &str,
        destination: (PathBuf, (u32, u32)),
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<()> {
        let entries = self.resolve(chain, pages, digest).await?;
        links::validate(entries.values().filter_map(|entry| match entry {
            CheckpointEntry::Symlink { path, target } => Some((path.as_str(), target.as_str())),
            _ => None,
        }))?;
        window::apply(self, destination, entries, permit, |session, ids| {
            Box::pin(async move {
                session.renew().await?;
                if ids.is_empty() {
                    return Ok(Vec::new());
                }
                let chunks = session.download(ids).await?;
                session.renew().await?;
                Ok(chunks)
            })
        })
        .await
    }

    /// The tree the chain resolves to: its full manifest first, each delta
    /// over the one before, `pages` last. Only the first may not remove.
    async fn resolve(
        &mut self,
        chain: &[CheckpointLink],
        pages: &[CheckpointOperationId],
        digest: &str,
    ) -> Result<BTreeMap<String, CheckpointEntry>> {
        let mut tree = Resolved::default();
        let links = chain
            .iter()
            .map(|link| {
                let ids = link.page_ids.iter().cloned().map(CheckpointOperationId);
                (ids.collect::<Vec<_>>(), link.manifest_root.as_str())
            })
            .collect::<Vec<_>>();
        let last = std::iter::once((pages.to_vec(), digest));
        for (position, (ids, root)) in links.into_iter().chain(last).enumerate() {
            for entry in self.manifest(&ids, root).await? {
                if position == 0 && matches!(entry, CheckpointEntry::Remove { .. }) {
                    return Err(error::config("turn manifest is not a full snapshot"));
                }
                tree.apply(entry)?;
            }
        }
        let entries = tree.into_entries();
        complete(&entries)?;
        Ok(entries)
    }

    /// One manifest's entries in order, checked against `digest`.
    async fn manifest(
        &mut self,
        pages: &[CheckpointOperationId],
        digest: &str,
    ) -> Result<Vec<CheckpointEntry>> {
        if pages.is_empty() {
            return Err(error::config("file recovery has no complete manifest"));
        }
        let mut digests = Vec::with_capacity(pages.len());
        let mut entries = Vec::new();
        let mut previous = String::new();
        for (number, id) in pages.iter().enumerate() {
            self.renew().await?;
            let page = self
                .native
                .command(TurnCheckpointPageRead {
                    checkpoint: self.reference.clone(),
                    page_id: id.clone(),
                })
                .await?;
            page.validate()?;
            if usize::try_from(page.page_number).ok() != Some(number)
                || !page.history_objects.is_empty()
                || !af_validate::serialized_within(&page, af_wire::run::TURN_CHECKPOINT_PAGE_BYTES)
            {
                return Err(error::config(
                    "turn manifest page has invalid layout or size",
                ));
            }
            digests.push((
                id.0.clone(),
                af_core::etag::sha256_hex(&serde_json::to_vec(&page).map_err(error::encode)?),
            ));
            ordered(&mut previous, &page.entries)?;
            entries.extend(page.entries);
        }
        if af_crypto::checkpoint_digest::manifest_root(
            digests
                .iter()
                .map(|(id, digest)| (id.as_str(), digest.as_str())),
        ) != digest
        {
            return Err(error::config(
                "turn manifest differs from the assigned root",
            ));
        }
        Ok(entries)
    }

    async fn download(&mut self, ids: &[String]) -> Result<Vec<Chunk>> {
        let unique: Vec<_> = ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let objects = self
            .authorize(&unique, CheckpointTransferPurpose::Restore)
            .await?;
        let transfers = &self.transfers;
        let workspace = &self.workspace;
        // Every GET in this bounded batch starts under the same freshly proved pin.
        let chunks: BTreeMap<_, _> = stream::iter(objects)
            .map(async |mut object| {
                let bytes = transfers.download(&object).await?;
                let plaintext = af_chunk_store::open(workspace, &mut object, bytes)?;
                Ok::<_, crate::Error>((object.object_id.clone(), Arc::new(plaintext)))
            })
            .buffered(4)
            .try_collect()
            .await?;
        ids.iter()
            .map(|id| {
                chunks
                    .get(id)
                    .cloned()
                    .ok_or_else(|| error::config("turn download omitted a file chunk"))
            })
            .collect()
    }
}

/// Holds one manifest's paths to strictly ascending order across its pages.
fn ordered(previous: &mut String, page: &[CheckpointEntry]) -> Result<()> {
    for entry in page {
        if entry.path() <= previous.as_str() {
            return Err(error::config("turn manifest paths repeat or change order"));
        }
        entry.path().clone_into(previous);
    }
    Ok(())
}

/// Holds the resolved tree whole: each path's parent a directory in it, and
/// its expanded size countable.
fn complete(entries: &BTreeMap<String, CheckpointEntry>) -> Result<()> {
    let mut bytes = 0_u64;
    for entry in entries.values() {
        if let Some((parent, _)) = entry.path().rsplit_once('/')
            && !matches!(entries.get(parent), Some(CheckpointEntry::Directory { .. }))
        {
            return Err(error::config("turn manifest omitted a parent directory"));
        }
        if let CheckpointEntry::File(file) = entry {
            bytes = bytes
                .checked_add(file.size)
                .ok_or_else(|| error::config("turn expanded size overflow"))?;
        }
    }
    Ok(())
}
