//! A sandbox killed at its lease's end: every process inside is gone, one
//! that left its shell's group with `setsid` among them, while the disk stays
//! for the release that follows the report.

use std::fs;
use std::path::Path;

use afr_sandbox::{Engine as _, Killed, Limits, SandboxRequest, TENANT_LEAF};
use libtest_mimic::Failed;

use crate::admission::MOUNTINFO;
use crate::lane::Lane;
use crate::run::{expect, run as run_in, runtime, shell};

/// The lease whose sandbox is killed.
const LEASE: &str = "killed";
/// A process that would outlive the lease by ten minutes, out of the
/// command's process group, so only the cgroup's kill can end it. The
/// executor kills what is left in a command's group once its shell exits, so
/// the shell waits, at most five seconds, until the sleeper leads a session of
/// its own (its `stat` session field is its own pid) before it exits.
const SLEEPER: &str = "setsid sleep 600 >/dev/null 2>&1 </dev/null & sleeper=$!; tries=0; \
    until [ \"$(cut -d' ' -f6 /proc/$sleeper/stat)\" = \"$sleeper\" ] || [ \"$tries\" -ge 500 ]; \
    do sleep 0.01; tries=$((tries + 1)); done";
/// What the sleeper's command line holds, as `setsid` and as the `sleep` it
/// becomes.
const SLEEPING: &str = "sleep 600";
/// A cgroup's own state, and what it reads once its tree is empty.
const CGROUP_EVENTS: &str = "cgroup.events";
const UNPOPULATED: &str = "populated 0";
/// The processes a cgroup holds.
const CGROUP_PROCS: &str = "cgroup.procs";
/// Where a lease's workspace disk is mounted, under its directory.
const DISK_MOUNT: &str = "workspace";

/// What the host saw before the kill, and between the kill and the release.
struct Seen {
    sleeping: bool,
    killed: Killed,
    events: String,
    mounted: bool,
}

/// The kill settles with `sleep 600` running in the tenant leaf before it:
/// the cgroup reads `populated 0` when the report would post, the workspace
/// disk is still mounted for the release, and the release then removes the
/// lease's directory.
pub(crate) fn no_sandbox_process_outlives_the_report(lane: &Lane) -> Result<(), Failed> {
    let cgroup = lane.config.cgroup_root.join(LEASE);
    let seen = runtime().block_on(async {
        let request = SandboxRequest::new(LEASE, Limits::default());
        let sandbox = lane.engine().prepare(request).await?;
        run_in(sandbox.executor(), shell(SLEEPER)).await?;
        let sleeping = sleeper_in(&cgroup.join(TENANT_LEAF))?;
        let killed = sandbox.kill().await;
        let events = fs::read_to_string(cgroup.join(CGROUP_EVENTS))?;
        let mounted = mounted_at(&lane.lease_dir(LEASE).join(DISK_MOUNT))?;
        sandbox.destroy().await?;
        Ok::<_, Failed>(Seen {
            sleeping,
            killed,
            events,
            mounted,
        })
    })?;
    expect(
        seen.sleeping,
        "the setsid sleeper ran in the tenant leaf before the kill",
    )?;
    expect(
        matches!(seen.killed, Killed::Settled),
        format!("the kill proved the tree empty: {:?}", seen.killed),
    )?;
    expect(
        seen.events.lines().any(|line| line == UNPOPULATED),
        format!("no process is left: {:?}", seen.events),
    )?;
    expect(
        seen.mounted,
        "the workspace disk stays mounted for the release",
    )?;
    expect(
        !lane.lease_dir(LEASE).exists(),
        "the release removed the lease's directory",
    )
}

/// Whether the cgroup at `leaf` holds the sleeper, by its command line.
fn sleeper_in(leaf: &Path) -> Result<bool, Failed> {
    let procs = fs::read_to_string(leaf.join(CGROUP_PROCS))?;
    Ok(procs.split_whitespace().any(|pid| {
        fs::read(Path::new("/proc").join(pid).join("cmdline")).is_ok_and(|cmdline| {
            String::from_utf8_lossy(&cmdline)
                .replace('\0', " ")
                .contains(SLEEPING)
        })
    }))
}

/// Whether anything is mounted at `point` on the host.
fn mounted_at(point: &Path) -> Result<bool, Failed> {
    let point = point.display().to_string();
    Ok(fs::read_to_string(MOUNTINFO)?
        .lines()
        .any(|line| line.split(' ').nth(4) == Some(point.as_str())))
}
