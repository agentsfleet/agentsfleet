//! The pause a boundary's save holds, and the release that ends its pin.
//!
//! ```text
//!   no call in flight ─► freeze tenant ─► pin ─► thaw tenant   the run waits
//!   changes ─► register ─► seal ─► PUT ─► pages ─► commit ─► release   it does not
//! ```
//!
//! A live tenant process no longer skips the save: the pause freezes it
//! (`afr_engine::quiesce_workspace`, through the sandbox's snapshots where
//! its engine offers them). A call still in flight does, before any freeze
//! or pin.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use afr_agent::RecoverySkip;
use afr_engine::{FREEZE_TENANT, PausedWork, Quiesced, Sandbox};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};

use crate::{Result, error, recovery::ChangeSource};

/// The stage a file walk pins at: the workspace read as files, each changed
/// one copied under the pause.
const STAGE: u8 = 1;

/// How a boundary's pause ended.
pub(crate) enum Pinning<'a> {
    /// No pause was taken, for this reason.
    Skipped(RecoverySkip),
    /// The workspace, held under a pause that has ended.
    Pinned(Box<dyn Quiesced + 'a>),
}

/// Proves no call of `sandbox`'s guest agent in flight, then runs
/// `quiesce`, which pauses the workspace, pins it and resumes it, the resume
/// running whether or not the pin was taken. Nothing here calls the daemon.
///
/// # Errors
/// The engine could not tell whether a tenant process runs.
pub(crate) async fn pin_sandbox<'a>(
    sandbox: &dyn Sandbox,
    quiesce: impl Future<Output = afr_engine::Result<Box<dyn Quiesced + 'a>>>,
) -> Result<Pinning<'a>> {
    match sandbox.agent().calls_idle().await {
        Ok(true) => {}
        Ok(false) => return Ok(Pinning::Skipped(RecoverySkip::BusyAgent)),
        Err(failure) => {
            tracing::debug!(event = "run_checkpoint_agent_unproved", error_code = %failure.code());
            return Ok(Pinning::Skipped(RecoverySkip::Unavailable));
        }
    }
    // Read for its refusal alone: a running tenant process is frozen by the
    // pause, or refuses it where nothing can freeze it.
    let _running = sandbox
        .quiesce()
        .await
        .map_err(|source| error::boundary("cannot prove live capture tenant quiescence", source))?;
    if sandbox.environment().root.toolbox().is_none() {
        return Ok(Pinning::Skipped(RecoverySkip::Unavailable));
    }
    let paused = tokio::time::Instant::now();
    match quiesce.await {
        Ok(pin) => {
            let engine: &str = sandbox.environment().engine.into();
            let paused_ms = u64::try_from(paused.elapsed().as_millis()).unwrap_or(u64::MAX);
            let PausedWork {
                entries,
                changed_bytes,
            } = pin.paused_work();
            tracing::info!(
                event = "checkpoint_paused",
                engine,
                stage = STAGE,
                entries,
                changed_bytes,
                paused_ms
            );
            Ok(Pinning::Pinned(pin))
        }
        Err(failure) if failure.unsupported_operation() == Some(FREEZE_TENANT) => {
            Ok(Pinning::Skipped(RecoverySkip::BusyProcesses))
        }
        // A workspace that cannot be read out skips this save, as one the
        // host could not scan did: the run goes on.
        Err(failure) => {
            let error_code = failure.code();
            tracing::warn!(event = "run_checkpoint_export_failed", %error_code);
            Ok(Pinning::Skipped(RecoverySkip::Unavailable))
        }
    }
}

/// Releases `pin`, naming the generation its publication committed, once
/// the daemon answered that commit.
pub(crate) async fn release(pin: Box<dyn Quiesced + '_>, committed: Option<&Uuid7>) {
    if let Err(failure) = pin.release(committed).await {
        let error_code = failure.code();
        tracing::warn!(event = "checkpoint_pin_release_failed", %error_code);
    }
}

/// The guest agent's own file jobs must be idle before a capture reads the
/// workspace.
pub(super) async fn agent_idle(sandbox: &dyn Sandbox) -> Result<bool> {
    sandbox
        .agent()
        .recovery_idle()
        .await
        .map_err(|source| error::boundary("cannot prove guest agent file jobs idle", source))
}

/// A pin read as the changes a publication consumes.
pub(crate) struct Pinned<'a>(pub(crate) &'a dyn Quiesced);

impl ChangeSource for Pinned<'_> {
    fn changes(&self) -> BoxStream<'_, Result<Change>> {
        self.0
            .changes()
            .map_err(|source| error::boundary("a pin's changes could not be read", source))
            .boxed()
    }

    fn parent(&self) -> Option<&Uuid7> {
        self.0.parent()
    }
}

#[cfg(test)]
#[path = "tests/pin_pause.rs"]
mod pause_tests;
