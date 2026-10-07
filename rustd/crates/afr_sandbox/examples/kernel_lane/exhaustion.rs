//! The trials of a sandbox under exhaustion: full disks, runaway processes
//! and memory, and what is left of the sandbox afterwards.

use std::fs;
use std::os::unix::fs::MetadataExt as _;

use afr_executor::Ending;
use afr_sandbox::{Engine as _, LeaseCgroup, Limits, SANDBOX_LEAF, SandboxRequest, TENANT_LEAF};
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::{expect, in_sandbox, in_sandbox_each, run as run_in, runtime, shell};
use crate::trials::{ENOSPC, SMALL_DISK, TENANT_CGROUP, TWO_OUTCOMES};

/// Memory a runaway trial exceeds.
const SMALL_MEMORY: u64 = 256 * 1024 * 1024;
/// Processes a fork-bomb trial exceeds.
const FEW_PIDS: u32 = 64;
/// A process that allocates, and touches, far more than [`SMALL_MEMORY`].
// pin test: literal is the contract
const HOG: &str = "exec python3 -c 'b = bytearray(2 * 1024 ** 3); print(len(b))'";
/// What a command that should run after an exhaustion prints.
pub(crate) const OK: &str = "ok";
/// A cgroup's count of what its memory limit did.
pub(crate) const MEMORY_EVENTS: &str = "memory.events";
/// The count of processes the kernel killed for memory, none yet.
pub(crate) const NO_OOM_KILLS: &str = "oom_kill 0";

/// Where the kernel publishes a block device by number, and the file of a
/// loop device's that says whether it bypasses the host's page cache.
const SYS_DEV_BLOCK: &str = "/sys/dev/block";
const LOOP_DIO: &str = "loop/dio";
/// What that file holds when direct I/O is on.
const DIO_ON: &str = "1";
/// Where a lease's workspace disk is mounted, under its directory.
const DISK_MOUNT: &str = "workspace";
/// Twice the default memory limit, written to `/workspace`, past its disk.
// pin test: literal is the contract
const FILL_PAST_THE_DISK: &str = "dd if=/dev/zero of=/workspace/fill bs=1M count=4096 2>&1";

/// More forks than any lane's process limit allows.
const FORK_ATTEMPTS: u32 = 1000;

/// A fork bomb that counts how many children it got before the kernel refused.
fn fork_bomb() -> String {
    format!(
        "import os, time\nmade = 0\nfor _ in range({FORK_ATTEMPTS}):\n    try:\n        \
         if os.fork() == 0:\n            time.sleep(60)\n            os._exit(0)\n        \
         made += 1\n    except OSError:\n        break\nprint(made)\n"
    )
}

/// Filling `/tmp` ends in `ENOSPC`, the disk's answer, and the sandbox is
/// still there to run the next command: a tmpfs would have taken the
/// lease's memory and its first process with it.
pub(crate) fn full_tmp_answers_enospc(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        disk_bytes: SMALL_DISK,
        ..Limits::default()
    };
    let outcomes = in_sandbox_each(
        lane,
        "tmpfill",
        limits,
        &[
            "dd if=/dev/zero of=/tmp/fill bs=1M count=80 2>&1",
            "echo ok",
        ],
    )?;
    let [filled, after] = outcomes.as_slice() else {
        return Err(Failed::from(TWO_OUTCOMES));
    };
    expect(
        filled.output.contains(ENOSPC),
        format!("ENOSPC on /tmp, got {:?}", filled.output),
    )?;
    expect(
        after.output.trim() == "ok" && after.ending == Ending::Exited(0),
        format!("a command runs after the fill, got {after:?}"),
    )
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

pub(crate) fn runaway(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        pids: FEW_PIDS,
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let bomb = in_sandbox(
        lane,
        "forkbomb",
        limits,
        &format!("python3 -c '{}'", fork_bomb()),
    )?;
    let made: u32 = bomb
        .output
        .trim()
        .parse()
        .map_err(|_unparsed| format!("fork count, got {:?}", bomb.output))?;
    expect(
        made < FEW_PIDS,
        format!("pids.max stops the bomb, it made {made}"),
    )?;
    let hog = in_sandbox(lane, "hog", limits, HOG)?;
    killed_for_memory(hog.ending)?;
    // The supervisor's own process is this one, and it is still here to build
    // another sandbox.
    in_sandbox(lane, "after", Limits::default(), "true").map(drop)
}

