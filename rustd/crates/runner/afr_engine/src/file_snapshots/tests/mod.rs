#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::sync::Arc;

use af_chunk_store::{ChunkStore, MemoryStore};
use af_wire::runner::SandboxEngine;

use self::guest::{Guest, Node};
use super::FileSnapshots;
use super::volume::GuestVolume;
use crate::engine::volume::{WorkspaceVolume as _, pin_paused};
use crate::engine::{Environment, Root, SnapshotFormat, SnapshotRef, fresh_sandbox_id};
use crate::error::SnapshotRefusal;

mod guest;

/// The disk a test's sandbox was granted.
const DISK: u64 = 1 << 20;

/// What the test's sandbox runs on.
fn ours() -> Environment {
    Environment {
        engine: SandboxEngine::Firecracker,
        root: Root::Toolbox {
            digest: "sha256-toolbox".into(),
        },
        guest_agent: Some("sha256-guest-agent".into()),
    }
}

fn file(bytes: &[u8], executable: bool) -> Node {
    Node::File {
        bytes: bytes.to_vec(),
        executable,
    }
}

/// Snapshots over `store`, spooled under a fresh directory, held to `disk`.
fn snapshots(store: &Arc<MemoryStore>, disk: u64) -> (FileSnapshots, tempfile::TempDir) {
    let spools = tempfile::tempdir().unwrap();
    let store: Arc<dyn ChunkStore> = Arc::<MemoryStore>::clone(store);
    let snapshots = FileSnapshots::new(store, spools.path().to_owned(), disk);
    (snapshots, spools)
}

/// A take holds tenant code only for the pin and publishes every entry; a
/// restore renews the workspace and lands each saved name, a second name
/// as a file of its own, and nothing written after the take.
#[tokio::test]
async fn test_a_restore_lands_what_the_take_saved_and_nothing_after() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store, DISK);
    let guest = Guest::default();
    guest.put("src", Node::Directory);
    guest.put("src/run.sh", file(b"#!/bin/sh\n", true));
    guest.put("src/notes", file(b"notes", false));
    guest.put("src/again", Node::SecondName("src/notes".to_owned()));
    guest.put("latest", Node::Symlink("src/notes".to_owned()));
    let saved = guest.held().nodes.clone();

    let snapshot = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    guest.put("src/notes", file(b"changed", false));
    guest.put("after", file(b"later", false));
    snapshots.restore(&guest, &ours(), &snapshot).await.unwrap();

    let held = guest.held();
    let mut expected = saved;
    expected.insert("src/again".to_owned(), file(b"notes", false));
    assert_eq!(held.nodes, expected);
    assert_eq!((held.frozen, held.thawed, held.renewed), (1, 1, 1));
    assert_eq!(held.released, [Some(snapshot.id().to_string())]);
    assert_eq!(snapshot.environment(), &ours());
    assert_eq!(snapshot.generation(), 1);
    assert_eq!(snapshot.format(), SnapshotFormat::Files);
}

/// A second take is a delta on the first: it walks against the first's
/// generation, and its restore holds the change and none of the removed.
#[tokio::test]
async fn test_a_second_take_is_a_delta_on_the_first() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store, DISK);
    let guest = Guest::default();
    guest.put("kept", file(b"kept", false));
    guest.put("gone", file(b"gone", false));
    let first = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    guest.held().nodes.remove("gone");
    guest.put("new", file(b"new", false));

    let second = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    snapshots.restore(&guest, &ours(), &second).await.unwrap();

    let held = guest.held();
    assert_eq!(held.walked, [None, Some(first.id().to_string())]);
    let paths: Vec<&str> = held.nodes.keys().map(String::as_str).collect();
    assert_eq!(paths, ["kept", "new"]);
    assert_eq!(second.generation(), 2);
}

