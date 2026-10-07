//! A sandbox held between two of its fleet's leases: frozen, it runs nothing
//! and keeps everything; thawed, the next lease finds the files and the
//! processes the last one left.
//!
//! Every observation while frozen is made from the host, because the executor
//! is frozen with everything else and answers nothing until the thaw.

use std::fs;
use std::path::Path;
use std::time::Duration;

use afr_sandbox::{Engine as _, Limits, Sandbox, SandboxRequest};
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::{Outcome, expect, run as run_in, runtime, shell};

/// The lease whose sandbox is held.
const LEASE: &str = "held";
/// A file the first lease writes, for the next to read back.
const NOTE: &str = "written before the hold";
/// Starts a ticker that appends to `tick` every 50 ms in a session of its
/// own, as a fleet's long-running server would be, and prints its pid.
///
/// The shell waits for the first tick before it ends: the executor kills the
/// shell's process group as it ends, and `setsid` is in that group until it
/// has made its session, so a shell that ended at once would take it along.
const START_TICKER: &str = "echo 'written before the hold' > /workspace/note; \
     setsid sh -c 'while :; do echo x >> /workspace/tick; sleep 0.05; done' \
     >/dev/null 2>&1 </dev/null & pid=$!; \
     until [ -s /workspace/tick ]; do sleep 0.01; done; echo $pid";
/// How long the frozen sandbox is watched for a tick it should not make.
const WATCH: Duration = Duration::from_millis(500);
/// What `cgroup.events` reads for a cgroup whose tree is all stopped.
const FROZEN: &str = "frozen 1";

/// What the host saw of the held sandbox.
struct Seen {
    pid: String,
    frozen_events: String,
    ticks_while_frozen: (u64, u64),
    after: Outcome,
    ticks_after_thaw: (u64, u64),
}

/// A frozen sandbox's cgroup reads `frozen 1` and its processes make no
/// progress; thawed, the file written before the hold reads back, the
/// background process is alive, and it runs on.
pub(crate) fn held_sandbox_resumes_where_it_stopped(lane: &Lane) -> Result<(), Failed> {
    let seen = runtime().block_on(async {
        let engine = lane.engine();
        let request = SandboxRequest {
            lease_id: LEASE,
            limits: Limits::default(),
        };
        let sandbox = engine.prepare(request).await?;
        let seen = hold_and_resume(lane, sandbox.as_ref()).await;
        sandbox.destroy().await?;
        seen
    })?;
    expect(
        seen.frozen_events.lines().any(|line| line == FROZEN),
        format!("the cgroup reports frozen, got {:?}", seen.frozen_events),
    )?;
    let (before, after) = seen.ticks_while_frozen;
    expect(
        before == after,
        format!("a frozen ticker makes no progress: {before} bytes, then {after}"),
    )?;
    expect(
        seen.after.output.lines().next() == Some(NOTE),
        format!("the next lease reads the note back, got {:?}", seen.after),
    )?;
    expect(
        seen.after.output.lines().any(|line| line == "alive"),
        format!(
            "process {} is still running, got {:?}",
            seen.pid, seen.after
        ),
    )?;
    let (thawed, later) = seen.ticks_after_thaw;
    expect(
        later > thawed,
        format!("the ticker runs on once thawed: {thawed} bytes, then {later}"),
    )
}

/// Starts the ticker, freezes, watches, thaws, and asks the executor what the
/// first lease left.
async fn hold_and_resume(lane: &Lane, sandbox: &dyn Sandbox) -> Result<Seen, Failed> {
    let started = run_in(sandbox.executor(), shell(START_TICKER)).await?;
    let pid = started.output.trim().to_owned();
    let workspace = sandbox
        .workspace()
        .ok_or_else(|| Failed::from("the bubblewrap engine offers its workspace to the host"))?;
    let tick = workspace.root.join("tick");
    tokio::time::sleep(WATCH).await;

    sandbox.freeze().await?;
    let frozen_events = fs::read_to_string(events(lane)).unwrap_or_default();
    let frozen_at = size(&tick);
    tokio::time::sleep(WATCH).await;
    let ticks_while_frozen = (frozen_at, size(&tick));
    sandbox.thaw().await?;

    let check = format!("cat /workspace/note; kill -0 {pid} && echo alive");
    let after = run_in(sandbox.executor(), shell(&check)).await?;
    let thawed = size(&tick);
    tokio::time::sleep(WATCH).await;
    Ok(Seen {
        pid,
        frozen_events,
        ticks_while_frozen,
        after,
        ticks_after_thaw: (thawed, size(&tick)),
    })
}

/// The lease cgroup's own state, its `frozen` key among it.
fn events(lane: &Lane) -> std::path::PathBuf {
    lane.config.cgroup_root.join(LEASE).join("cgroup.events")
}

fn size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}
