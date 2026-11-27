//! Every engine that declares snapshots holds to one behaviour: a restore
//! brings back the saved files byte for byte and ends every process of the
//! environment before it; a forgotten or unknown snapshot is refused; a
//! snapshot from another environment is refused naming it, before any chunk
//! is read; and no restore brings back a plan older than the one in force.
//! An engine that declares none offers none.

use std::sync::Arc;

use af_chunk_store::{ChunkStore, MemoryStore};
use af_wire::runner::SandboxEngine;
use bytes::Bytes;
use libtest_mimic::Failed;

use super::Context;
use super::run::{BACKGROUND_ALIVE, GONE, expect, said, start_background};
use crate::engine::{
    EgressPlan, Engine, Environment, Generation, Limits, Root, Sandbox, SandboxSpec,
    SnapshotFormat, SnapshotRef, Snapshots, fresh_sandbox_id,
};
use crate::error::{EgressRefusal, SnapshotRefusal, unsupported};
use crate::network::Network;

/// The file a snapshot saves, what it holds when saved, and what it is
/// changed to after.
const SAVED_FILE: &str = "saved";
const SAVED: &[u8] = b"the bytes a snapshot keeps";
const CHANGED: &[u8] = b"the bytes written after it";
/// A file written only after the snapshot, which a restore removes.
const LATER_FILE: &str = "later";
/// How much of a file a read takes.
const FILE_BYTES: u64 = 64;
/// The plan a snapshot is taken under, and the later one in force when it is
/// restored.
const TAKEN_UNDER: u64 = 3;
const IN_FORCE: u64 = 4;
/// Why a sandbox that offered snapshots fails a trial when it stops.
const STOPPED_OFFERING: &str = "the sandbox stopped offering snapshots";

/// Runs `body` over a fresh sandbox of `engine`, built with the context's
/// store, when it declares snapshots; when it declares none, its sandbox
/// must offer none. The sandbox is destroyed whatever `body` found.
async fn offered(
    engine: &dyn Engine,
    context: &Context<'_>,
    body: impl AsyncFnOnce(&mut dyn Sandbox) -> Result<(), Failed>,
) -> Result<(), Failed> {
    let capabilities = engine.capabilities();
    let store: Arc<dyn ChunkStore> = Arc::<MemoryStore>::clone(context.store);
    let id = fresh_sandbox_id();
    let spec = SandboxSpec::granted_by(engine, &id, Limits::DEFAULT)?.with_store(&store);
    let mut sandbox = engine.prepare(spec).await?;
    let found = match (capabilities.snapshots, sandbox.snapshots().is_some()) {
        (true, true) => body(&mut *sandbox).await,
        (false, false) => Ok(()),
        (true, false) => Err(format!(
            "it declares snapshots, and its sandbox answers {}",
            unsupported(capabilities.engine, "snapshot")
        )
        .into()),
        (false, true) => Err("its sandbox offers snapshots it does not declare".into()),
    };
    let destroyed = sandbox.destroy().await;
    found?;
    Ok(destroyed?)
}

/// The snapshots `sandbox` offers; [`offered`] proved it offers them.
fn held(sandbox: &dyn Sandbox) -> Result<&dyn Snapshots, Failed> {
    Ok(sandbox.snapshots().ok_or(STOPPED_OFFERING)?)
}

/// A restore brings back the saved file byte for byte and removes a file
/// written after the save.
pub(super) async fn restores_the_files(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        let (agent, snapshots) = (sandbox.agent(), held(sandbox)?);
        agent
            .write_file(SAVED_FILE, Bytes::from_static(SAVED))
            .await?;
        let snapshot = snapshots.take(fresh_sandbox_id()).await?;
        agent
            .write_file(SAVED_FILE, Bytes::from_static(CHANGED))
            .await?;
        agent
            .write_file(LATER_FILE, Bytes::from_static(CHANGED))
            .await?;
        snapshots.restore(&snapshot).await?;
        let saved = agent.read_file(SAVED_FILE, FILE_BYTES).await?;
        let later = agent.read_file(LATER_FILE, FILE_BYTES).await;
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

/// What a quiesce fixed is what its publication saves, though tenant code
/// wrote on between them.
pub(super) async fn publishes_what_it_quiesced(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        let (agent, snapshots) = (sandbox.agent(), held(sandbox)?);
        agent
            .write_file(SAVED_FILE, Bytes::from_static(SAVED))
            .await?;
        let quiesced = snapshots.quiesce(None).await?;
        agent
            .write_file(SAVED_FILE, Bytes::from_static(CHANGED))
            .await?;
        let snapshot = snapshots.publish(quiesced, fresh_sandbox_id()).await?;
        snapshots.restore(&snapshot).await?;
        let saved = agent.read_file(SAVED_FILE, FILE_BYTES).await?;
        snapshots.forget(&snapshot).await?;
        expect(
            saved.data == SAVED,
            format!(
                "the publication saves the file as quiesced, got {:?}",
                saved.data
            ),
        )
    })
    .await
}

