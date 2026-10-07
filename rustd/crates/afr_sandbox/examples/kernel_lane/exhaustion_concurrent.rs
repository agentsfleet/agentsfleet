//! Spike S6, kept: four leases on one engine fill `/tmp` and then
//! `/workspace` at once, each writing twice its memory limit, and every
//! sandbox lives through it.
//!
//! S6 ran this by hand and lost every sandbox: `/tmp` was a tmpfs larger than
//! the lease's memory, so filling it ended in the out-of-memory killer taking
//! `bwrap`. The single-lease trials beside this one prove each repair; this
//! one proves they hold when four leases contend for one host at once.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use afr_executor::Ending;
use afr_sandbox::{BubblewrapEngine, Engine as _, Limits, SANDBOX_LEAF, SandboxRequest};
use libtest_mimic::Failed;

use crate::exhaustion::{MEMORY_EVENTS, NO_OOM_KILLS, OK};
use crate::lane::Lane;
use crate::run::{Outcome, expect, run as run_in, run_within, runtime, shell};
use crate::trials::ENOSPC;

/// The four leases, filling at once as S6's did.
const LEASES: [&str; 4] = ["s6a", "s6b", "s6c", "s6d"];
/// Each lease's disk: a quarter of the default, so four fill a lane host's
/// free space rather than exceed it.
const DISK: u64 = 1 << 30;
/// Each lease's memory: half its disk, so every fill writes twice the memory
/// limit, the ratio that killed S6's sandboxes through a tmpfs `/tmp`.
const MEMORY: u64 = DISK / 2;
/// How long each fill may run before the trial calls it hung. Four leases
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

/// What one lease saw: its two fills, the command after them, and its
/// sandbox leaf's memory events read before the sandbox was destroyed.
struct Seen {
    tmp: Outcome,
    workspace: Outcome,
    after: Outcome,
    events: String,
}

/// Four leases fill `/tmp` and `/workspace` at once; each fill ends in
/// `ENOSPC`, nothing in any sandbox leaf is killed, and each executor runs a
/// command afterwards.
pub(crate) fn writable_state_exhaustion_spares_the_sandbox(lane: &Lane) -> Result<(), Failed> {
    let seen = runtime().block_on(async {
        let engine = lane.engine();
        let [a, b, c, d] = LEASES;
        let (a, b, c, d) = tokio::join!(
            fill(&engine, lane, a),
            fill(&engine, lane, b),
            fill(&engine, lane, c),
            fill(&engine, lane, d),
        );
        [a, b, c, d]
    });
    for (lease_id, seen) in LEASES.into_iter().zip(seen) {
        survived(lease_id, &seen?)?;
    }
    Ok(())
}

/// Builds `lease_id`'s sandbox on the shared engine, runs both fills and a
/// command after them, and destroys it.
async fn fill(engine: &BubblewrapEngine, lane: &Lane, lease_id: &str) -> Result<Seen, Failed> {
    let limits = Limits {
        disk_bytes: DISK,
        memory_bytes: MEMORY,
        ..Limits::default()
    };
    let sandbox = engine.prepare(SandboxRequest { lease_id, limits }).await?;
    let executor = sandbox.executor();
    let seen = async {
        let tmp = run_within(executor, shell(FILL_TMP), FILL_TIMEOUT).await?;
        let workspace = run_within(executor, shell(FILL_WORKSPACE), FILL_TIMEOUT).await?;
        let after = run_in(executor, shell(&format!("echo {OK}"))).await?;
        let events = fs::read_to_string(sandbox_events(lane, lease_id)).unwrap_or_default();
        Ok::<_, Failed>(Seen {
            tmp,
            workspace,
            after,
            events,
        })
    }
    .await;
    sandbox.destroy().await?;
    seen
}

/// The sandbox leaf's memory events: what the kernel killed among bubblewrap
/// and the executor.
fn sandbox_events(lane: &Lane, lease_id: &str) -> PathBuf {
    lane.config
        .cgroup_root
        .join(lease_id)
        .join(SANDBOX_LEAF)
        .join(MEMORY_EVENTS)
}

/// Both fills ended at the disk, the executor still runs, and nothing in the
/// sandbox leaf was killed.
fn survived(lease_id: &str, seen: &Seen) -> Result<(), Failed> {
    expect(
        seen.tmp.output.contains(ENOSPC),
        format!("{lease_id}: /tmp ends in ENOSPC, got {:?}", seen.tmp.output),
    )?;
    expect(
        seen.workspace.output.contains(ENOSPC),
        format!(
            "{lease_id}: /workspace ends in ENOSPC, got {:?}",
            seen.workspace.output
        ),
    )?;
    expect(
        seen.after.output.trim() == OK && seen.after.ending == Ending::Exited(0),
        format!(
            "{lease_id}: the executor runs a command afterwards, got {:?}",
            seen.after
        ),
    )?;
    expect(
        seen.events.lines().any(|line| line == NO_OOM_KILLS),
        format!(
            "{lease_id}: nothing in the sandbox leaf was killed, got {:?}",
            seen.events
        ),
    )
}
