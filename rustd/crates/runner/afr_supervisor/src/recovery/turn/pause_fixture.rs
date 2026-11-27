//! What the pause tests drive a save through: the workspace's own volume
//! with each step recorded, and a native owner that records each command.

use super::super::capture_tests::{Checked, Remote, snapshot};
use super::super::scope;
use crate::recovery::NativeTurn;
use crate::run_loop::pin::{Pinned as PinSource, Pinning, pin_sandbox, release};
use crate::staging::{ExportPin, ExportVolume, Staging};
use crate::test_support::FakeEngine;
use crate::{error, test_support};
use af_chunk_store::Change;
use af_core::id::Uuid7;
use af_wire::run::{TurnCheckpointAction as Action, TurnCheckpointCommand, TurnResume};
use afr_engine::{Paused, Resumed, Sandbox, WorkspaceVolume};
use futures_util::stream::BoxStream;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// One ordered record of everything a save did, both sides of the pause.
#[derive(Clone, Default)]
pub(super) struct Log(Arc<Mutex<Vec<&'static str>>>);

impl Log {
    pub(super) fn push(&self, step: &'static str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(step);
    }

    pub(super) fn steps(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The workspace's own volume, each step it takes recorded in `log`, with the
/// sandbox's quiesce drained in ahead of the pause it precedes.
pub(super) struct Recording<'a> {
    inner: ExportVolume<'a>,
    log: Log,
    steps: Mutex<mpsc::UnboundedReceiver<&'static str>>,
}

impl<'a> Recording<'a> {
    /// The volume of `sandbox` staged under `staging`, recording into `log`.
    pub(super) fn over(
        sandbox: &'a dyn Sandbox,
        staging: &'a Staging,
        steps: mpsc::UnboundedReceiver<&'static str>,
        log: &Log,
        stop: &CancellationToken,
    ) -> Self {
        Self {
            inner: ExportVolume::new(sandbox, staging, stop.clone()),
            log: log.clone(),
            steps: Mutex::new(steps),
        }
    }

    fn drain_sandbox(&self) {
        let mut steps = self.steps.lock().unwrap_or_else(PoisonError::into_inner);
        while let Ok(step) = steps.try_recv() {
            self.log.push(step);
        }
    }
}

#[async_trait::async_trait]
impl WorkspaceVolume for Recording<'_> {
    type Pin = ExportPin;

    async fn pause(&self) -> afr_engine::Result<()> {
        self.drain_sandbox();
        self.log.push("pause");
        self.inner.pause().await
    }

    async fn pin(&self, paused: &Paused, parent: Option<&Uuid7>) -> afr_engine::Result<ExportPin> {
        let pin = self.inner.pin(paused, parent).await;
        self.log.push("pin");
        pin
    }

    async fn resume(&self) -> afr_engine::Result<()> {
        self.log.push("resume");
        self.inner.resume().await
    }

    fn changes<'a>(
        &'a self,
        pin: &'a Resumed<ExportPin>,
    ) -> BoxStream<'a, afr_engine::Result<Change>> {
        self.inner.changes(pin)
    }

    async fn release(&self, pin: Resumed<ExportPin>, committed: bool) -> afr_engine::Result<()> {
        self.log.push(if committed {
            "release"
        } else {
            "release uncommitted"
        });
        self.inner.release(pin, committed).await
    }
}

/// A native owner that records each command's kind in `log`, then answers
/// as `remote`.
pub(super) fn recorded_plane(
    remote: Arc<Mutex<Remote>>,
    log: Log,
) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let result = (|| {
            let body = call
                .body
                .as_ref()
                .ok_or_else(|| error::config("missing command"))?;
            let request: TurnCheckpointCommand =
                serde_json::from_slice(body).map_err(error::encode)?;
            log.push(match &request.action {
                Action::Begin(_) => "begin",
                Action::Objects(_) => "register",
                Action::Uploads(_) => "seal",
                Action::PageWrite(_) => "pages",
                Action::Commit(_) => "commit",
                Action::PinRenew(_) => "renew",
                Action::Abandon(_) => "abandon",
                _other => "other",
            });
            remote
                .lock()
                .map_err(|_poisoned| error::config("the fixture owner panicked"))?
                .respond(request)
        })();
        match result {
            Ok(response) => test_support::json(&response),
            Err(failure) => test_support::Answer::Fail(failure),
        }
    })
}

/// A sandbox serving `root`, whose steps arrive on the receiver.
pub(super) async fn sandbox(
    root: &std::path::Path,
) -> Checked<(Box<dyn Sandbox>, mpsc::UnboundedReceiver<&'static str>)> {
    let (steps, recorded) = mpsc::unbounded_channel();
    let engine = FakeEngine {
        workspace: Some(root.to_owned()),
        toolbox: Some("b".repeat(64)),
        steps: Some(steps),
        ..FakeEngine::default()
    };
    Ok((engine.sandbox().await?, recorded))
}

/// The native owner's state, held.
pub(super) fn owner(remote: &Mutex<Remote>) -> Checked<std::sync::MutexGuard<'_, Remote>> {
    Ok(remote.lock().map_err(|_poisoned| "the owner panicked")?)
}

/// The pin files the staging directory at `root` holds.
pub(super) fn pin_files(root: &std::path::Path) -> Checked<usize> {
    let mut found = 0;
    for entry in std::fs::read_dir(root)? {
        if entry?.path().extension().is_some_and(|ext| ext == "pin") {
            found += 1;
        }
    }
    Ok(found)
}

/// Pins `volume` over `sandbox` against `parent`, or fails the test.
pub(super) async fn pinned(
    sandbox: &dyn Sandbox,
    volume: &Recording<'_>,
    parent: Option<&Uuid7>,
) -> Checked<Resumed<ExportPin>> {
    match pin_sandbox(sandbox, volume, parent).await? {
        Pinning::Pinned(pin) => Ok(pin),
        Pinning::Skipped(reason) => Err(format!("the save skipped as {reason:?}").into()),
    }
}

/// One save's pause and publication, as the boundary owner runs them.
pub(super) async fn save(
    native: &mut NativeTurn<'_>,
    sandbox: &dyn Sandbox,
    volume: &Recording<'_>,
    stop: &CancellationToken,
) -> Checked<crate::Result<TurnResume>> {
    let parent = native
        .head()
        .map(|head| Uuid7::parse(&head.checkpoint_id.0))
        .transpose()?;
    let pin = pinned(sandbox, volume, parent.as_ref()).await?;
    let source = PinSource { volume, pin: &pin };
    let saved = native.capture(snapshot(scope()), Some(&source), stop).await;
    release(volume, pin, saved.is_ok()).await;
    Ok(saved)
}
