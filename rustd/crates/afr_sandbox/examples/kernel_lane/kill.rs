//! A sandbox killed at its lease's end: every process inside is gone, one
//! that left its shell's group with `setsid` among them, while the disk stays
//! for the release that follows the report.

use afr_sandbox::{Engine as _, Killed, Limits, SandboxRequest};
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::{expect, run as run_in, runtime, shell};

/// The lease whose sandbox is killed.
const LEASE: &str = "killed";
/// A process that would outlive the lease by ten minutes, out of the
/// command's process group, so only the cgroup's kill can end it.
const SLEEPER: &str = "setsid sleep 600 >/dev/null 2>&1 </dev/null &";
/// A cgroup's own state, and what it reads once its tree is empty.
const CGROUP_EVENTS: &str = "cgroup.events";
const UNPOPULATED: &str = "populated 0";

/// What the host saw between the kill and the release.
struct Seen {
    killed: Killed,
    events: String,
    disk_kept: bool,
}

/// The kill settles with `sleep 600` still running before it: the cgroup
/// reads `populated 0` when the report would post, the lease's directory is
/// still there for the release, and the release then removes it.
pub(crate) fn no_sandbox_process_outlives_the_report(lane: &Lane) -> Result<(), Failed> {
    let seen = runtime().block_on(async {
        let request = SandboxRequest::new(LEASE, Limits::default());
        let sandbox = lane.engine().prepare(request).await?;
        run_in(sandbox.executor(), shell(SLEEPER)).await?;
        let killed = sandbox.kill().await;
        let events =
            std::fs::read_to_string(lane.config.cgroup_root.join(LEASE).join(CGROUP_EVENTS))?;
        let disk_kept = lane.lease_dir(LEASE).exists();
        sandbox.destroy().await?;
        Ok::<_, Failed>(Seen {
            killed,
            events,
            disk_kept,
        })
    })?;
    expect(
        matches!(seen.killed, Killed::Settled),
        format!("the kill proved the tree empty: {:?}", seen.killed),
    )?;
    expect(
        seen.events.lines().any(|line| line == UNPOPULATED),
        format!("no process is left: {:?}", seen.events),
    )?;
    expect(seen.disk_kept, "the disk waits for the release")?;
    expect(
        !lane.lease_dir(LEASE).exists(),
        "the release removed the lease's directory",
    )
}
