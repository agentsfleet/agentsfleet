//! Running one command inside a sandbox and reading what it said.

use afr_engine::{Engine as _, Limits, Network};
use libtest_mimic::Failed;

pub(crate) use afr_engine::conformance::{
    Outcome, REACH_OUT, UNREACHABLE, expect, run, run_within, shell,
};

use crate::lane::{Lane, id_of, spec};

/// A single-threaded runtime: the trials drive one sandbox at a time.
pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| unreachable!("a runtime always builds here: {error}"))
}

/// Runs `script` in a fresh sandbox the trial names `name`, under `limits`
/// and reaching nothing, then destroys it.
pub(crate) fn in_sandbox(
    lane: &Lane,
    name: &str,
    limits: Limits,
    script: &str,
) -> Result<Outcome, Failed> {
    in_sandbox_each(lane, name, limits, Network::Isolated, &[script])?
        .pop()
        .ok_or_else(|| Failed::from("one script runs once"))
}

/// Runs each of `scripts` in turn in one fresh sandbox the trial names
/// `name`, under `limits` and reaching `network`, then destroys it: what the
/// guest agent does after a command exhausted something is only seen on the
/// same sandbox.
pub(crate) fn in_sandbox_each(
    lane: &Lane,
    name: &str,
    limits: Limits,
    network: Network<'_>,
    scripts: &[impl AsRef<str>],
) -> Result<Vec<Outcome>, Failed> {
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(name);
        let sandbox = engine.prepare(spec(&engine, &id, limits, network)?).await?;
        let mut outcomes = Vec::with_capacity(scripts.len());
        for script in scripts {
            match run(sandbox.agent(), shell(script.as_ref())).await {
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

/// What each outcome printed, trimmed.
pub(crate) fn said(outcomes: Vec<Outcome>) -> Vec<String> {
    outcomes
        .into_iter()
        .map(|outcome| outcome.output.trim().to_owned())
        .collect()
}
