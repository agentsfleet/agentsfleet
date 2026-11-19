//! The ordered restore writer fetches at most four chunks per batch and
//! refuses a missing chunk or an overlay removal instead of writing past it.

use crate::recovery::window::{self, Chunk};
use crate::test_support;
use af_crypto::secret::SecretBytes;
use af_wire::task::{CheckpointEntry, CheckpointFile};
use std::{collections::BTreeMap, path::Path, sync::Arc};

/// What the downloader was asked for, and whether it drops the last chunk.
#[derive(Default)]
struct Fetcher {
    asked: Vec<Vec<String>>,
    short: bool,
}

fn owner() -> (u32, u32) {
    (
        rustix::process::getuid().as_raw(),
        rustix::process::getgid().as_raw(),
    )
}

/// The chunk id `index`; each chunk's bytes are its own id.
fn chunk_id(index: usize) -> String {
    format!("019f0000-0000-7000-8000-{:012}", index + 300)
}

async fn apply(
    root: &Path,
    entries: BTreeMap<String, CheckpointEntry>,
    fetcher: &mut Fetcher,
) -> crate::Result<()> {
    window::apply(
        fetcher,
        (root.to_owned(), owner()),
        entries,
        None,
        |fetcher, ids| {
            Box::pin(async move {
                fetcher.asked.push(ids.to_vec());
                let answered = if fetcher.short {
                    ids.len().saturating_sub(1)
                } else {
                    ids.len()
                };
                Ok(ids
                    .iter()
                    .take(answered)
                    .map(|id| -> Chunk { Arc::new(SecretBytes::new(id.as_bytes().to_vec())) })
                    .collect())
            })
        },
    )
    .await
}

fn file(path: &str, chunks: usize) -> BTreeMap<String, CheckpointEntry> {
    let objects: Vec<_> = (0..chunks).map(chunk_id).collect();
    BTreeMap::from([(
        path.to_owned(),
        CheckpointEntry::File(CheckpointFile {
            path: path.to_owned(),
            executable: false,
            size: objects.iter().map(|id| crate::byte_count(id.len())).sum(),
            objects,
        }),
    )])
}

#[tokio::test]
async fn a_file_of_five_chunks_is_fetched_four_then_one_in_order() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let mut fetcher = Fetcher::default();

    apply(root.path(), file("big", 5), &mut fetcher).await?;

    let ids: Vec<_> = (0..5).map(chunk_id).collect();
    assert_eq!(
        fetcher.asked,
        vec![(0..4).map(chunk_id).collect::<Vec<_>>(), vec![chunk_id(4)]],
        "the fifth chunk waits for the next batch"
    );
    assert_eq!(
        std::fs::read(root.path().join("big"))?,
        ids.concat().into_bytes()
    );
    Ok(())
}

#[tokio::test]
async fn a_downloader_that_omits_an_authorized_chunk_is_refused() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let mut fetcher = Fetcher {
        short: true,
        ..Fetcher::default()
    };

    let refused = apply(root.path(), file("pair", 2), &mut fetcher).await;

    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint writer omitted an authorized chunk")
    );
    assert_eq!(fetcher.asked, vec![vec![chunk_id(0), chunk_id(1)]]);
    Ok(())
}

#[tokio::test]
async fn an_overlay_removal_is_never_applied_to_a_restore() -> crate::Result<()> {
    let root = tempfile::tempdir()?;
    let mut fetcher = Fetcher::default();
    let entries = BTreeMap::from([(
        "gone".to_owned(),
        CheckpointEntry::Remove {
            path: "gone".to_owned(),
        },
    )]);

    let refused = apply(root.path(), entries, &mut fetcher).await;

    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint deletion survived overlay expansion")
    );
    assert_eq!(
        fetcher.asked,
        vec![Vec::<String>::new()],
        "no chunk is fetched"
    );
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}
