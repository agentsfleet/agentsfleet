//! What a catalog publishes: its ordered pages, with each chunk's object in
//! place, and its history's chunks and root.

use super::{Catalog, HistoryChunk, PLACEHOLDER};
use crate::{Result, error};
use af_wire::durable_task::{CheckpointEntry, CheckpointPage};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

impl Catalog {
    /// The history chunks, by key, with their bytes.
    pub(in crate::recovery) fn history(&self) -> Result<Vec<HistoryChunk<'_>>> {
        self.history_chunks
            .iter()
            .map(|key| {
                let (start, end) = self
                    .chunks
                    .get(key)
                    .and_then(|chunk| chunk.history)
                    .ok_or_else(|| error::config("checkpoint history source is absent"))?;
                let bytes = self
                    .history
                    .get(start..end)
                    .ok_or_else(|| error::config("checkpoint history chunk range is invalid"))?;
                Ok((key.as_str(), Zeroizing::new(bytes.to_vec())))
            })
            .collect()
    }

    pub(in crate::recovery) fn history_root(
        &self,
        ids: &BTreeMap<String, String>,
    ) -> Result<String> {
        let mut history = Vec::new();
        for key in &self.history_chunks {
            let id = ids
                .get(key)
                .ok_or_else(|| error::config("checkpoint history registration is incomplete"))?;
            let chunk = self
                .chunks
                .get(key)
                .ok_or_else(|| error::config("checkpoint history source is absent"))?;
            history.push((
                id.as_str(),
                chunk.descriptor.sha256.as_str(),
                chunk.descriptor.size,
            ));
        }
        Ok(af_crypto::checkpoint_digest::history_root(history))
    }

    pub(in crate::recovery) fn pages(
        &self,
        ids: Option<&BTreeMap<String, String>>,
        ceiling: usize,
    ) -> Result<Vec<CheckpointPage>> {
        let mut pages = super::super::layout::Pages::new(ceiling)?;
        for (path, original) in &self.entries {
            pages.push(self.mapped_entry(path, original, ids)?)?;
        }
        let history = self
            .history_chunks
            .iter()
            .map(|key| {
                ids.map_or_else(
                    || Ok(PLACEHOLDER.to_owned()),
                    |ids| {
                        ids.get(key).cloned().ok_or_else(|| {
                            error::config("checkpoint history registration is incomplete")
                        })
                    },
                )
            })
            .collect::<Result<_>>()?;
        pages.finish(history)
    }

    fn mapped_entry(
        &self,
        path: &str,
        original: &CheckpointEntry,
        ids: Option<&BTreeMap<String, String>>,
    ) -> Result<CheckpointEntry> {
        let mut entry = original.clone();
        if let CheckpointEntry::File(file) = &mut entry {
            let keys = self
                .file_chunks
                .get(path)
                .ok_or_else(|| error::config("checkpoint file mapping is absent"))?;
            file.objects = match ids {
                None => vec![PLACEHOLDER.to_owned(); keys.len()],
                Some(ids) => keys
                    .iter()
                    .map(|key| {
                        ids.get(key).cloned().ok_or_else(|| {
                            error::config("checkpoint file registration is incomplete")
                        })
                    })
                    .collect::<Result<_>>()?,
            };
        }
        Ok(entry)
    }
}
