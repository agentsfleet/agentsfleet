//! The fake sandbox quiesced: its files as they stood, read back as the
//! changes a publication takes after tenant code ran on.

use std::collections::BTreeMap;

use af_chunk_store::{Change, ChunkKey};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use futures_util::stream::{self, BoxStream, StreamExt as _};
use zeroize::Zeroizing;

use crate::engine::volume::{PausedWork, Quiesced};
use crate::error::Result;

/// The files a fake sandbox held when it was quiesced.
#[derive(Debug)]
pub(super) struct FakeQuiesced(pub(super) BTreeMap<String, Vec<u8>>);

#[async_trait::async_trait]
impl Quiesced for FakeQuiesced {
    /// Each file's entry, then its bytes as one chunk.
    fn changes(&self) -> BoxStream<'_, Result<Change>> {
        let changes = self.0.iter().flat_map(|(path, bytes)| {
            let file = CheckpointFile {
                path: path.clone(),
                executable: false,
                size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                objects: Vec::new(),
            };
            let key = ChunkKey::File {
                path: path.clone(),
                index: 0,
            };
            let bytes = Zeroizing::new(bytes.clone());
            [
                Ok(Change::Entry(CheckpointEntry::File(file))),
                Ok(Change::Chunk { key, bytes }),
            ]
        });
        stream::iter(changes).boxed()
    }

    fn parent(&self) -> Option<&Uuid7> {
        None
    }

    fn paused_work(&self) -> PausedWork {
        PausedWork::default()
    }

    async fn release(self: Box<Self>, _committed: Option<&Uuid7>) -> Result<()> {
        Ok(())
    }
}

/// The files `changes` hold, each chunk appended to the file it names.
pub(super) async fn files(
    mut changes: BoxStream<'_, Result<Change>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    while let Some(change) = changes.next().await {
        if let Change::Chunk {
            key: ChunkKey::File { path, .. },
            bytes,
        } = change?
        {
            files
                .entry(path)
                .or_insert_with(Vec::new)
                .extend_from_slice(&bytes);
        }
    }
    Ok(files)
}
