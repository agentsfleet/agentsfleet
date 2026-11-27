#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::sync::Arc;

use af_chunk_store::{ChunkStore, MemoryStore};
use af_wire::runner::SandboxEngine;

use self::guest::{Fault, Guest, Node};
use super::volume::GuestVolume;
use super::{FREEZE_TENANT, FileSnapshots, quiesce_workspace};
use crate::engine::volume::{Quiesced as _, WorkspaceVolume as _, pin_paused};
use crate::engine::{EgressPlan, Generation, Limits, ParkMode, Sandbox, SandboxState};
use crate::engine::{Environment, Root, SnapshotFormat, SnapshotRef, fresh_sandbox_id};
use crate::error::SnapshotRefusal;
use crate::network::Network;

mod guest;
mod restore;

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

/// Snapshots over `store`, spooled under a fresh directory.
fn snapshots(store: &Arc<MemoryStore>) -> (FileSnapshots, tempfile::TempDir) {
    let spools = tempfile::tempdir().unwrap();
    let store: Arc<dyn ChunkStore> = Arc::<MemoryStore>::clone(store);
    let snapshots = FileSnapshots::new(Some(store), spools.path().to_owned());
    (snapshots, spools)
}

/// A take holds tenant code only for the pin and publishes every entry; a
/// restore renews the workspace and lands each saved name, a second name
/// as a file of its own, and nothing written after the take.
#[tokio::test]
async fn test_a_restore_lands_what_the_take_saved_and_nothing_after() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store);
    let guest = Guest::default();
    guest.put("src", Node::Directory);
    guest.put("src/run.sh", file(b"#!/bin/sh\n", true));
    guest.put("src/notes", file(b"notes", false));
    guest.put("src/again", Node::SecondName("src/notes".to_owned()));
    guest.put("latest", Node::Symlink("src/notes".to_owned()));
    let saved = guest.held().nodes.clone();

    let snapshot = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    guest.put("src/notes", file(b"changed", false));
    guest.put("after", file(b"later", false));
    snapshots.restore(&guest, &snapshot).await.unwrap();

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
    let (snapshots, _spools) = snapshots(&store);
    let guest = Guest::default();
    guest.put("kept", file(b"kept", false));
    guest.put("gone", file(b"gone", false));
    let first = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    guest.held().nodes.remove("gone");
    guest.put("new", file(b"new", false));

    let second = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    snapshots.restore(&guest, &second).await.unwrap();

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
    let (snapshots, _spools) = snapshots(&store);
    let guest = Guest::default();
    guest.put("a", file(b"a", false));
    let taken = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    let elsewhere = Environment {
        guest_agent: Some("sha256-another".into()),
        ..ours()
    };
    let crossed = SnapshotRef::new(taken.id().clone(), elsewhere, 1, SnapshotFormat::Files);
    let other_generation = SnapshotRef::new(taken.id().clone(), ours(), 9, SnapshotFormat::Files);
    let never = SnapshotRef::new(fresh_sandbox_id(), ours(), 1, SnapshotFormat::Files);

    let mut refusals = Vec::new();
    for snapshot in [&crossed, &other_generation, &never] {
        let refused = snapshots.restore(&guest, snapshot).await.unwrap_err();
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
    let (snapshots, _spools) = snapshots(&store);
    let guest = Guest::default();
    let taken = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    let kept = snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();
    snapshots.forget(&taken).await.unwrap();
    snapshots.forget(&kept).await.unwrap();

    let restored = snapshots.restore(&guest, &kept).await;
    let again = snapshots.forget(&kept).await;
    snapshots.take(&guest, fresh_sandbox_id()).await.unwrap();

    assert!(restored.is_err() && again.is_err());
    assert_eq!(guest.held().walked.last(), Some(&None));
}

/// A pause the guest refuses, a pin streamed past the granted disk and a
/// publication the store refuses each fail the take, and name nothing; a
/// release the guest refuses after a commit costs only the next delta.
#[tokio::test]
async fn test_a_take_that_fails_names_nothing() {
    let store = Arc::new(MemoryStore::default());
    let (snapshots, _spools) = snapshots(&store);
    let frozen_out = Guest {
        fault: Fault::FreezeFails,
        ..Guest::default()
    };
    snapshots
        .take(&frozen_out, fresh_sandbox_id())
        .await
        .unwrap_err();

    let (small, _small_spools) = self::snapshots(&store);
    let guest = Guest::on_disk(16);
    guest.put("big", file(&[7; 4096], false));
    small.take(&guest, fresh_sandbox_id()).await.unwrap_err();

    let guest = Guest::default();
    let id = fresh_sandbox_id();
    snapshots.take(&guest, id.clone()).await.unwrap();
    snapshots.take(&guest, id).await.unwrap_err();
    assert_eq!(guest.held().released.last(), Some(&None));

    let unreleased = Guest {
        fault: Fault::ReleaseFails,
        ..Guest::default()
    };
    let (fresh, _fresh_spools) = self::snapshots(&store);
    let held = fresh.take(&unreleased, fresh_sandbox_id()).await;
    assert!(held.is_ok(), "{held:?}");
}

