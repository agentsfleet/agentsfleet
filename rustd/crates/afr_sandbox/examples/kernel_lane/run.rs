//! Running one command inside a sandbox and reading what it said.

use std::time::Duration;

use afr_executor::{Ending, Executor, ProcessEvent, Spawn};

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
