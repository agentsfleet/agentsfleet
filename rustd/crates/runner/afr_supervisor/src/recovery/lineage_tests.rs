//! A save over a lineage writes only what changed, until its chain would
//! outgrow a full manifest; resolving what it wrote is the tree a full scan
//! lists.

use super::Lineage;
use crate::recovery::catalog::Catalog;
use crate::recovery::tree::{ChangeSource, Tree};
use crate::recovery::{Object, uploads};
use af_chunk_store::{Change, Resolved};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    CheckpointCipher, CheckpointEntry, CheckpointObject, CheckpointObjectStatus,
    CheckpointPlaintext,
};
use futures_util::{StreamExt as _, stream::BoxStream};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub(super) type Checked<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// What a pin that read only the changes since `parent` yields: a removal for
/// each of `removed`, then the tree of what changed, every chunk counted.
struct Delta {
    removed: Vec<String>,
    changed: Tree,
    parent: Option<Uuid7>,
    read: AtomicU64,
}

impl Delta {
    fn over(changed: &Path, removed: &[&str], parent: Option<&Uuid7>) -> Self {
        Self {
            removed: removed.iter().map(|path| (*path).to_owned()).collect(),
            changed: Tree::new(changed.to_owned(), CancellationToken::new()),
            parent: parent.cloned(),
            read: AtomicU64::new(0),
        }
    }
}

impl ChangeSource for Delta {
    fn changes(&self) -> BoxStream<'_, crate::Result<Change>> {
        let removed = self.removed.iter().map(|path| {
            Ok(Change::Entry(CheckpointEntry::Remove {
                path: path.clone(),
            }))
        });
        let counted = self.changed.changes().inspect(|change| {
            if let Ok(Change::Chunk { bytes, .. }) = change {
                self.read
                    .fetch_add(crate::byte_count(bytes.len()), Ordering::SeqCst);
            }
        });
        futures_util::stream::iter(removed).chain(counted).boxed()
    }

    fn parent(&self) -> Option<&Uuid7> {
        self.parent.as_ref()
    }
}

pub(super) fn id() -> Checked<Uuid7> {
    Ok(af_crypto::entropy::Entropy::new().uuid7(af_core::clock::now())?)
}

/// `count` files of `bytes` bytes each, named so they sort in creation order.
pub(super) fn files(root: &Path, count: usize, bytes: usize) -> Checked {
    for index in 0..count {
        fs::write(root.join(format!("file-{index:04}")), vec![b'a'; bytes])?;
    }
    Ok(())
}

pub(super) async fn catalog(
    source: &dyn ChangeSource,
    lineage: Option<&Lineage>,
    ceiling: usize,
) -> Checked<Catalog> {
    let history = Zeroizing::new(Vec::new());
    let stop = CancellationToken::new();
    Ok(Catalog::read(Some(source), history, (Vec::new(), lineage), ceiling, &stop).await?)
}

/// One object per distinct chunk across `catalogs`, as the daemon answers.
pub(super) fn objects(catalogs: &[&Catalog]) -> Checked<BTreeMap<String, String>> {
    let keys: BTreeSet<_> = catalogs
        .iter()
        .flat_map(|catalog| catalog.chunks.keys().cloned())
        .collect();
    keys.into_iter()
        .map(|key| Ok((key, id()?.to_string())))
        .collect()
}

/// The tree `manifests` resolve to, applied in order.
pub(super) fn resolve(manifests: &[&Catalog], ids: &BTreeMap<String, String>) -> Checked<Resolved> {
    let mut tree = Resolved::default();
    for catalog in manifests {
        for page in catalog.pages(Some(ids), af_wire::run::TURN_CHECKPOINT_PAGE_BYTES)? {
            for entry in page.entries {
                tree.apply(entry)?;
            }
        }
    }
    Ok(tree)
}

