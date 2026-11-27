//! A saved tree landed from a source the sandbox's build was not handed:
//! through its snapshots where its engine offers them, and only into an
//! empty workspace where it offers none.

use std::io;
use std::sync::Arc;

use af_chunk_store::MemoryStore;

use super::guest::{Fault, Guest, Node};
use super::{file, ours, snapshots};
use crate::engine::{Environment, SnapshotFormat, SnapshotRef, fresh_sandbox_id};
use crate::error::SnapshotRefusal;
use crate::file_snapshots::restore_workspace;

/// A store holding one take of a guest's `notes`, and the ref it answered.
async fn saved() -> (Arc<MemoryStore>, SnapshotRef) {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store);
    let taken = Guest::default();
    taken.put("notes", file(b"saved", false));
    let snapshot = snapshots.take(&taken, fresh_sandbox_id()).await.unwrap();
    (store, snapshot)
}

/// A sandbox whose engine offers snapshots renews through them and lands the
/// tree the source holds, not the one its own build's store holds.
#[tokio::test]
async fn test_a_sandbox_with_snapshots_restores_from_the_source_it_is_handed() {
    let (source, snapshot) = saved().await;
    let own = Arc::new(MemoryStore::default());
    let (files, spools) = snapshots(&own);
    let guest = Guest {
        files: Some(files),
        ..Guest::default()
    };
    guest.put("stale", file(b"from before", false));

    restore_workspace(&guest, spools.path(), &snapshot, &*source)
        .await
        .unwrap();

    let held = guest.held();
    let paths: Vec<&str> = held.nodes.keys().map(String::as_str).collect();
    assert_eq!(paths, ["notes"]);
    assert_eq!(held.nodes.get("notes"), Some(&file(b"saved", false)));
    assert_eq!(held.renewed, 1);
    assert_eq!((own.reads(), source.reads()), (0, 1));
}

/// A sandbox with no snapshots cannot renew: the tree lands in its empty
/// workspace, and a workspace already holding an entry, or holding more than
/// its listing answers, is refused before any chunk is read.
#[tokio::test]
async fn test_a_sandbox_without_snapshots_lands_a_tree_only_in_an_empty_workspace() {
    let (source, snapshot) = saved().await;
    let spools = tempfile::tempdir().unwrap();
    let empty = Guest::default();
    restore_workspace(&empty, spools.path(), &snapshot, &*source)
        .await
        .unwrap();
    let held = Guest::default();
    held.put("keep", Node::Directory);
    let crowded = Guest {
        fault: Fault::Crowded,
        ..Guest::default()
    };
    let read = source.reads();

    let mut refusals = Vec::new();
    for occupied in [&held, &crowded] {
        let refused = restore_workspace(occupied, spools.path(), &snapshot, &*source)
            .await
            .unwrap_err();
        let cause = std::error::Error::source(&refused)
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .map(io::Error::kind);
        refusals.push(cause);
    }

    assert_eq!(
        empty.held().nodes.get("notes"),
        Some(&file(b"saved", false))
    );
    assert_eq!(empty.held().renewed, 0);
    assert_eq!(refusals, [Some(io::ErrorKind::DirectoryNotEmpty); 2]);
    assert_eq!(source.reads(), read);
    let paths: Vec<String> = held.held().nodes.keys().cloned().collect();
    assert_eq!(paths, ["keep"]);
}

/// A ref from another environment, or naming another generation than the
/// source holds, is refused by name before any chunk is read or the
/// workspace renewed, whether or not the sandbox offers snapshots.
#[tokio::test]
async fn test_a_crossed_or_stale_ref_is_refused_before_any_chunk_is_read() {
    let (source, snapshot) = saved().await;
    let elsewhere = Environment {
        guest_agent: Some("sha256-another".into()),
        ..ours()
    };
    let crossed = SnapshotRef::new(snapshot.id().clone(), elsewhere, 1, SnapshotFormat::Files);
    let stale = SnapshotRef::new(snapshot.id().clone(), ours(), 9, SnapshotFormat::Files);
    let (files, spools) = snapshots(&Arc::new(MemoryStore::default()));
    let offering = Guest {
        files: Some(files),
        ..Guest::default()
    };
    let bare = Guest::default();

    let mut refusals = Vec::new();
    for guest in [&offering, &bare] {
        for refused in [&crossed, &stale] {
            let failure = restore_workspace(guest, spools.path(), refused, &*source)
                .await
                .unwrap_err();
            refusals.push(failure.snapshot_refusal());
        }
    }

    let expected = [
        Some(SnapshotRefusal::Environment),
        Some(SnapshotRefusal::Generation),
    ];
    assert_eq!(refusals, [expected, expected].concat());
    assert_eq!(source.reads(), 0);
    assert_eq!((offering.held().renewed, bare.held().renewed), (0, 0));
}
