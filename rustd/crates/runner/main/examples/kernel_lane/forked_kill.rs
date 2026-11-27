//! A tenant command whose shell forked the process the guest's kernel killed
//! for memory: the shell lives on and exits with the status it reports the
//! kill under, and that is still read as out of memory.

use std::fs;

use afr_engine::conformance::SMALL_MEMORY;
use afr_engine::{Engine as _, Limits, Network};
use libtest_mimic::Failed;

use crate::exhaustion::{NO_OOM_KILLS, killed_for_memory, monitor_events};
use crate::lane::{Lane, id_of, spec};
use crate::run::{expect, run as run_in, runtime, shell};

/// The sandbox the forked child is killed in.
const RUN: &str = "forkedhog";
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
/// executor reads as `out_of_memory`; the monitor's leaf counted no kill.
pub(crate) fn forked_oom(lane: &Lane) -> Result<(), Failed> {
    let limits = Limits {
        memory_bytes: SMALL_MEMORY,
        ..Limits::default()
    };
    let (hog, events) = runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(RUN);
        let sandbox = engine
            .prepare(spec(&engine, &id, limits, Network::Isolated)?)
            .await?;
        let hog = run_in(sandbox.executor(), shell(FORKED_HOG)).await;
        let events = fs::read_to_string(monitor_events(lane, RUN)).unwrap_or_default();
        sandbox.destroy().await?;
        Ok::<_, Failed>((hog?, events))
    })?;
    // The shell says `Killed` first, then the status it exits with.
    expect(
        hog.output.lines().last() == Some(SHELL_KILLED),
        format!("the shell lived to report its child's kill, got {hog:?}"),
    )?;
    killed_for_memory(hog.ending)?;
    expect(
        events.lines().any(|line| line == NO_OOM_KILLS),
        format!("the monitor was not killed, got {events:?}"),
    )
}
