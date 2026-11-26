//! Every engine that declares snapshots holds to one behaviour: a restore
//! brings back the saved files byte for byte and ends every process of the
//! environment before it, and a forgotten or unknown snapshot is refused. An
//! engine that declares none offers none.

use bytes::Bytes;
use libtest_mimic::Failed;

use super::Context;
use super::run::{BACKGROUND_ALIVE, GONE, build, expect, said, start_background, within};
use crate::engine::{Engine, Limits, Sandbox, SnapshotRef, Snapshots, fresh_sandbox_id};
use crate::error::unsupported;

/// The file a snapshot saves, what it holds when saved, and what it is
/// changed to after.
const SAVED_FILE: &str = "saved";
const SAVED: &[u8] = b"the bytes a snapshot keeps";
const CHANGED: &[u8] = b"the bytes written after it";
/// A file written only after the snapshot, which a restore removes.
const LATER_FILE: &str = "later";
/// How much of a file a read takes.
const FILE_BYTES: u64 = 64;

/// Runs `body` over `engine`'s snapshots in a fresh sandbox, when it declares
/// them; when it declares none, its sandbox must offer none.
async fn offered(
    engine: &dyn Engine,
    body: impl AsyncFnOnce(&dyn Sandbox, &dyn Snapshots) -> Result<(), Failed>,
) -> Result<(), Failed> {
    let capabilities = engine.capabilities();
    within(
        build(engine, Limits::DEFAULT).await?,
        async |sandbox| match (capabilities.snapshots, sandbox.snapshots()) {
            (true, Some(snapshots)) => body(sandbox, snapshots).await,
            (false, None) => Ok(()),
            (true, None) => Err(format!(
                "it declares snapshots, and its sandbox answers {}",
                unsupported(capabilities.engine, "snapshot")
            )
            .into()),
            (false, Some(_offered)) => {
                Err("its sandbox offers snapshots it does not declare".into())
            }
        },
    )
    .await
}

/// A restore brings back the saved file byte for byte and removes a file
/// written after the save.
pub(super) async fn restores_the_files(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, async |sandbox, snapshots| {
        let executor = sandbox.executor();
        executor
            .write_file(SAVED_FILE, Bytes::from_static(SAVED))
            .await?;
        let snapshot = snapshots.take(fresh_sandbox_id()).await?;
        executor
            .write_file(SAVED_FILE, Bytes::from_static(CHANGED))
            .await?;
        executor
            .write_file(LATER_FILE, Bytes::from_static(CHANGED))
            .await?;
        snapshots.restore(&snapshot).await?;
        let saved = executor.read_file(SAVED_FILE, FILE_BYTES).await?;
        let later = executor.read_file(LATER_FILE, FILE_BYTES).await;
        snapshots.forget(&snapshot).await?;
        expect(
            saved.data == SAVED,
            format!("the saved file reads back as saved, got {:?}", saved.data),
        )?;
        expect(
            later.is_err(),
            format!("a file written after the save is gone, got {later:?}"),
        )
    })
    .await
}

/// No process of the environment a restore replaced still runs.
pub(super) async fn no_process_survives(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, async |sandbox, snapshots| {
        start_background(sandbox).await?;
        let snapshot = snapshots.take(fresh_sandbox_id()).await?;
        snapshots.restore(&snapshot).await?;
        let after = said(sandbox, BACKGROUND_ALIVE).await?;
        snapshots.forget(&snapshot).await?;
        expect(
            after == GONE,
            format!("no process of the old environment survives, got {after:?}"),
        )
    })
    .await
}

/// A snapshot once forgotten is refused.
pub(super) async fn forgotten(engine: &dyn Engine, _context: &Context<'_>) -> Result<(), Failed> {
    offered(engine, async |_sandbox, snapshots| {
        let snapshot = snapshots.take(fresh_sandbox_id()).await?;
        snapshots.forget(&snapshot).await?;
        let restored = snapshots.restore(&snapshot).await;
        expect(
            restored.is_err(),
            "a forgotten snapshot is refused, and it restored",
        )
    })
    .await
}

/// A snapshot the sandbox never took is refused.
pub(super) async fn unknown(engine: &dyn Engine, _context: &Context<'_>) -> Result<(), Failed> {
    offered(engine, async |_sandbox, snapshots| {
        let restored = snapshots
            .restore(&SnapshotRef::new(fresh_sandbox_id()))
            .await;
        expect(
            restored.is_err(),
            "a snapshot it never took is refused, and it restored",
        )
    })
    .await
}
