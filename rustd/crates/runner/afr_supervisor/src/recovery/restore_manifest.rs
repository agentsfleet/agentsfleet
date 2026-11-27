//! Authenticate the base, every chain link and the plan's own manifest
//! before restoring any file.

use super::Session;
use crate::{Result, error};
use af_chunk_store::Resolved;
use af_core::id::Uuid7;
use af_wire::durable_task::{CHECKPOINT_PAGE_BYTES, CheckpointEntry, CheckpointReadQuery};
use std::collections::BTreeMap;

impl Session<'_> {
    /// The tree the plan resolves to: its retained base, then every link of
    /// its chain, then its own pages, each checked against its root first;
    /// with the plan's own history objects and the base's paths.
    pub(super) async fn manifest(
        &mut self,
    ) -> Result<(BTreeMap<String, CheckpointEntry>, Vec<String>, Vec<String>)> {
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
        let entries = entries.into_entries();
        entries
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

    pub(super) async fn pages(
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
