//! A tenant command whose shell forked the process the kernel killed for
//! memory: the shell lives on and exits with the status it reports the kill
//! under, and that is still read as out of memory.

use std::fs;

use afr_executor::Ending;
use afr_sandbox::{Engine as _, Limits, SANDBOX_LEAF, SandboxRequest, TENANT_LEAF};
use libtest_mimic::Failed;

use crate::exhaustion::{MEMORY_EVENTS, NO_OOM_KILLS, SMALL_MEMORY};
use crate::lane::Lane;
use crate::run::{expect, run as run_in, runtime, shell};

/// The lease the forked child is killed in.
const LEASE: &str = "forkedhog";
/// A child the shell forks and waits for, allocating and touching far more
/// than [`SMALL_MEMORY`]; the shell prints the status it saw, then exits
/// with it.
const FORKED_HOG: &str =
    // pin test: literal is the contract
    "python3 -c 'b = bytearray(2 * 1024 ** 3)'; status=$?; echo \"$status\"; exit \"$status\"";
/// What a POSIX shell reports a child killed by `SIGKILL` as: 128 + 9.
// pin test: literal is the contract
const SHELL_KILLED: &str = "137";

/// The child is killed for memory and its shell exits 137, which the
/// executor reads as `out_of_memory` because the tenant leaf counted the
/// kill; the sandbox leaf counted none.
pub(crate) fn forked_oom(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let events = |leaf: &str| {
        let path = lane.config.cgroup_root.join(LEASE).join(leaf);
        fs::read_to_string(path.join(MEMORY_EVENTS)).unwrap_or_default()
    };
    let (hog, sandbox_events, tenant_events) = runtime().block_on(async {
        let request = SandboxRequest {
            lease_id: LEASE,
            limits,
        };
        let sandbox = lane.engine().prepare(request).await?;
        let hog = run_in(sandbox.executor(), shell(FORKED_HOG)).await;
        let counted = (events(SANDBOX_LEAF), events(TENANT_LEAF));
        sandbox.destroy().await?;
        Ok::<_, Failed>((hog?, counted.0, counted.1))
    })?;
    // The shell says `Killed` first, then the status it exits with.
    expect(
        hog.output.lines().last() == Some(SHELL_KILLED),
        format!("the shell lived to report its child's kill, got {hog:?}"),
    )?;
    expect(
        hog.ending == Ending::OutOfMemory,
        format!("its exit reads as out of memory, got {:?}", hog.ending),
    )?;
    expect(
        !tenant_events.lines().any(|line| line == NO_OOM_KILLS),
        format!("the tenant leaf counted the kill, got {tenant_events:?}"),
    )?;
    expect(
        sandbox_events.lines().any(|line| line == NO_OOM_KILLS),
        format!("nothing in the sandbox leaf was killed, got {sandbox_events:?}"),
    )
}
