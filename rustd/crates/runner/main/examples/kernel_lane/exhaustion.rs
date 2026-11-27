//! The trials of a guest under exhaustion: full disks and runaway memory,
//! what the monitor's cgroup counts of them, and what is left afterwards.

use std::fs;
use std::path::PathBuf;

use afr_cgroup::{MEMORY_EVENTS, RunCgroup, SANDBOX_LEAF};
use afr_engine::conformance::{ENOSPC, SMALL_DISK, SMALL_MEMORY};
use afr_engine::{Engine as _, Limits, Network};
use afr_executor::Ending;
use libtest_mimic::Failed;

use crate::lane::{Lane, id_of, isolated, spec};
use crate::run::{expect, in_sandbox, in_sandbox_each, run as run_in, runtime, shell};

/// A process that allocates, and touches, far more than [`SMALL_MEMORY`].
// pin test: literal is the contract
pub(crate) const HOG: &str = "exec python3 -c 'b = bytearray(2 * 1024 ** 3); print(len(b))'";
/// What a command that should run after an exhaustion prints.
pub(crate) const OK: &str = "ok";
/// The count of processes the kernel killed for memory, none yet.
pub(crate) const NO_OOM_KILLS: &str = "oom_kill 0";
/// Twice the default memory limit, written to `/workspace`, past its disk.
// pin test: literal is the contract
const FILL_PAST_THE_DISK: &str = "dd if=/dev/zero of=/workspace/fill bs=1M count=4096 2>&1";
/// Why a trial that ran two scripts and got another count fails.
const TWO_OUTCOMES: &str = "two outcomes";

/// The `memory.events` of the leaf sandbox `name`'s monitor runs in: what the
/// host's kernel killed of the microVM itself.
pub(crate) fn monitor_events(lane: &Lane, name: &str) -> PathBuf {
    lane.cgroup_of(name).join(SANDBOX_LEAF).join(MEMORY_EVENTS)
}

/// `/workspace` and `/tmp` draw on one disk: what the workspace took,
/// `/tmp` no longer has; and `lost+found` is not in `/workspace`.
pub(crate) fn workspace_and_tmp_share_the_disk(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        disk_bytes: SMALL_DISK,
        ..Limits::default()
    };
    let outcomes = in_sandbox_each(
        lane,
        "shared",
        limits,
        Network::Isolated,
        &[
            "dd if=/dev/zero of=/workspace/fill bs=1M count=40 2>&1; ls -a /workspace",
            "dd if=/dev/zero of=/tmp/fill bs=1M count=40 2>&1",
        ],
    )?;
    let [workspace, tmp] = outcomes.as_slice() else {
        return Err(Failed::from(TWO_OUTCOMES));
    };
    expect(
        !workspace.output.contains(ENOSPC),
        format!("40 MiB fit the workspace, got {:?}", workspace.output),
    )?;
    expect(
        !workspace.output.contains("lost+found"),
        format!("no lost+found in /workspace, got {:?}", workspace.output),
    )?;
    expect(
        tmp.output.contains(ENOSPC),
        format!(
            "the second 40 MiB find the disk shared, got {:?}",
            tmp.output
        ),
    )
}

/// A hog past its guest's memory is killed for memory, and the engine goes
/// on to build another sandbox.
pub(crate) fn runaway(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let hog = in_sandbox(lane, "hog", limits, HOG)?;
    killed_for_memory(hog.ending)?;
    in_sandbox(lane, "after", Limits::default(), "true").map(drop)
}

/// A tenant process allocating past its guest's memory is killed and reads
/// `out_of_memory`; the monitor's leaf counts no kill, and the same executor
/// runs the next command.
pub(crate) fn oom_kills_only_the_tenant(lane: &Lane) -> Result<(), Failed> {
    let name = "oomsplit";
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let (hog, after, events) = runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(name);
        let sandbox = engine
            .prepare(spec(&engine, &id, limits, Network::Isolated)?)
            .await?;
        let hog = run_in(sandbox.executor(), shell(HOG)).await;
        let after = run_in(sandbox.executor(), shell(&format!("echo {OK}"))).await;
        let events = fs::read_to_string(monitor_events(lane, name)).unwrap_or_default();
        sandbox.destroy().await?;
        Ok::<_, Failed>((hog?, after?, events))
    })?;
    killed_for_memory(hog.ending)?;
    expect(
        after.output.trim() == OK && after.ending == Ending::Exited(0),
        format!("the executor runs a command afterwards, got {after:?}"),
    )?;
    expect(
        events.lines().any(|line| line == NO_OOM_KILLS),
        format!("the monitor was not killed, got {events:?}"),
    )
}

/// A tenant process holds only its three standard streams.
pub(crate) fn tenant_holds_no_stray_descriptor(lane: &Lane) -> Result<(), Failed> {
    let said = in_sandbox(lane, "tenantfds", Limits::default(), "ls /proc/$$/fd")?.output;
    let held: Vec<&str> = said.lines().collect();
    expect(
        held == ["0", "1", "2"],
        format!("only the standard streams, got {held:?}"),
    )
}

/// A runner that died after making a sandbox's cgroup and directory leaves
/// both; the next engine's sweep removes them.
pub(crate) fn sweep_removes_a_leftover(lane: &Lane) -> Result<(), Failed> {
    let name = "leftover";
    let cgroup = lane.cgroup_of(name);
    // What a runner killed here leaves: a monitor's cgroup and the sandbox's
    // directory, with no sandbox to destroy either.
    drop(RunCgroup::create(
        &lane.config.cgroup_root,
        id_of(name).as_str(),
        &Limits::default(),
    )?);
    fs::create_dir_all(lane.run_dir(name))?;
    expect(
        cgroup.join(SANDBOX_LEAF).is_dir(),
        "the leftover cgroup is made",
    )?;

    drop(lane.microvms()?);

    expect(
        !cgroup.exists(),
        format!("the sweep removed {}", cgroup.display()),
    )?;
    expect(
        !lane.run_dir(name).exists(),
        "the sweep removed the sandbox's directory",
    )
}

/// Writing twice the memory limit to `/workspace` ends at the disk's limit,
/// not in the out-of-memory killer: the guest's page cache writes back to
/// its drive, and the host never kills the monitor for the image's pages.
pub(crate) fn disk_fill_under_memory_limit_ends_in_enospc(lane: &Lane) -> Result<(), Failed> {
    let name = "diskfill";
    let (filled, events) = runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(name);
        let sandbox = engine.prepare(isolated(&engine, &id)?).await?;
        let filled = run_in(sandbox.executor(), shell(FILL_PAST_THE_DISK)).await;
        let events = fs::read_to_string(monitor_events(lane, name)).unwrap_or_default();
        sandbox.destroy().await?;
        Ok::<_, Failed>((filled?, events))
    })?;
    expect(
        filled.output.contains(ENOSPC),
        format!("the fill ends at the disk, got {:?}", filled.output),
    )?;
    expect(
        events.lines().any(|line| line == NO_OOM_KILLS),
        format!("the fill killed nothing for memory, got {events:?}"),
    )
}

/// The hog's ending is the kernel's out-of-memory kill, and nothing else.
pub(crate) fn killed_for_memory(ending: Ending) -> Result<(), Failed> {
    expect(
        ending == Ending::OutOfMemory,
        format!("the hog is killed for memory, got {ending:?}"),
    )
}
