//! Reuse descriptor-relative file restoration, normalized ownership and authenticated chunks.

use super::super::{
    links, transfer,
    window::{self, Chunk},
};
use super::session::TurnSession;
use crate::{Result, error};
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointResponse as Response,
    },
    task::{CheckpointEntry, CheckpointTransferPurpose},
};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use garde::Validate as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

#[derive(garde::Validate)]
struct Bounds {
    #[garde(range(max = 25_769_803_776))]
    bytes: u64,
    #[garde(range(max = 200_000))]
    entries: usize,
}

impl TurnSession<'_, '_> {
    pub(super) async fn restore_files(
        &mut self,
        pages: &[CheckpointOperationId],
        digest: &str,
        destination: (PathBuf, (u32, u32)),
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<()> {
        let entries = self.manifest(pages, digest).await?;
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

    async fn manifest(
        &mut self,
        pages: &[CheckpointOperationId],
        digest: &str,
    ) -> Result<BTreeMap<String, CheckpointEntry>> {
        if pages.is_empty() {
            return Err(error::config("file recovery has no complete manifest"));
        }
        let mut digests = Vec::with_capacity(pages.len());
        let mut entries = BTreeMap::new();
        let mut previous = String::new();
        let mut bytes = 0_u64;
        for (number, id) in pages.iter().enumerate() {
            self.renew().await?;
            let Response::PageRead(page) = self
                .native
                .command(Action::PageRead {
                    checkpoint: self.reference.clone(),
                    page_id: id.clone(),
                })
                .await?
            else {
                return Err(error::config(
                    "turn page read returned another command's response",
                ));
            };
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
            append(&mut entries, &mut previous, &mut bytes, page.entries)?;
            Bounds {
                bytes,
                entries: entries.len(),
            }
            .validate()?;
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
                let plaintext = transfer::open(workspace, &mut object, bytes)?;
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

fn append(
    entries: &mut BTreeMap<String, CheckpointEntry>,
    previous: &mut String,
    bytes: &mut u64,
    page: Vec<CheckpointEntry>,
) -> Result<()> {
    for entry in page {
        if entry.path() <= previous.as_str() {
            return Err(error::config("turn manifest paths repeat or change order"));
        }
        entry.path().clone_into(previous);
        if let Some((parent, _)) = entry.path().rsplit_once('/')
            && !matches!(entries.get(parent), Some(CheckpointEntry::Directory { .. }))
        {
            return Err(error::config("turn manifest omitted a parent directory"));
        }
        if let CheckpointEntry::File(file) = &entry {
            *bytes = bytes
                .checked_add(file.size)
                .ok_or_else(|| error::config("turn expanded size overflow"))?;
        }
        if matches!(entry, CheckpointEntry::Remove { .. }) {
            return Err(error::config("turn manifest is not a full snapshot"));
        }
        entries.insert(entry.path().to_owned(), entry);
    }
    Ok(())
}