/// A tenant process allocating past the tenant leaf's limit is killed and
/// reads `out_of_memory`; nothing in the sandbox leaf is killed, and the same
/// executor runs the next command.
pub(crate) fn oom_kills_only_the_tenant(lane: &Lane) -> Result<(), Failed> {
    let lease_id = "oomsplit";
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let sandbox_events = lane
        .config
        .cgroup_root
        .join(lease_id)
        .join(SANDBOX_LEAF)
        .join(MEMORY_EVENTS);
    let (hog, after, events) = runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest { lease_id, limits })
            .await?;
        let hog = run_in(sandbox.executor(), shell(HOG)).await;
        let after = run_in(sandbox.executor(), shell(&format!("echo {OK}"))).await;
        let events = fs::read_to_string(&sandbox_events).unwrap_or_default();
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
        format!("nothing in the sandbox leaf was killed, got {events:?}"),
    )
}

/// A tenant process holds only its three standard streams, so no cgroup
/// descriptor reaches it, and it runs in the tenant leaf.
pub(crate) fn tenant_holds_no_cgroup_descriptor(lane: &Lane) -> Result<(), Failed> {
    let said = in_sandbox(
        lane,
        "tenantfds",
        Limits::default(),
        "ls /proc/$$/fd; cat /proc/$$/cgroup",
    )?
    .output;
    let held: Vec<&str> = said
        .lines()
        .take_while(|line| !line.starts_with("0::"))
        .collect();
    expect(
        held == ["0", "1", "2"],
        format!("only the standard streams, got {held:?}"),
    )?;
    expect(
        said.lines().any(|line| line == TENANT_CGROUP),
        format!("the shell runs in the tenant leaf, got {said:?}"),
    )
}

/// A runner that died after splitting a lease's cgroup leaves both leaves;
/// the next engine's boot sweep removes them, then the lease's cgroup.
pub(crate) fn sweep_removes_both_leaves(lane: &Lane) -> Result<(), Failed> {
    let lease_id = "leftover";
    let cgroup = lane.config.cgroup_root.join(lease_id);
    // What a runner killed here leaves: a split cgroup and the lease's
    // directory, with no sandbox to destroy either.
    drop(LeaseCgroup::create(
        &lane.config.cgroup_root,
        lease_id,
        &Limits::default(),
    )?);
    fs::create_dir_all(lane.lease_dir(lease_id))?;
    expect(
        cgroup.join(SANDBOX_LEAF).is_dir() && cgroup.join(TENANT_LEAF).is_dir(),
        "the split made both leaves",
    )?;

    drop(lane.engine());

    expect(
        !cgroup.exists(),
        format!("the sweep removed {}", cgroup.display()),
    )?;
    expect(
        !lane.lease_dir(lease_id).exists(),
        "the sweep removed the lease's directory",
    )
}

/// The workspace disk's loop device reads and writes its image past the
/// host's page cache.
pub(crate) fn workspace_disk_uses_direct_io(lane: &Lane) -> Result<(), Failed> {
    let lease_id = "directio";
    let dio = runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest {
                lease_id,
                limits: Limits::default(),
            })
            .await?;
        // Read while the sandbox holds the device: destroy detaches it.
        let dio = fs::metadata(lane.lease_dir(lease_id).join(DISK_MOUNT)).and_then(|meta| {
            let number = format!(
                "{}:{}",
                rustix::fs::major(meta.dev()),
                rustix::fs::minor(meta.dev())
            );
            fs::read_to_string(
                std::path::Path::new(SYS_DEV_BLOCK)
                    .join(number)
                    .join(LOOP_DIO),
            )
        });
        sandbox.destroy().await?;
        Ok::<_, Failed>(dio?)
    })?;
    expect(
        dio.trim() == DIO_ON,
        format!("the loop device reports direct I/O, got {dio:?}"),
    )
}

/// Writing twice the memory limit to `/workspace` ends at the disk's limit,
/// not in the out-of-memory killer: the image is cached once, in the
/// sandbox, rather than again on the host.
pub(crate) fn disk_fill_under_memory_limit_ends_in_enospc(lane: &Lane) -> Result<(), Failed> {
    let lease_id = "diskfill";
    let tenant_events = lane
        .config
        .cgroup_root
        .join(lease_id)
        .join(TENANT_LEAF)
        .join(MEMORY_EVENTS);
    let (filled, events) = runtime().block_on(async {
        let sandbox = lane
            .engine()
            .prepare(SandboxRequest {
                lease_id,
                limits: Limits::default(),
            })
            .await?;
        let filled = run_in(sandbox.executor(), shell(FILL_PAST_THE_DISK)).await;
        let events = fs::read_to_string(&tenant_events).unwrap_or_default();
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
fn killed_for_memory(ending: Ending) -> Result<(), Failed> {
    expect(
        ending == Ending::OutOfMemory,
        format!("the hog is killed for memory, got {ending:?}"),
    )
}
