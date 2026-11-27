//! The trials of a guest under exhaustion: full disks, runaway memory and a
//! fork bomb, what the monitor's cgroup counts of them, and what is left
//! afterwards.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use afr_cgroup::{MEMORY_EVENTS, RunCgroup, SANDBOX_LEAF};
use afr_engine::conformance::{ENOSPC, SMALL_DISK, SMALL_MEMORY};
use afr_engine::{Engine as _, Limits, Network};
use afr_executor::{Ending, Executor};
use libtest_mimic::Failed;

use crate::firecracker::room;
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
/// The process cap the fork bomb meets: far below the default, so the
/// bomb's processes fit the guest's memory many times over.
const FORK_PIDS: u32 = 64;
/// Where the fork bomb writes how many children it forked and the `errno`
/// that stopped it, relative to the workspace.
const FORKS: &str = "forks";
/// The `errno` a fork refused at `pids.max` reads: `EAGAIN`.
const EAGAIN: &str = "11";
/// A fork bomb in a session of its own, so the shell's exit does not end it:
/// after two seconds, which the shell needs to see it lead that session and
/// exit, it forks children that sleep 30 seconds until a fork is refused,
/// then writes how many it forked and the refusal's `errno` to [`FORKS`].
/// The children hold the tenant leaf full while the trial lists the
/// workspace. It stops at four times [`FORK_PIDS`], so a leaf with no cap
/// fails the trial rather than the guest.
// pin test: literal is the contract
const FORK_BOMB: &str = "setsid python3 -c 'import os, time
time.sleep(2)
forked, errno = 0, 0
while forked < 256:
    try:
        child = os.fork()
    except OSError as refused:
        errno = refused.errno
        break
    if child == 0:
        time.sleep(30)
        os._exit(0)
    forked += 1
open(\"/workspace/forks\", \"w\").write(str(forked) + \" \" + str(errno))
' >/dev/null 2>&1 </dev/null & pid=$!; tries=0; \
     until [ \"$(cut -d' ' -f6 /proc/$pid/stat 2>/dev/null)\" = \"$pid\" ] || [ \"$tries\" -ge 100 ]; \
     do sleep 0.01; tries=$((tries + 1)); done; echo started";
/// What [`FORK_BOMB`] prints once the bomb leads its own session.
const STARTED: &str = "started";
/// How often the trial lists the workspace while the bomb runs.
const LIST_EVERY: Duration = Duration::from_millis(100);
/// How many listings it waits through: 20 seconds, inside the children's 30.
const LIST_TRIES: usize = 200;

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

/// A fork bomb inside the microVM stops at the run's process cap, the
/// tenant leaf's `pids.max`, with `EAGAIN`; while its children hold the leaf
/// full, the executor, outside the leaf, still lists the workspace, reads
/// what the bomb wrote and counts the refusals from the leaf's `pids.events`.
pub(crate) fn fork_bomb_stops_at_the_process_cap(lane: &Lane) -> Result<(), Failed> {
    let name = "forkbomb";
    let limits = Limits {
        pids: FORK_PIDS,
        ..Limits::default()
    };
    let (started, listed, forks, refused) = runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(name);
        let sandbox = engine
            .prepare(spec(&engine, &id, limits, Network::Isolated)?)
            .await?;
        let bombed = async {
            let started = run_in(sandbox.executor(), shell(FORK_BOMB)).await?;
            let listed = listed_while_full(sandbox.executor()).await?;
            let forks = sandbox.executor().read_file(FORKS, 64).await?;
            let refused = sandbox.executor().pids_refused().await?;
            Ok::<_, Failed>((started, listed, forks, refused))
        }
        .await;
        sandbox.destroy().await?;
        bombed
    })?;
    expect(
        started.output.trim() == STARTED,
        format!("the bomb leads its own session, got {started:?}"),
    )?;
    expect(
        listed,
        "the executor listed the workspace with the leaf full",
    )?;
    let said = String::from_utf8_lossy(&forks.data).into_owned();
    let (forked, errno) = said.split_once(' ').unwrap_or_default();
    expect(
        errno == EAGAIN && forked.parse::<u32>().is_ok_and(|forked| forked < FORK_PIDS),
        format!("the bomb stops below {FORK_PIDS} with EAGAIN, got {said:?}"),
    )?;
    expect(
        refused > 0,
        format!("the leaf counts the refused forks, got {refused}"),
    )
}

/// Lists the workspace through `executor` until the fork bomb's [`FORKS`]
/// appears in it, each listing an answer the executor gave while the bomb
/// ran: whether it appeared within [`LIST_TRIES`] listings.
async fn listed_while_full(executor: &dyn Executor) -> Result<bool, Failed> {
    for _try in 0..LIST_TRIES {
        let listing = executor.list_dir(".").await?;
        if listing.entries.iter().any(|entry| entry.name == FORKS) {
            return Ok(true);
        }
        tokio::time::sleep(LIST_EVERY).await;
    }
    Ok(false)
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
        room(&lane.config.tuning),
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
