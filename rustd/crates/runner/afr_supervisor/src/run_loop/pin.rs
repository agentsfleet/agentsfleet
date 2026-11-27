//! The pause a boundary's save holds, and the release that ends its pin.
//!
//! ```text
//!   executor idle ─► tenant quiet ─► pause ─► pin ─► resume   the run waits
//!   changes ─► register ─► seal ─► PUT ─► pages ─► commit ─► release   it does not
//! ```

use af_chunk_store::Change;
use af_core::id::Uuid7;
use afr_agent::RecoverySkip;
use afr_engine::{Quiescence, Resumed, Sandbox, WorkspaceVolume};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};

use crate::{Result, error, recovery::ChangeSource};

/// How a boundary's pause ended.
pub(crate) enum Pinning<P> {
    /// No pause was taken, for this reason.
    Skipped(RecoverySkip),
    /// The pin, taken under a pause that has ended.
    Pinned(Resumed<P>),
}

/// Proves `sandbox`'s executor and tenant idle, then pauses `volume`, pins it
/// against `parent` and resumes it. Nothing here calls the daemon.
///
/// # Errors
/// The engine could not tell whether a tenant process runs.
pub(crate) async fn pin_sandbox<V: WorkspaceVolume>(
    sandbox: &dyn Sandbox,
    volume: &V,
    parent: Option<&Uuid7>,
) -> Result<Pinning<V::Pin>> {
    match executor_idle(sandbox).await {
        Ok(true) => {}
        Ok(false) => return Ok(Pinning::Skipped(RecoverySkip::BusyExecutor)),
        Err(failure) => {
            tracing::debug!(event = "run_checkpoint_executor_unproved", error_code = %failure.code());
            return Ok(Pinning::Skipped(RecoverySkip::Unavailable));
        }
    }
    let quiescence = sandbox
        .quiesce()
        .await
        .map_err(|source| error::boundary("cannot prove live capture tenant quiescence", source))?;
    if quiescence == Quiescence::Busy {
        return Ok(Pinning::Skipped(RecoverySkip::BusyProcesses));
    }
    if sandbox.environment().root.toolbox().is_none() {
        return Ok(Pinning::Skipped(RecoverySkip::Unavailable));
    }
    let paused = tokio::time::Instant::now();
    match afr_engine::pin_paused(volume, parent).await {
        Ok(pin) => {
            let engine: &str = sandbox.environment().engine.into();
            let paused_ms = u64::try_from(paused.elapsed().as_millis()).unwrap_or(u64::MAX);
            tracing::info!(event = "checkpoint_paused", engine, paused_ms);
            Ok(Pinning::Pinned(pin))
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

/// Releases `pin`, as committed only once its publication answered.
pub(crate) async fn release<V: WorkspaceVolume>(volume: &V, pin: Resumed<V::Pin>, committed: bool) {
    if let Err(failure) = volume.release(pin, committed).await {
        let error_code = failure.code();
        tracing::warn!(event = "checkpoint_pin_release_failed", %error_code);
    }
}

/// The executor's own file jobs must be idle before a capture reads the
/// workspace.
pub(super) async fn executor_idle(sandbox: &dyn Sandbox) -> Result<bool> {
    sandbox
        .executor()
        .recovery_idle()
        .await
        .map_err(|source| error::boundary("cannot prove executor file jobs idle", source))
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
}
