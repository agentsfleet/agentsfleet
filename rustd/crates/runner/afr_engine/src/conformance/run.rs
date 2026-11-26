//! Running one command inside a sandbox and reading what it said, and the
//! sandbox a trial builds and always destroys.

use std::time::Duration;

use afr_executor::{Ending, Executor, Spawn};
use libtest_mimic::Failed;

use crate::engine::{EgressPlan, Engine, Limits, Sandbox, SandboxSpec, fresh_sandbox_id};

/// How long one command may run before the trial calls it hung.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
/// What [`REACH_OUT`] prints when no connection could be opened.
pub const UNREACHABLE: &str = "unreachable";
/// Tries to open a connection outside loopback and prints whether it could.
pub const REACH_OUT: &str = "python3 -c 'import socket; \
     socket.create_connection((\"1.1.1.1\", 443), 3)' 2>/dev/null && echo reached || echo unreachable";
/// What the kernel says to a writer on a full disk.
pub const ENOSPC: &str = "No space left on device";
/// A workspace disk a limit trial fills past.
pub const SMALL_DISK: u64 = 64 * 1024 * 1024;
/// Memory a limit trial runs under.
pub const SMALL_MEMORY: u64 = 256 * 1024 * 1024;
/// Why a trial that ran two commands and got another count fails.
pub(super) const TWO_OUTCOMES: &str = "two outcomes";

/// Starts `sleep 600` in a session of its own, so only the sandbox's own
/// mechanism can end it, keeps its identifier in the workspace's
/// `.background`, where a later command finds it on whatever the sandbox
/// kept, and prints `started` once it
/// leads that session: the executor ends what is left in a command's group as
/// its shell exits, and `setsid` is in that group until it has made its own.
pub(super) const START_BACKGROUND: &str = "setsid sleep 600 >/dev/null 2>&1 </dev/null & \
     pid=$!; echo $pid > .background; tries=0; \
     until [ \"$(cut -d' ' -f6 /proc/$pid/stat 2>/dev/null)\" = \"$pid\" ] || [ \"$tries\" -ge 3000 ]; \
     do sleep 0.01; tries=$((tries + 1)); done; echo started";
/// What [`START_BACKGROUND`] prints once its process runs.
pub(super) const STARTED: &str = "started";
/// Prints `alive` while the process [`START_BACKGROUND`] started still runs,
/// by its command line, so a fresh environment reusing its number reads
/// `gone`; otherwise `gone`.
pub(super) const BACKGROUND_ALIVE: &str = "pid=$(cat .background); \
     tr '\\0' ' ' < /proc/$pid/cmdline 2>/dev/null | grep -q '^sleep 600' && echo alive || echo gone";
/// What [`BACKGROUND_ALIVE`] prints for a process still running.
pub(super) const ALIVE: &str = "alive";
/// What it prints for one that is not.
pub(super) const GONE: &str = "gone";

/// What one command produced.
#[derive(Debug)]
pub struct Outcome {
    /// How it ended.
    pub ending: Ending,
    /// Everything it said, both streams in arrival order.
    pub output: String,
}

/// Runs `spawn` to its end and gathers its output.
///
/// # Errors
/// It could not start, hung, or ended without saying how.
pub async fn run(executor: &dyn Executor, spawn: Spawn) -> Result<Outcome, Failed> {
    run_within(executor, spawn, COMMAND_TIMEOUT).await
}

/// Runs `spawn` to its end, calling it hung past `timeout`, and gathers its
/// output.
///
/// # Errors
/// It could not start, hung, or ended without saying how.
pub async fn run_within(
    executor: &dyn Executor,
    spawn: Spawn,
    timeout: Duration,
) -> Result<Outcome, Failed> {
    let process = executor.spawn(&spawn).await?;
    let mut output = Vec::new();
    let gathered = tokio::time::timeout(
        timeout,
        process.ended(|_stream, data| output.extend_from_slice(&data)),
    )
    .await;
    let ending = gathered
        .map_err(|_elapsed| "the command hung".to_owned())?
        .ok_or_else(|| "the process ended without saying how".to_owned())?;
    Ok(Outcome {
        ending,
        output: String::from_utf8_lossy(&output).into_owned(),
    })
}

/// `sh -c script`.
#[must_use]
pub fn shell(script: &str) -> Spawn {
    Spawn::program("/bin/sh").arg("-c").arg(script)
}

/// Nothing when `holds`, otherwise a failure saying `why`.
///
/// # Errors
/// `holds` is false.
pub fn expect(holds: bool, why: impl Into<String>) -> Result<(), Failed> {
    if holds {
        Ok(())
    } else {
        Err(Failed::from(why.into()))
    }
}

/// What `script` printed in `sandbox`, trimmed.
pub(super) async fn said(sandbox: &dyn Sandbox, script: &str) -> Result<String, Failed> {
    Ok(run(sandbox.executor(), shell(script))
        .await?
        .output
        .trim()
        .to_owned())
}

/// A fresh sandbox under `limits`, as `engine` grants them, reaching nothing.
pub(super) async fn build(engine: &dyn Engine, limits: Limits) -> Result<Box<dyn Sandbox>, Failed> {
    let id = fresh_sandbox_id();
    Ok(engine
        .prepare(SandboxSpec::granted_by(engine, &id, limits)?)
        .await?)
}

/// A fresh sandbox with the default grant, held to `plan`.
pub(super) async fn build_reaching(
    engine: &dyn Engine,
    plan: EgressPlan<'_>,
) -> Result<Box<dyn Sandbox>, Failed> {
    let id = fresh_sandbox_id();
    let spec = SandboxSpec::granted_by(engine, &id, Limits::DEFAULT)?.with_egress(plan);
    Ok(engine.prepare(spec).await?)
}

/// What `body` found in `sandbox`, which is destroyed whatever it found: a
/// trial's failure is told before its teardown's.
pub(super) async fn within<T>(
    sandbox: Box<dyn Sandbox>,
    body: impl AsyncFnOnce(&dyn Sandbox) -> Result<T, Failed>,
) -> Result<T, Failed> {
    let found = body(&*sandbox).await;
    let destroyed = sandbox.destroy().await;
    let found = found?;
    destroyed?;
    Ok(found)
}

/// What each of `scripts` printed, run in turn in one fresh sandbox under
/// `limits`: what a command does after another exhausted something is only
/// seen on the same sandbox.
pub(super) async fn in_sandbox_each(
    engine: &dyn Engine,
    limits: Limits,
    scripts: &[&str],
) -> Result<Vec<Outcome>, Failed> {
    within(build(engine, limits).await?, async |sandbox| {
        let mut outcomes = Vec::with_capacity(scripts.len());
        for script in scripts {
            outcomes.push(run(sandbox.executor(), shell(script)).await?);
        }
        Ok(outcomes)
    })
    .await
}

/// Starts the background process in `sandbox`.
pub(super) async fn start_background(sandbox: &dyn Sandbox) -> Result<(), Failed> {
    let started = said(sandbox, START_BACKGROUND).await?;
    expect(
        started == STARTED,
        format!("the background process starts, got {started:?}"),
    )
}
