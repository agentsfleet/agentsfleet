//! A turn's checkpoint as its restore reads it: the chain's manifests and
//! its own, each checked against its root before any of it is applied, then
//! each object under the turn's restore pin.

use super::session::TurnSession;
use crate::recovery::source::{Custody, links_inside};
use crate::{Result, error};
use af_chunk_store::Resolved;
use af_wire::{
    durable_task::{CheckpointEntry, CheckpointLink, CheckpointTransferPurpose},
    run::{CheckpointOperationId, TurnCheckpointPageRead},
};
use garde::Validate as _;
use zeroize::Zeroizing;

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

    async fn read(&mut self, object: &str) -> Result<Zeroizing<Vec<u8>>> {
        self.session.renew().await?;
        let mut objects = self
            .session
            .authorize(&[object.to_owned()], CheckpointTransferPurpose::Restore)
            .await?;
        let mut object = objects
            .pop()
            .ok_or_else(|| error::config("turn download omitted a file chunk"))?;
        let bytes = self.session.transfers.download(&object).await?;
        let plaintext = af_chunk_store::open(&self.session.workspace, &mut object, bytes)?;
        self.session.renew().await?;
        Ok(Zeroizing::new(plaintext.expose().to_vec()))
    }
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
