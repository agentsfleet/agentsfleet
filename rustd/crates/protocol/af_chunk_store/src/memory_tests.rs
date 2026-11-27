#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the binaries"
)]

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use zeroize::Zeroizing;

use super::MemoryStore;
use crate::store::{ChunkSource as _, ChunkStore as _};
use crate::{Change, ChunkKey};

fn fresh() -> Uuid7 {
    Entropy::new().uuid7(af_core::clock::now()).unwrap()
}

fn file(path: &str, size: u64) -> Change {
    Change::Entry(CheckpointEntry::File(CheckpointFile {
        path: path.to_owned(),
        executable: false,
        size,
        objects: vec!["left over from a walk".to_owned()],
    }))
}

fn chunk(path: &str, index: u64, bytes: &[u8]) -> Change {
    Change::Chunk {
        key: ChunkKey::File {
            path: path.to_owned(),
            index,
        },
        bytes: Zeroizing::new(bytes.to_vec()),
    }
}

/// Publishes `changes` under a fresh name over `parent`; the name and the
/// generation committed.
async fn published(
    store: &MemoryStore,
    parent: Option<&Uuid7>,
    changes: Vec<Change>,
) -> (Uuid7, u64) {
    let id = fresh();
    let mut publication = store.publish(&id, parent).await.unwrap();
    for change in changes {
        publication.add(change).await.unwrap();
    }
    (id.clone(), publication.commit().await.unwrap())
}

/// A full tree then a delta over it each resolve to what they saved, each
/// file's objects read back as its chunks in order, and each read counted.
#[tokio::test]
async fn a_delta_publishes_over_its_parent_and_reads_back_its_chunks() {
    let store = MemoryStore::default();
    let directory = Change::Entry(CheckpointEntry::Directory {
        path: "src".to_owned(),
    });
    let (base, first) = published(
        &store,
        None,
        vec![
            directory,
            file("src/a", 4),
            chunk("src/a", 0, b"ab"),
            chunk("src/a", 1, b"cd"),
            file("gone", 0),
        ],
    )
    .await;
    let removed = Change::Entry(CheckpointEntry::Remove {
        path: "gone".to_owned(),
    });
    let (delta, second) = published(
        &store,
        Some(&base),
        vec![removed, file("b", 1), chunk("b", 0, b"e")],
    )
    .await;

    assert_eq!((first, second), (1, 2));
    let stored = store.tree(&delta).await.unwrap();
    assert_eq!(stored.generation, 2);
    let paths: Vec<&str> = stored.tree.entries().keys().map(String::as_str).collect();
    assert_eq!(paths, ["b", "src", "src/a"]);
    let Some(CheckpointEntry::File(saved)) = stored.tree.entries().get("src/a") else {
        unreachable!("src/a is a file")
    };
    let mut read = Vec::new();
    for object in &saved.objects {
        read.extend_from_slice(&store.read(object).await.unwrap());
    }
    assert_eq!(read, b"abcd");
    assert_eq!(store.reads(), 2);
    assert_eq!(store.tree(&base).await.unwrap().generation, 1);
}

/// A chunk out of order, for another file or with no file before it is
/// refused; so are a taken name and a parent the store does not name.
#[tokio::test]
async fn a_publication_refuses_what_it_cannot_place() {
    let store = MemoryStore::default();
    let (taken, _) = published(&store, None, Vec::new()).await;
    for misplaced in [
        vec![chunk("a", 0, b"x")],
        vec![file("a", 1), chunk("a", 1, b"x")],
        vec![file("a", 1), chunk("b", 0, b"x")],
    ] {
        let id = fresh();
        let mut publication = store.publish(&id, None).await.unwrap();
        let mut added = Ok(());
        for change in misplaced {
            added = added.and(publication.add(change).await);
        }
        assert!(added.is_err());
        drop(publication);
        assert!(store.tree(&id).await.is_err(), "nothing named uncommitted");
    }
    assert!(store.publish(&taken, None).await.is_err());
    assert!(store.publish(&fresh(), Some(&fresh())).await.is_err());
}

/// A forgotten name is no longer read, and a second forget, or a read of an
/// object never stored, is refused.
#[tokio::test]
async fn a_forgotten_tree_is_no_longer_named() {
    let store = MemoryStore::default();
    let (id, _) = published(&store, None, vec![file("a", 0)]).await;
    let kept = store.tree(&id).await.unwrap();
    let Some(CheckpointEntry::File(saved)) = kept.tree.entries().get("a") else {
        unreachable!("a is a file")
    };
    assert!(
        saved.objects.is_empty(),
        "a walk's objects are the store's to fill"
    );

    store.forget(&id).await.unwrap();

    store.tree(&id).await.unwrap_err();
    store.forget(&id).await.unwrap_err();
    store.read("never stored").await.unwrap_err();
    assert_eq!(store.reads(), 1);
}

/// A source another crate implements fails under its own failure's code,
/// and says only that it could not answer.
#[test]
fn a_source_that_could_not_answer_fails_under_its_own_code() {
    let failure = crate::unavailable(af_core::error_code::INTERNAL_OPERATION_FAILED);

    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(
        failure.to_string().contains("could not answer"),
        "{failure}"
    );
}