#[tokio::test]
async fn a_turn_save_writes_only_changed_and_removed_entries() -> Checked {
    let (root, changed) = (tempfile::tempdir()?, tempfile::tempdir()?);
    files(root.path(), 1_000, 16)?;
    let ceiling = af_wire::run::TURN_CHECKPOINT_PAGE_BYTES;
    let base = catalog(
        &Tree::new(root.path().to_owned(), CancellationToken::new()),
        None,
        ceiling,
    )
    .await?;
    let head = id()?;
    let lineage = base.lineage(head.clone());
    for name in ["file-0007", "file-0500"] {
        fs::write(root.path().join(name), b"rewritten")?;
        fs::write(changed.path().join(name), b"rewritten")?;
    }
    fs::remove_file(root.path().join("file-0999"))?;
    let delta = catalog(
        &Delta::over(changed.path(), &["file-0999"], Some(&head)),
        Some(&lineage),
        ceiling,
    )
    .await?;
    let kinds: Vec<_> = delta
        .entries
        .values()
        .map(|entry| match entry {
            CheckpointEntry::File(_) => "file",
            CheckpointEntry::Remove { .. } => "remove",
            CheckpointEntry::Directory { .. } | CheckpointEntry::Symlink { .. } => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["file", "file", "remove"],
        "two files and one removal"
    );
    assert_eq!(
        delta.parent.as_ref(),
        Some(&head),
        "the delta names its base"
    );
    assert_eq!(delta.entry_count, 999, "the plan counts the resolved tree");
    let scan = catalog(
        &Tree::new(root.path().to_owned(), CancellationToken::new()),
        None,
        ceiling,
    )
    .await?;
    let ids = objects(&[&base, &delta, &scan])?;
    assert_eq!(
        resolve(&[&base, &delta], &ids)?,
        resolve(&[&scan], &ids)?,
        "the chain resolves to a full scan's manifest"
    );
    Ok(())
}

/// A page ceiling under which twenty one-byte files lay out as three pages
/// and one changed file as one.
const SMALL_PAGES: usize = 1_100;

#[tokio::test]
async fn a_chain_whose_deltas_outgrow_a_full_manifest_writes_a_full_one() -> Checked {
    let (root, changed) = (tempfile::tempdir()?, tempfile::tempdir()?);
    files(root.path(), 20, 1)?;
    let full = catalog(
        &Tree::new(root.path().to_owned(), CancellationToken::new()),
        None,
        SMALL_PAGES,
    )
    .await?;
    assert_eq!(
        full.pages(None, SMALL_PAGES)?.len(),
        3,
        "a 3-page full manifest"
    );
    let mut lineage = full.lineage(id()?);
    for save in 1..=4_usize {
        fs::write(
            changed.path().join("file-0003"),
            [b'0' + u8::try_from(save)?],
        )?;
        let parent = lineage.head().clone();
        let next = catalog(
            &Delta::over(changed.path(), &[], Some(&parent)),
            Some(&lineage),
            SMALL_PAGES,
        )
        .await?;
        if save <= 3 {
            assert_eq!(
                next.parent.as_ref(),
                Some(&parent),
                "a chain at {save} pages stays a delta"
            );
            assert_eq!(next.pages(None, SMALL_PAGES)?.len(), 1);
        } else {
            assert_eq!(
                next.parent, None,
                "reaching 4 delta pages writes a full manifest"
            );
            assert_eq!(
                next.entries.len(),
                20,
                "the full manifest lists the whole tree"
            );
            assert_eq!(next.chunks.len(), 2, "it registers every chunk it names");
        }
        lineage = next.lineage(id()?);
    }
    Ok(())
}

/// The daemon end of a save that holds bytes for every chunk `known` names.
#[derive(Default)]
struct Bucket {
    known: BTreeSet<String>,
    uploaded: usize,
}

impl uploads::Publisher for Bucket {
    fn register(
        &mut self,
        descriptors: Vec<CheckpointPlaintext>,
    ) -> impl Future<Output = crate::Result<Vec<Object>>> {
        std::future::ready(self.objects(descriptors))
    }

    fn registered(&mut self, _objects: &[Object], _total: usize) -> impl Future<Output = ()> {
        std::future::ready(())
    }

    fn upload_batch(
        &mut self,
        pending: Vec<(Object, Zeroizing<Vec<u8>>)>,
    ) -> impl Future<Output = crate::Result<Vec<String>>> {
        self.uploaded += pending.len();
        let accepted = pending
            .into_iter()
            .map(|(object, _)| object.object_id.clone());
        std::future::ready(Ok(accepted.collect()))
    }
}

impl Bucket {
    /// One object per descriptor: reused when the bucket holds its bytes.
    fn objects(&self, descriptors: Vec<CheckpointPlaintext>) -> crate::Result<Vec<Object>> {
        descriptors
            .into_iter()
            .map(|plaintext| {
                let status = if self.known.contains(&plaintext.sha256) {
                    CheckpointObjectStatus::Reused
                } else {
                    CheckpointObjectStatus::Missing
                };
                let object_id = af_crypto::entropy::Entropy::new().uuid7(af_core::clock::now())?;
                Ok(Object::from(CheckpointObject {
                    object_id: object_id.to_string(),
                    repository: String::new(),
                    plaintext,
                    status,
                    cipher: CheckpointCipher::Plain,
                    nonce: None,
                    data_key: None,
                    ciphertext_sha256: None,
                    ciphertext_length: None,
                    transfer: None,
                }))
            })
            .collect()
    }
}

#[tokio::test]
async fn a_save_without_its_cache_reads_everything_once() -> Checked {
    let (root, changed) = (tempfile::tempdir()?, tempfile::tempdir()?);
    for index in 0..50_u8 {
        fs::write(root.path().join(format!("file-{index:04}")), [index; 64])?;
    }
    let ceiling = af_wire::run::TURN_CHECKPOINT_PAGE_BYTES;
    let base = catalog(&Delta::over(root.path(), &[], None), None, ceiling).await?;
    let head = id()?;
    let lineage = base.lineage(head.clone());
    fs::write(root.path().join("file-0003"), b"rewritten")?;
    fs::write(changed.path().join("file-0003"), b"rewritten")?;
    let warm = catalog(
        &Delta::over(changed.path(), &[], Some(&head)),
        Some(&lineage),
        ceiling,
    )
    .await?;
    let cold_source = Delta::over(root.path(), &[], None);
    let cold = catalog(&cold_source, Some(&lineage), ceiling).await?;
    assert_eq!(cold.parent, None, "a cacheless save writes a full manifest");
    assert_eq!(
        cold_source.read.load(Ordering::SeqCst),
        49 * 64 + 9,
        "it reads every file once"
    );
    let ids = objects(&[&base, &warm, &cold])?;
    assert_eq!(resolve(&[&base, &warm], &ids)?, resolve(&[&cold], &ids)?);
    let mut bucket = Bucket {
        known: base
            .chunks
            .values()
            .map(|chunk| chunk.descriptor.sha256.clone())
            .collect(),
        ..Bucket::default()
    };
    uploads::publish(&mut bucket, &cold, Some(&cold_source)).await?;
    assert_eq!(bucket.uploaded, 1, "its unchanged chunks upload nothing");
    Ok(())
}