/// A pin states the work its pause did: the entries the walk read.
#[tokio::test]
async fn test_a_pin_states_the_work_its_pause_did() {
    let guest = Guest::default();
    guest.put("a", file(b"a", false));
    guest.put("b", Node::Directory);
    let spools = tempfile::tempdir().unwrap();
    let volume = GuestVolume::new(&guest, spools.path());

    let pin = pin_paused(&volume, None).await.unwrap();
    let work = volume.paused_work(&pin);
    volume.release(pin, None).await.unwrap();
    let held = crate::engine::volume::quiesce(&volume, None).await.unwrap();
    let told = format!("{held:?}");
    Box::new(held).release(None).await.unwrap();

    assert_eq!(work.entries, 2);
    assert_eq!(
        told, "QuiescedVolume { .. }",
        "a pin tells no tenant content"
    );
}

/// A sandbox whose engine offers snapshots is quiesced through them, and a
/// publication after tenant code ran on saves the workspace as it stood.
#[tokio::test]
async fn test_a_sandbox_with_snapshots_is_quiesced_through_them() {
    let store = Arc::new(MemoryStore::default());
    let (files, spools) = snapshots(&store);
    let guest = Guest {
        files: Some(files),
        ..Guest::default()
    };
    guest.put("a", file(b"as quiesced", false));
    let snapshots = guest.snapshots().unwrap();

    let quiesced = quiesce_workspace(&guest, spools.path(), None)
        .await
        .unwrap();
    guest.put("a", file(b"written after", false));
    let published = snapshots
        .publish(quiesced, fresh_sandbox_id())
        .await
        .unwrap();
    let taken = snapshots.take(fresh_sandbox_id()).await.unwrap();
    snapshots.restore(&published).await.unwrap();

    assert_eq!(
        guest.held().nodes.get("a"),
        Some(&file(b"as quiesced", false))
    );
    snapshots.forget(&taken).await.unwrap();
    assert_eq!(guest.held().frozen, 2);
}

/// With no tenant leaf, a quiet sandbox quiesces through its guest agent's
/// walk and a busy one is refused naming the freeze; a freeze that fails is
/// refused after a thaw is tried; and snapshots built with no store take,
/// restore and forget nothing.
#[tokio::test]
async fn test_a_quiesce_with_no_leaf_or_no_store_is_refused_by_name() {
    let spools = tempfile::tempdir().unwrap();
    let quiet = Guest {
        fault: Fault::NoLeaf,
        ..Guest::default()
    };
    let held = quiesce_workspace(&quiet, spools.path(), None)
        .await
        .unwrap();
    held.release(None).await.unwrap();
    let busy = Guest {
        fault: Fault::NoLeafBusy,
        ..Guest::default()
    };
    let refused = quiesce_workspace(&busy, spools.path(), None).await.err();
    assert_eq!(
        refused.and_then(|error| error.unsupported_operation()),
        Some(FREEZE_TENANT)
    );
    let stuck = Guest {
        fault: Fault::Stuck,
        ..Guest::default()
    };
    quiesce_workspace(&stuck, spools.path(), None)
        .await
        .err()
        .unwrap();

    let storeless = FileSnapshots::new(None, spools.path().to_owned());
    let guest = Guest::default();
    let taken = storeless.take(&guest, fresh_sandbox_id()).await.err();
    let never = SnapshotRef::new(fresh_sandbox_id(), ours(), 1, SnapshotFormat::Files);
    let restored = storeless.restore(&guest, &never).await.err();
    let forgotten = storeless.forget(&never).await.err();
    for refused in [taken, restored, forgotten] {
        assert_eq!(
            refused.and_then(|error| error.snapshot_refusal()),
            Some(SnapshotRefusal::Store)
        );
    }
}

/// The test's sandbox answers the calls no snapshot makes as a sandbox does.
#[tokio::test]
async fn test_the_stand_in_sandbox_answers_as_a_sandbox() {
    let mut guest = Box::new(Guest::default());
    let plan = EgressPlan {
        generation: Generation(1),
        network: Network::Isolated,
    };
    guest.egress(plan).await.unwrap();
    assert_eq!(guest.state(), SandboxState::Running);
    assert_eq!(Sandbox::id(&*guest), &guest.id);
    assert_eq!(guest.granted().limits().disk_bytes, DISK);
    assert!(guest.snapshots().is_none());
    assert!(matches!(guest.kill().await, crate::engine::Killed::Settled));
    assert_ne!(Limits::DEFAULT.disk_bytes, 0);
    let parked = Box::new(Guest::default()).park(ParkMode::Stop).await;
    parked.unwrap_err();
    guest.destroy().await.unwrap();
}
