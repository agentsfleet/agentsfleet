//! A save over a real pin: the guest agent's walk copies only what changed,
//! and the catalog registers only those chunks; the pause's work grows with
//! entries and changed bytes, never with untouched bytes.

use super::tests::{Checked, catalog, files, id, objects, resolve};
use crate::recovery::tree::Tree;
use crate::run_loop::pin::{Pinned, Pinning, pin_sandbox, release};
use crate::staging::Staging;
use crate::test_support::FakeEngine;
use af_core::id::Uuid7;
use af_guest_wire::protocol::MAX_FRAME_BYTES;
use af_guest_wire::tree::Pins;
use afr_engine::{GuestVolume, PausedWork, Quiesced, Sandbox, quiesce};
use cap_std::{ambient_authority, fs::Dir};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// The three files every test rewrites after the first pin, at new sizes.
const REWRITTEN: [(&str, usize); 3] = [("file-0001", 3), ("file-0400", 70_000), ("file-0998", 5)];

/// A sandbox serving `root`, its tenant leaf frozen for each pin, and its
/// guest agent's pin record.
async fn sandbox(root: &Path) -> Checked<(Box<dyn Sandbox>, Arc<Mutex<Pins>>)> {
    let engine = FakeEngine {
        workspace: Some(root.to_owned()),
        toolbox: Some("b".repeat(64)),
        freezes: true,
        ..FakeEngine::default()
    };
    Ok((engine.sandbox().await?, Arc::clone(&engine.pins)))
}

async fn pinned<'a>(
    sandbox: &dyn Sandbox,
    volume: &'a GuestVolume<'_>,
    parent: Option<&Uuid7>,
) -> Checked<Box<dyn Quiesced + 'a>> {
    let quiesced = async {
        let held = quiesce(volume, parent).await?;
        Ok(Box::new(held) as Box<dyn Quiesced + 'a>)
    };
    match pin_sandbox(sandbox, quiesced).await? {
        Pinning::Pinned(pin) => Ok(pin),
        Pinning::Skipped(reason) => Err(format!("the save skipped as {reason:?}").into()),
    }
}

fn rewrite(root: &Path) -> Checked<u64> {
    let mut bytes = 0;
    for (name, size) in REWRITTEN {
        fs::write(root.join(name), vec![b'z'; size])?;
        bytes += u64::try_from(size)?;
    }
    Ok(bytes)
}

#[tokio::test]
async fn a_save_reads_and_registers_only_the_changed_files() -> Checked {
    let (root, staged) = (tempfile::tempdir()?, tempfile::tempdir()?);
    files(root.path(), 1_000, 32)?;
    let (sandbox, _pins) = sandbox(root.path()).await?;
    let staging = Staging::new(staged.path().to_owned());
    let volume = GuestVolume::new(sandbox.as_ref(), staging.root());
    let ceiling = af_wire::run::TURN_CHECKPOINT_PAGE_BYTES;
    let first = pinned(sandbox.as_ref(), &volume, None).await?;
    let base = catalog(&Pinned(&*first), None, ceiling).await?;
    let head = id()?;
    let lineage = base.lineage(head.clone());
    release(first, Some(&head)).await;
    let changed = rewrite(root.path())?;

    let second = pinned(sandbox.as_ref(), &volume, Some(&head)).await?;
    let delta = catalog(&Pinned(&*second), Some(&lineage), ceiling).await?;

    assert_eq!(
        second.paused_work().changed_bytes,
        changed,
        "only the 3 files were read"
    );
    assert_eq!(delta.parent.as_ref(), Some(&head));
    let registered: Vec<_> = delta
        .chunks
        .values()
        .map(|chunk| chunk.descriptor.size)
        .collect();
    assert_eq!(
        registered.len(),
        3,
        "registrations name only the rewritten files' chunks"
    );
    assert_eq!(registered.iter().sum::<u64>(), changed);
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
        "the restore is the source tree"
    );
    release(second, None).await;
    Ok(())
}

/// Records a walk of `root` as the pin `head` committed, as the guest agent
/// does, without packing it.
fn committed(pins: &Mutex<Pins>, root: &Path, head: &Uuid7) -> Checked {
    let mut pins = pins.lock().map_err(|_poisoned| "the pin record panicked")?;
    let dir = Dir::open_ambient_dir(root, ambient_authority())?;
    let first = pins.walk(&dir, None, MAX_FRAME_BYTES)?;
    pins.release(first.pin, Some(&head.to_string()))?;
    Ok(())
}

/// The pause's work for a workspace of 1,000 entries whose first file holds
/// `untouched` bytes as a hole, after the same three rewrites.
async fn paused_work(untouched: u64) -> Checked<(PausedWork, u64)> {
    let (root, staged) = (tempfile::tempdir()?, tempfile::tempdir()?);
    files(root.path(), 1_000, 1)?;
    fs::File::options()
        .write(true)
        .open(root.path().join("file-0000"))?
        .set_len(untouched)?;
    let (sandbox, pins) = sandbox(root.path()).await?;
    // The first pin is recorded as committed without packing the workspace:
    // only its identities matter to the pause after it.
    let head = id()?;
    committed(&pins, root.path(), &head)?;
    let changed = rewrite(root.path())?;
    let staging = Staging::new(staged.path().to_owned());
    let volume = GuestVolume::new(sandbox.as_ref(), staging.root());
    let pin = pinned(sandbox.as_ref(), &volume, Some(&head)).await?;
    let work = pin.paused_work();
    release(pin, None).await;
    Ok((work, changed))
}

#[tokio::test]
async fn a_pause_does_work_in_entries_and_changed_bytes_only() -> Checked {
    let (small, changed) = paused_work(100 << 20).await?;
    let (large, _) = paused_work(64 << 30).await?;

    assert_eq!(
        small, large,
        "64 GiB untouched costs the pause what 100 MiB does"
    );
    assert_eq!(small.entries, 1_000, "one identity read per entry");
    assert_eq!(
        small.changed_bytes, changed,
        "and a copy of the changed bytes"
    );
    Ok(())
}