/// No process of the environment a restore replaced still runs.
pub(super) async fn no_process_survives(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        start_background(sandbox).await?;
        let snapshots = held(sandbox)?;
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
pub(super) async fn forgotten(engine: &dyn Engine, context: &Context<'_>) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        let snapshots = held(sandbox)?;
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
pub(super) async fn unknown(engine: &dyn Engine, context: &Context<'_>) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        let never = SnapshotRef::new(
            fresh_sandbox_id(),
            sandbox.environment().clone(),
            1,
            SnapshotFormat::Files,
        );
        let restored = held(sandbox)?.restore(&never).await;
        expect(
            restored.is_err(),
            "a snapshot it never took is refused, and it restored",
        )
    })
    .await
}

/// A ref naming another toolbox, another guest agent or the debug engine's
/// host root is refused naming `environment`, with no chunk read and the
/// workspace as it was.
pub(super) async fn never_crosses_environments(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        let (agent, snapshots) = (sandbox.agent(), held(sandbox)?);
        let taken = snapshots.take(fresh_sandbox_id()).await?;
        agent
            .write_file(LATER_FILE, Bytes::from_static(CHANGED))
            .await?;
        let reads = context.store.reads();
        let mut answers = Vec::new();
        for crossed in crossings(&taken) {
            let refused = snapshots.restore(&crossed).await.err();
            answers.push(refused.and_then(|refused| refused.snapshot_refusal()));
        }
        let read = context.store.reads() - reads;
        let later = agent.read_file(LATER_FILE, FILE_BYTES).await;
        snapshots.forget(&taken).await?;
        expect(
            answers
                .iter()
                .all(|answer| *answer == Some(SnapshotRefusal::Environment)),
            format!("each crossing is refused naming environment, got {answers:?}"),
        )?;
        expect(read == 0, format!("no chunk is read, got {read}"))?;
        expect(
            later.is_ok(),
            format!("the workspace is as it was, got {later:?}"),
        )
    })
    .await
}

/// `taken`, named as taken on another toolbox, another guest agent and the
/// debug engine's host root.
fn crossings(taken: &SnapshotRef) -> Vec<SnapshotRef> {
    let ran_on = taken.environment();
    let toolbox = Environment {
        root: Root::Toolbox {
            digest: "sha256-another-toolbox".into(),
        },
        ..ran_on.clone()
    };
    let guest_agent = Environment {
        guest_agent: Some("sha256-another-guest-agent".into()),
        ..ran_on.clone()
    };
    let host = Environment {
        engine: SandboxEngine::Unsandboxed,
        root: Root::Host,
        guest_agent: None,
    };
    [toolbox, guest_agent, host]
        .into_iter()
        .map(|environment| {
            SnapshotRef::new(
                taken.id().clone(),
                environment,
                taken.generation(),
                taken.format(),
            )
        })
        .collect()
}

/// A snapshot taken under plan 3 and restored while plan 4 is in force
/// leaves plan 4 in force: plan 3 is then refused as stale, naming 4.
pub(super) async fn never_an_older_plan(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    offered(engine, context, async |sandbox| {
        sandbox.egress(isolated(TAKEN_UNDER)).await?;
        let snapshot = held(sandbox)?.take(fresh_sandbox_id()).await?;
        sandbox.egress(isolated(IN_FORCE)).await?;
        held(sandbox)?.restore(&snapshot).await?;
        let refusal = sandbox.egress(isolated(TAKEN_UNDER)).await.err();
        held(sandbox)?.forget(&snapshot).await?;
        let refusal = refusal.and_then(|refused| refused.egress_refusal().cloned());
        expect(
            refusal
                == Some(EgressRefusal::Stale {
                    asked: TAKEN_UNDER,
                    in_force: IN_FORCE,
                }),
            format!("plan {IN_FORCE} stays in force through the restore, got {refusal:?}"),
        )
    })
    .await
}

/// Plan `generation`, reaching nothing.
const fn isolated(generation: u64) -> EgressPlan<'static> {
    EgressPlan {
        generation: Generation(generation),
        network: Network::Isolated,
    }
}
