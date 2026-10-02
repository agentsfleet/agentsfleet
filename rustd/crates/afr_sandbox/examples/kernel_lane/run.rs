//! Running one command inside a sandbox and reading what it said.

use std::time::Duration;

use afr_executor::{Ending, Executor, ProcessEvent, Spawn};
use afr_sandbox::{Engine, Limits, SandboxRequest};
use libtest_mimic::Failed;

use crate::lane::Lane;

/// How long one command may run before the trial calls it hung.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// What one command produced.
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
pub(crate) async fn run(executor: &dyn Executor, spawn: Spawn) -> Result<Outcome, String> {
    let mut process = executor
        .spawn(spawn)
        .await
        .map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    let gathered = tokio::time::timeout(COMMAND_TIMEOUT, async {
        while let Some(event) = process.events.recv().await {
            match event {
                ProcessEvent::Output { data, .. } => output.extend_from_slice(&data),
                ProcessEvent::Ended { ending, .. } => return Some(ending),
            }
        }
        None
    })
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
    runtime().block_on(async {
        let engine = lane.engine();
        let sandbox = engine
            .prepare(SandboxRequest { lease_id, limits })
            .await
            .map_err(|error| error.to_string())?;
        let outcome = run(sandbox.executor(), shell(script)).await;
        sandbox.destroy().await.map_err(|error| error.to_string())?;
        outcome.map_err(Failed::from)
    })
}

pub(crate) fn expect(holds: bool, why: impl Into<String>) -> Result<(), Failed> {
    if holds {
        Ok(())
    } else {
        Err(Failed::from(why.into()))
    }
}
