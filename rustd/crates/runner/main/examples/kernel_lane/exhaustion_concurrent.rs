//! Spike S6, kept: four runs on one engine fill `/tmp` and then
//! `/workspace` at once, each writing twice its memory limit, and every
//! microVM lives through it.
//!
//! S6 ran this by hand and lost every sandbox: `/tmp` was a tmpfs larger than
//! the run's memory, so filling it ended in the out-of-memory killer. The
//! single-run trials beside this one prove each repair; this one proves they
//! hold when four guests contend for one host's disk at once.

use std::fs;
use std::time::Duration;

use afr_engine::conformance::ENOSPC;
use afr_engine::{Engine as _, Limits, Network};
use afr_engine_firecracker::FirecrackerEngine;
use afr_executor::Ending;
use libtest_mimic::Failed;

use crate::exhaustion::{NO_OOM_KILLS, OK, monitor_events};
use crate::lane::{Lane, id_of, spec};
use crate::run::{Outcome, expect, run as run_in, run_within, runtime, shell};

/// The four runs, filling at once as S6's did.
const RUNS: [&str; 4] = ["s6a", "s6b", "s6c", "s6d"];
/// Each run's disk: a quarter of the default, so four fill a lane host's
/// free space rather than exceed it.
const DISK: u64 = 1 << 30;
/// Each run's memory: half its disk, so every fill writes twice the memory
/// limit, the ratio that killed S6's sandboxes through a tmpfs `/tmp`.
const MEMORY: u64 = DISK / 2;
/// How long each fill may run before the trial calls it hung. Four runs
/// fill one host disk at once, each slowed by its tenant throttle while its
/// pages are written back: minutes under load, where a command gets one.
const FILL_TIMEOUT: Duration = Duration::from_secs(300);
/// Fills `/tmp` past the disk.
// pin test: literal is the contract
const FILL_TMP: &str = "dd if=/dev/zero of=/tmp/fill bs=1M count=2048 2>&1";
/// Frees `/tmp`, then fills `/workspace` past the disk.
// pin test: literal is the contract
const FILL_WORKSPACE: &str =
    "rm -f /tmp/fill; dd if=/dev/zero of=/workspace/fill bs=1M count=2048 2>&1";

/// What one run saw: its two fills, the command after them, and its
/// monitor leaf's memory events read before the sandbox was destroyed.
struct Seen {
    tmp: Outcome,
    workspace: Outcome,
    after: Outcome,
    events: String,
}

/// Four runs fill `/tmp` and `/workspace` at once; each fill ends in
/// `ENOSPC`, no monitor is killed, and each executor runs a command
/// afterwards.
pub(crate) fn writable_state_exhaustion_spares_the_sandbox(lane: &Lane) -> Result<(), Failed> {
    let engine = lane.microvms()?;
    let seen = runtime().block_on(async {
        let [a, b, c, d] = RUNS;
        let seen: [_; 4] = tokio::join!(
            fill(&engine, lane, a),
            fill(&engine, lane, b),
            fill(&engine, lane, c),
            fill(&engine, lane, d),
        )
        .into();
        seen
    });
    for (run_id, seen) in RUNS.into_iter().zip(seen) {
        survived(run_id, &seen?)?;
    }
    Ok(())
}

/// Builds `run_id`'s sandbox on the shared engine, runs both fills and a
/// command after them, and destroys it.
async fn fill(engine: &FirecrackerEngine, lane: &Lane, run_id: &str) -> Result<Seen, Failed> {
    let limits = Limits {
        disk_bytes: DISK,
        memory_bytes: MEMORY,
        ..Limits::default()
    };
    let id = id_of(run_id);
    let sandbox = engine
        .prepare(spec(engine, &id, limits, Network::Isolated)?)
        .await
        .map_err(|error| step(run_id, "prepare", &error))?;
    let executor = sandbox.executor();
    let seen = async {
        let tmp = run_within(executor, shell(FILL_TMP), FILL_TIMEOUT).await?;
        let workspace = run_within(executor, shell(FILL_WORKSPACE), FILL_TIMEOUT).await?;
        let after = run_in(executor, shell(&format!("echo {OK}"))).await?;
        let events = fs::read_to_string(monitor_events(lane, run_id)).unwrap_or_default();
        Ok::<_, Failed>(Seen {
            tmp,
            workspace,
            after,
            events,
        })
    }
    .await;
    sandbox
        .destroy()
        .await
        .map_err(|error| step(run_id, "destroy", &error))?;
    seen
}

/// A failed step of `run_id`, with its cause: four runs run at once, and
/// a bare error code says neither which one nor why.
fn step(run_id: &str, name: &str, error: &afr_engine::Error) -> Failed {
    format!("{run_id} {name}: {error:?}").into()
}

/// Both fills ended at the disk, the executor still runs, and the monitor
/// was not killed.
fn survived(run_id: &str, seen: &Seen) -> Result<(), Failed> {
    expect(
        seen.tmp.output.contains(ENOSPC),
        format!("{run_id}: /tmp ends in ENOSPC, got {:?}", seen.tmp.output),
    )?;
    expect(
        seen.workspace.output.contains(ENOSPC),
        format!(
            "{run_id}: /workspace ends in ENOSPC, got {:?}",
            seen.workspace.output
        ),
    )?;
    expect(
        seen.after.output.trim() == OK && seen.after.ending == Ending::Exited(0),
        format!(
            "{run_id}: the executor runs a command afterwards, got {:?}",
            seen.after
        ),
    )?;
    expect(
        seen.events.lines().any(|line| line == NO_OOM_KILLS),
        format!(
            "{run_id}: the monitor was not killed, got {:?}",
            seen.events
        ),
    )
}
