//! A restore's reads, a window of them in flight at once: four together and
//! never five, each file's chunks still landing in its order, and the first
//! failure ending the restore before a later window is read.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use af_chunk_store::{ChunkSource, Resolved, Stored};
use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use zeroize::Zeroizing;

use super::guest::{Guest, Node};
use super::{file, ours};
use crate::engine::{SnapshotFormat, SnapshotRef, fresh_sandbox_id};
use crate::file_snapshots::restore_workspace;

/// A source of one tree whose every object holds its own name, counting the
/// reads in flight together.
#[derive(Debug, Default)]
struct Counting {
    tree: Resolved,
    flying: AtomicUsize,
    /// How many reads were in flight as each began, in the order they began.
    began: Mutex<Vec<(String, usize)>>,
    /// The objects whose read fails, and the code each fails under.
    failing: Vec<(&'static str, ErrorCode)>,
}

impl Counting {
    /// A source of a tree holding `files`, each its path and how many
    /// objects it names, beside a directory and a link.
    fn of(files: &[(&str, usize)]) -> Self {
        let mut tree = Resolved::default();
        let (dir, link) = ("dir".to_owned(), "link".to_owned());
        tree.apply(CheckpointEntry::Directory { path: dir })
            .unwrap();
        for (path, count) in files {
            let objects: Vec<String> = (0..*count).map(|at| format!("{path}{at};")).collect();
            tree.apply(CheckpointEntry::File(CheckpointFile {
                path: (*path).to_owned(),
                executable: false,
                size: objects.iter().map(|object| object.len() as u64).sum(),
                objects,
            }))
            .unwrap();
        }
        let target = "a".to_owned();
        tree.apply(CheckpointEntry::Symlink { path: link, target })
            .unwrap();
        Self {
            tree,
            ..Self::default()
        }
    }

    /// Each read's object and how many were in flight as it began.
    fn began(&self) -> Vec<(String, usize)> {
        self.began
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl ChunkSource for Counting {
    async fn tree(&self, _id: &Uuid7) -> af_chunk_store::Result<Stored> {
        Ok(Stored {
            generation: 1,
            tree: self.tree.clone(),
        })
    }

    /// Yields once before it answers, so every read its window started with
    /// it is in flight before it ends.
    async fn read(&self, object: &str) -> af_chunk_store::Result<Zeroizing<Vec<u8>>> {
        let flying = self.flying.fetch_add(1, Ordering::SeqCst) + 1;
        self.began
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((object.to_owned(), flying));
        tokio::task::yield_now().await;
        self.flying.fetch_sub(1, Ordering::SeqCst);
        let failed = self.failing.iter().find(|(failing, _)| *failing == object);
        failed.map_or_else(
            || Ok(Zeroizing::new(object.as_bytes().to_vec())),
            |(_, code)| Err(af_chunk_store::unavailable(*code)),
        )
    }
}

/// A source whose every window answers one chunk fewer than it was asked.
#[derive(Debug)]
struct Short(Counting);

#[async_trait::async_trait]
impl ChunkSource for Short {
    async fn tree(&self, id: &Uuid7) -> af_chunk_store::Result<Stored> {
        self.0.tree(id).await
    }

    async fn read(&self, object: &str) -> af_chunk_store::Result<Zeroizing<Vec<u8>>> {
        self.0.read(object).await
    }

    async fn read_window(
        &self,
        objects: &[String],
    ) -> af_chunk_store::Result<Vec<Zeroizing<Vec<u8>>>> {
        let mut read = self.0.read_window(objects).await?;
        read.pop();
        Ok(read)
    }
}

/// A ref to the tree `Counting` holds, taken where the test's sandbox runs.
fn snapshot() -> SnapshotRef {
    SnapshotRef::new(fresh_sandbox_id(), ours(), 1, SnapshotFormat::Files)
}

/// Ten objects across three files are read as windows of four, four and
/// two: four in flight together and never five, a window spanning files,
/// and every file landing in manifest order with its chunks in theirs.
#[tokio::test]
async fn test_a_restore_reads_four_objects_together_and_never_five() {
    let source = Counting::of(&[("a", 3), ("b", 6), ("c", 1), ("empty", 0)]);
    let spools = tempfile::tempdir().unwrap();
    let guest = Guest::default();

    restore_workspace(&guest, spools.path(), &snapshot(), &source)
        .await
        .unwrap();

    let began = source.began();
    let objects: Vec<&str> = began.iter().map(|(object, _)| object.as_str()).collect();
    let flying: Vec<usize> = began.iter().map(|(_, flying)| *flying).collect();
    assert_eq!(
        objects,
        [
            "a0;", "a1;", "a2;", "b0;", "b1;", "b2;", "b3;", "b4;", "b5;", "c0;"
        ]
    );
    assert_eq!(flying, [1, 2, 3, 4, 1, 2, 3, 4, 1, 2]);
    let held = guest.held();
    assert_eq!(held.imported, ["a", "b", "c", "dir", "empty", "link"]);
    assert_eq!(held.nodes.get("a"), Some(&file(b"a0;a1;a2;", false)));
    assert_eq!(
        held.nodes.get("b"),
        Some(&file(b"b0;b1;b2;b3;b4;b5;", false))
    );
    assert_eq!(held.nodes.get("c"), Some(&file(b"c0;", false)));
    assert_eq!(held.nodes.get("empty"), Some(&file(b"", false)));
    assert_eq!(held.nodes.get("dir"), Some(&Node::Directory));
    assert_eq!(held.nodes.get("link"), Some(&Node::Symlink("a".to_owned())));
}

/// Two reads of the second window fail: the restore answers the first of
/// them, no later window is read, and nothing lands.
#[tokio::test]
async fn test_the_first_failed_read_ends_the_restore_before_a_later_window() {
    let source = Counting {
        failing: vec![
            ("b1;", error_code::RUN_CHECKPOINT_UNAVAILABLE),
            ("b3;", error_code::RUN_CHECKPOINT_CONFLICT),
            ("c0;", error_code::RUN_CHECKPOINT_LIMIT),
        ],
        ..Counting::of(&[("a", 3), ("b", 6), ("c", 1)])
    };
    let spools = tempfile::tempdir().unwrap();
    let guest = Guest::default();

    let failed = restore_workspace(&guest, spools.path(), &snapshot(), &source)
        .await
        .unwrap_err();

    assert_eq!(failed.code(), error_code::RUN_CHECKPOINT_UNAVAILABLE);
    let began = source.began();
    let last = began.last().map(|(object, _)| object.as_str());
    assert_eq!((began.len(), last), (8, Some("b4;")));
    let held = guest.held();
    assert_eq!((held.nodes.len(), held.imported.len()), (0, 0));
}

/// A window answered with fewer chunks than it named is refused as invalid
/// data, so no chunk lands in another's place.
#[tokio::test]
async fn test_a_window_answered_short_is_refused() {
    let source = Short(Counting::of(&[("a", 3)]));
    let spools = tempfile::tempdir().unwrap();
    let guest = Guest::default();

    let refused = restore_workspace(&guest, spools.path(), &snapshot(), &source)
        .await
        .unwrap_err();

    let cause = std::error::Error::source(&refused)
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .map(io::Error::kind);
    assert_eq!(cause, Some(io::ErrorKind::InvalidData));
    assert_eq!(source.0.began().len(), 3);
    assert_eq!(guest.held().nodes.len(), 0);
}
