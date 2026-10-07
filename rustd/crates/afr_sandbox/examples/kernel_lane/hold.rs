//! A sandbox held between two of its fleet's leases: frozen, it runs nothing
//! and keeps everything; thawed, the next lease finds the files and the
//! processes the last one left.
//!
//! Every observation while frozen is made from the host, because the executor
//! is frozen with everything else and answers nothing until the thaw.

use std::fs;
use std::os::unix::process::ExitStatusExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

use afr_sandbox::{Engine as _, LeaseCgroup, Limits, SANDBOX_LEAF, Sandbox, SandboxRequest};
use libtest_mimic::Failed;
use rustix::process::Signal;

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
/// A cgroup's own state, its `frozen` key among it.
const CGROUP_EVENTS: &str = "cgroup.events";
/// The file a process is moved into a cgroup through.
const CGROUP_PROCS: &str = "cgroup.procs";
/// The lease whose held sandbox is destroyed without a thaw.
const DESTROYED_FROZEN: &str = "heldgone";
/// The lease a runner died holding, frozen.
const LEFT_FROZEN: &str = "heldleft";
/// A process that outlives the trial unless something kills it.
const SLEEPER: &str = "sleep";
const SLEEP_SECONDS: &str = "60";
/// How long a process the sweep killed may take to be reaped, and how often
/// it is looked for.
const REAP_WAIT: Duration = Duration::from_secs(1);
const REAP_POLL: Duration = Duration::from_millis(10);

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
    let frozen_events = fs::read_to_string(events(lane, LEASE)).unwrap_or_default();
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
fn events(lane: &Lane, lease_id: &str) -> PathBuf {
    lane.config.cgroup_root.join(lease_id).join(CGROUP_EVENTS)
}

fn size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

/// A held sandbox its fleet never comes back for is destroyed as it stands,
/// frozen: every process is killed where it stopped, and its cgroup and its
/// directory go, with no thaw first.
pub(crate) fn destroy_frozen(lane: &Lane) -> Result<(), Failed> {
    let (frozen_events, destroyed) = runtime().block_on(async {
        let request = SandboxRequest {
            lease_id: DESTROYED_FROZEN,
            limits: Limits::default(),
        };
        let sandbox = lane.engine().prepare(request).await?;
        run_in(sandbox.executor(), shell(START_TICKER)).await?;
        sandbox.freeze().await?;
        let frozen_events = fs::read_to_string(events(lane, DESTROYED_FROZEN));
        Ok::<_, Failed>((frozen_events.unwrap_or_default(), sandbox.destroy().await))
    })?;
    expect(
        frozen_events.lines().any(|line| line == FROZEN),
        format!("frozen before the destroy, got {frozen_events:?}"),
    )?;
    expect(
        destroyed.is_ok(),
        format!("destroyed without a thaw, got {destroyed:?}"),
    )?;
    expect(
        !lane.config.cgroup_root.join(DESTROYED_FROZEN).exists(),
        "its cgroup is gone",
    )?;
    expect(
        !lane.lease_dir(DESTROYED_FROZEN).exists(),
        "its directory is gone",
    )
}

/// A runner that died holding a sandbox leaves its cgroup frozen with a
/// process stopped inside; the next engine's boot sweep kills it where it
/// stands and removes the cgroup and the lease's directory.
pub(crate) fn sweep_frozen(lane: &Lane) -> Result<(), Failed> {
    let cgroup = lane.config.cgroup_root.join(LEFT_FROZEN);
    let made = LeaseCgroup::create(&lane.config.cgroup_root, LEFT_FROZEN, &Limits::default())?;
    let mut stopped = Command::new(SLEEPER).arg(SLEEP_SECONDS).spawn()?;
    fs::write(
        cgroup.join(SANDBOX_LEAF).join(CGROUP_PROCS),
        stopped.id().to_string(),
    )?;
    made.freezer().freeze()?;
    let frozen_events = fs::read_to_string(events(lane, LEFT_FROZEN)).unwrap_or_default();
    drop(made);
    fs::create_dir_all(lane.lease_dir(LEFT_FROZEN))?;

    drop(lane.engine());

    // Reaped here if the sweep killed it; killed here, so nothing outlives
    // the trial, if it did not.
    let swept = ended_within(&mut stopped, REAP_WAIT)?;
    if swept.is_none() {
        stopped.kill()?;
        stopped.wait()?;
    }
    expect(
        frozen_events.lines().any(|line| line == FROZEN),
        format!("the leftover was frozen, got {frozen_events:?}"),
    )?;
    expect(
        swept.and_then(|status| status.signal()) == Some(Signal::KILL.as_raw()),
        format!("the sweep killed the stopped process, got {swept:?}"),
    )?;
    expect(!cgroup.exists(), "the sweep removed the frozen cgroup")?;
    expect(
        !lane.lease_dir(LEFT_FROZEN).exists(),
        "the sweep removed the lease's directory",
    )
}

/// `child`'s status once it has ended, looked for until `limit` passes;
/// `None` while it still runs.
fn ended_within(child: &mut Child, limit: Duration) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait()? {
            None if Instant::now() < deadline => std::thread::sleep(REAP_POLL),
            ended => return Ok(ended),
        }
    }
}
