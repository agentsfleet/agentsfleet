//! Running one command inside a sandbox and reading what it said.

use std::time::Duration;

use afr_executor::{Ending, Executor, Spawn};
use afr_sandbox::{Engine, Limits, SandboxRequest};
use libtest_mimic::Failed;

use crate::lane::Lane;

/// How long one command may run before the trial calls it hung.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
/// What [`REACH_OUT`] prints when no connection could be opened.
pub(crate) const UNREACHABLE: &str = "unreachable";
/// Tries to open a connection outside loopback and prints whether it could.
pub(crate) const REACH_OUT: &str = "python3 -c 'import socket; \
     socket.create_connection((\"1.1.1.1\", 443), 3)' 2>/dev/null && echo reached || echo unreachable";

/// What one command produced.
#[derive(Debug)]
pub(crate) struct Outcome {
    pub(crate) ending: Ending,
    pub(crate) output: String,
}

/// A single-threaded runtime: the trials drive one sandbox at a time.
pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| unreachable!("a runtime always builds here: {error}"))
}

/// Runs `spawn` to its end and gathers its output.
pub(crate) async fn run(executor: &dyn Executor, spawn: Spawn) -> Result<Outcome, Failed> {
    run_within(executor, spawn, COMMAND_TIMEOUT).await
}

/// Runs `spawn` to its end, calling it hung past `timeout`, and gathers its
/// output.
pub(crate) async fn run_within(
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
pub(crate) fn shell(script: &str) -> Spawn {
    Spawn::program("/bin/sh").arg("-c").arg(script)
}

/// Runs `script` in a fresh sandbox with `limits`, then destroys it.
pub(crate) fn in_sandbox(
    lane: &Lane,
    lease_id: &str,
    limits: Limits,
    script: &str,
) -> Result<Outcome, Failed> {
    in_sandbox_each(lane, lease_id, limits, &[script])?
        .pop()
        .ok_or_else(|| Failed::from("one script runs once"))
}

/// Runs each of `scripts` in turn in one fresh sandbox with `limits`, then
/// destroys it: what the executor does after a command exhausted something
/// is only seen on the same sandbox.
pub(crate) fn in_sandbox_each(
    lane: &Lane,
    lease_id: &str,
    limits: Limits,
    scripts: &[&str],
) -> Result<Vec<Outcome>, Failed> {
    runtime().block_on(async {
        let engine = lane.engine();
        let sandbox = engine.prepare(SandboxRequest { lease_id, limits }).await?;
        let mut outcomes = Vec::with_capacity(scripts.len());
        for script in scripts {
            match run(sandbox.executor(), shell(script)).await {
                Ok(outcome) => outcomes.push(outcome),
                Err(failed) => {
                    sandbox.destroy().await?;
                    return Err(failed);
                }
            }
        }
        sandbox.destroy().await?;
        Ok(outcomes)
    })
}

pub(crate) fn expect(holds: bool, why: impl Into<String>) -> Result<(), Failed> {
    if holds {
        Ok(())
    } else {
        Err(Failed::from(why.into()))
    }
}
