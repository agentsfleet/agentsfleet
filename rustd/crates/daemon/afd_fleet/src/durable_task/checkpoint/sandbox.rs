//! The sandbox a checkpoint records, as its two columns hold it: the
//! engine's wire spelling and the digest of the image its guest agent booted
//! from. Turns and durable tasks store it the same way.

use af_wire::durable_task::CheckpointSandbox;
use af_wire::runner::SandboxEngine;
use serde::Deserialize as _;

use crate::{Error, Result};

/// The columns `sandbox` is stored as; both absent when no sandbox ran.
pub(crate) fn columns(sandbox: Option<&CheckpointSandbox>) -> (Option<&'static str>, Option<&str>) {
    sandbox.map_or((None, None), |sandbox| {
        (
            Some(sandbox.engine.into()),
            sandbox.guest_agent_digest.as_deref(),
        )
    })
}

/// The sandbox `engine` and `guest_agent_digest` record; none when no engine
/// is recorded.
///
/// # Errors
/// `incompatible`, when the engine is not one this build names: the
/// checkpoint restores nowhere it runs.
pub(crate) fn recorded(
    engine: Option<&str>,
    guest_agent_digest: Option<&str>,
    incompatible: impl Into<Error>,
) -> Result<Option<CheckpointSandbox>> {
    let Some(engine) = engine else {
        return Ok(None);
    };
    let engine = SandboxEngine::deserialize(serde::de::value::StrDeserializer::<
        serde::de::value::Error,
    >::new(engine))
    .map_err(|_unknown| incompatible.into())?;
    Ok(Some(CheckpointSandbox {
        engine,
        guest_agent_digest: guest_agent_digest.map(str::to_owned),
    }))
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
