//! A turn's checkpoint as its restore reads it: the chain's manifests and
//! its own, each checked against its root before any of it is applied, then
//! each object under the turn's restore pin.

use super::session::TurnSession;
use crate::recovery::source::{Custody, links_inside};
use crate::{Result, error};
use af_chunk_store::{READ_WINDOW, Resolved};
use af_wire::{
    durable_task::{CheckpointEntry, CheckpointLink, CheckpointTransferPurpose},
    run::{CheckpointOperationId, TurnCheckpointPageRead},
};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use garde::Validate as _;
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroizing;

/// Why a window whose authorization answered no object for a chunk is refused.
const OMITTED: &str = "turn download omitted a file chunk";

/// A turn's restore custody over the manifests its saved descriptor names.
pub(super) struct TurnReading<'s, 'a, 'native> {
    /// The turn's restore pin and the calls made under it.
    pub(super) session: &'s mut TurnSession<'a, 'native>,
    /// Every generation the saved one's pages are a delta through.
    pub(super) chain: &'s [CheckpointLink],
    /// The saved generation's own pages.
    pub(super) pages: &'s [CheckpointOperationId],
    /// The root the saved generation's pages hash to.
    pub(super) digest: &'s str,
}

impl Custody for TurnReading<'_, '_, '_> {
    /// The tree the chain's manifests, then the saved pages, resolve to:
    /// its full manifest first, each delta over the one before. Only the
    /// first may not remove. Custody is proved once more after, so a tree
    /// with no chunk to read lands only under a live pin.
    async fn tree(&mut self) -> Result<Resolved> {
        let mut tree = Resolved::default();
        let links = self.chain.iter().map(|link| {
            let ids = link.page_ids.iter().cloned().map(CheckpointOperationId);
            (ids.collect::<Vec<_>>(), link.manifest_root.as_str())
        });
        let last = std::iter::once((self.pages.to_vec(), self.digest));
        for (position, (ids, root)) in links
            .collect::<Vec<_>>()
            .into_iter()
            .chain(last)
            .enumerate()
        {
            for entry in self.session.manifest(&ids, root).await? {
                if position == 0 && matches!(entry, CheckpointEntry::Remove { .. }) {
                    return Err(error::config("turn manifest is not a full snapshot"));
                }
                tree.apply(entry)?;
            }
        }
        complete(&tree)?;
        links_inside(&tree)?;
        self.session.renew().await?;
        Ok(tree)
    }

    /// Every GET of the window starts under the one freshly proved pin its
    /// authorization renewed, and custody is proved once more after. A
    /// repeated object is authorized and fetched once.
    async fn read(&mut self, objects: &[String]) -> Result<Vec<Zeroizing<Vec<u8>>>> {
        let unique: BTreeSet<&String> = objects.iter().collect();
        let unique: Vec<String> = unique.into_iter().cloned().collect();
        let frames = self
            .session
            .authorize(&unique, CheckpointTransferPurpose::Restore)
            .await?;
        let (transfers, workspace) = (&self.session.transfers, &self.session.workspace);
        let chunks: BTreeMap<String, Zeroizing<Vec<u8>>> = stream::iter(frames)
            .map(async |mut object| {
                let bytes = transfers.download(&object).await?;
                let plaintext = af_chunk_store::open(workspace, &mut object, bytes)?;
                let plaintext = Zeroizing::new(plaintext.expose().to_vec());
                Ok::<_, crate::Error>((object.object_id.clone(), plaintext))
            })
            .buffered(READ_WINDOW)
            .try_collect()
            .await?;
        self.session.renew().await?;
        placed(objects, chunks)
    }
}

/// Each of `ids`' chunks in their order. A chunk is moved to the last place
/// that names it and copied only to an earlier one, so a window is never
/// held twice.
fn placed(
    ids: &[String],
    mut chunks: BTreeMap<String, Zeroizing<Vec<u8>>>,
) -> Result<Vec<Zeroizing<Vec<u8>>>> {
    let mut read = Vec::with_capacity(ids.len());
    for (position, id) in ids.iter().enumerate() {
        let again = ids.iter().skip(position + 1).any(|later| later == id);
        let chunk = chunks.get(id).filter(|_| again).cloned();
        let chunk = chunk.or_else(|| chunks.remove(id));
        read.push(chunk.ok_or_else(|| error::config(OMITTED))?);
    }
    Ok(read)
}

impl TurnSession<'_, '_> {
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
fn complete(tree: &Resolved) -> Result<()> {
    let entries = tree.entries();
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
