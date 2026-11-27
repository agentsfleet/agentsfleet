//! The pause a boundary's save holds, and the release that ends its pin.
//!
//! ```text
//!   no call in flight ─► freeze tenant ─► pin ─► thaw tenant   the run waits
//!   changes ─► register ─► seal ─► PUT ─► pages ─► commit ─► release   it does not
//! ```
//!
//! A live tenant process no longer skips the save: the pause freezes it
//! (`WalkVolume`). A call still in flight does, before any freeze or pin.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use afr_agent::RecoverySkip;
use afr_engine::{PausedWork, Resumed, Sandbox, WorkspaceVolume};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};

use crate::staging::FREEZE_TENANT;
use crate::{Result, error, recovery::ChangeSource};

/// The stage a file walk pins at: the workspace read as files, each changed
/// one copied under the pause.
const STAGE: u8 = 1;

/// How a boundary's pause ended.
pub(crate) enum Pinning<P> {
    /// No pause was taken, for this reason.
    Skipped(RecoverySkip),
    /// The pin, taken under a pause that has ended.
    Pinned(Resumed<P>),
}

/// Proves no call of `sandbox`'s guest agent in flight, then pauses `volume`,
/// pins it against `parent` and resumes it, the resume running whether or
/// not the pin was taken. Nothing here calls the daemon.
///
/// # Errors
/// The engine could not tell whether a tenant process runs.
pub(crate) async fn pin_sandbox<V: WorkspaceVolume>(
    sandbox: &dyn Sandbox,
    volume: &V,
    parent: Option<&Uuid7>,
) -> Result<Pinning<V::Pin>> {
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
    match afr_engine::pin_paused(volume, parent).await {
        Ok(pin) => {
            let engine: &str = sandbox.environment().engine.into();
            let paused_ms = u64::try_from(paused.elapsed().as_millis()).unwrap_or(u64::MAX);
            let PausedWork {
                entries,
                changed_bytes,
            } = volume.paused_work(&pin);
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
pub(crate) async fn release<V: WorkspaceVolume>(
    volume: &V,
    pin: Resumed<V::Pin>,
    committed: Option<&Uuid7>,
) {
    if let Err(failure) = volume.release(pin, committed).await {
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
pub(crate) struct Pinned<'a, V: WorkspaceVolume> {
    pub(crate) volume: &'a V,
    pub(crate) pin: &'a Resumed<V::Pin>,
}

impl<V: WorkspaceVolume> ChangeSource for Pinned<'_, V> {
    fn changes(&self) -> BoxStream<'_, Result<Change>> {
        self.volume
            .changes(self.pin)
            .map_err(|source| error::boundary("a pin's changes could not be read", source))
            .boxed()
    }

    fn parent(&self) -> Option<&Uuid7> {
        self.volume.parent(self.pin)
    }
}

#[cfg(test)]
#[path = "tests/pin_pause.rs"]
mod pause_tests;