/// A ref from another environment, or naming another generation than the
/// store does, is refused by name before any chunk is read or the
/// workspace renewed; so is one the store never named.
#[tokio::test]
async fn test_a_crossed_ref_is_refused_before_any_chunk_is_read() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store, DISK);
    let guest = Guest::default();
    guest.put("a", file(b"a", false));
    let taken = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    let elsewhere = Environment {
        guest_agent: Some("sha256-another".into()),
        ..ours()
    };
    let crossed = SnapshotRef::new(taken.id().clone(), elsewhere, 1, SnapshotFormat::Files);
    let other_generation = SnapshotRef::new(taken.id().clone(), ours(), 9, SnapshotFormat::Files);
    let never = SnapshotRef::new(fresh_sandbox_id(), ours(), 1, SnapshotFormat::Files);

    let mut refusals = Vec::new();
    for snapshot in [&crossed, &other_generation, &never] {
        let refused = snapshots
            .restore(&guest, &ours(), snapshot)
            .await
            .unwrap_err();
        refusals.push(refused.snapshot_refusal());
    }

    assert_eq!(
        refusals,
        [
            Some(SnapshotRefusal::Environment),
            Some(SnapshotRefusal::Generation),
            None
        ]
    );
    assert_eq!(store.reads(), 0);
    assert_eq!(guest.held().renewed, 0);
}

/// A forgotten snapshot is refused, a second forget too, and a take after
/// the last one is forgotten walks the whole workspace.
#[tokio::test]
async fn test_a_forgotten_snapshot_is_refused_and_the_next_take_is_whole() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store, DISK);
    let guest = Guest::default();
    let taken = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    let kept = snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();
    snapshots.forget(&taken).await.unwrap();
    snapshots.forget(&kept).await.unwrap();

    let restored = snapshots.restore(&guest, &ours(), &kept).await;
    let again = snapshots.forget(&kept).await;
    snapshots
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap();

    assert!(restored.is_err() && again.is_err());
    assert_eq!(guest.held().walked.last(), Some(&None));
}

/// A pause the guest refuses, a pin streamed past the granted disk and a
/// publication the store refuses each fail the take, and name nothing; a
/// release the guest refuses after a commit costs only the next delta.
#[tokio::test]
async fn test_a_take_that_fails_names_nothing() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store, DISK);
    let frozen_out = Guest {
        freeze_fails: true,
        ..Guest::default()
    };
    snapshots
        .take(&frozen_out, &ours(), fresh_sandbox_id())
        .await
        .unwrap_err();

    let (small, _small_spools) = self::snapshots(&store, 16);
    let guest = Guest::default();
    guest.put("big", file(&[7; 4096], false));
    small
        .take(&guest, &ours(), fresh_sandbox_id())
        .await
        .unwrap_err();

    let id = fresh_sandbox_id();
    snapshots.take(&guest, &ours(), id.clone()).await.unwrap();
    snapshots.take(&guest, &ours(), id).await.unwrap_err();
    assert_eq!(guest.held().released.last(), Some(&None));

    let unreleased = Guest {
        release_fails: true,
        ..Guest::default()
    };
    let (fresh, _fresh_spools) = self::snapshots(&store, DISK);
    let held = fresh.take(&unreleased, &ours(), fresh_sandbox_id()).await;
    assert!(held.is_ok(), "{held:?}");
}

/// A pin states the work its pause did: the entries the walk read.
#[tokio::test]
async fn test_a_pin_states_the_work_its_pause_did() {
    let guest = Guest::default();
    guest.put("a", file(b"a", false));
    guest.put("b", Node::Directory);
    let spools = tempfile::tempdir().unwrap();
    let volume = GuestVolume {
        agent: &guest,
        spools: spools.path(),
        disk_bytes: DISK,
    };

    let pin = pin_paused(&volume, None).await.unwrap();
    let work = volume.paused_work(&pin);
    volume.release(pin, None).await.unwrap();

    assert_eq!(work.entries, 2);
}
