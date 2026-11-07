//! The expiry timer can interrupt a slow upload before cleanup runs.

use super::{Spool, files};
use crate::{Result, client::ControlPlane, halt::Halt};
use afd_core::id::Uuid7;
use std::{path::PathBuf, time::Duration};
use tokio::time::Instant;

const RETRY: Duration = Duration::from_secs(1);
const EVENT_CLEANUP_FAILED: &str = "authoring_spool_cleanup_failed";
const EVENT_REPORT_WAITING: &str = "authoring_report_waiting";
const EVENT_REPORT_REFUSED: &str = "authoring_report_refused";

impl Spool {
    pub(crate) async fn run(&self, plane: &ControlPlane, halt: &Halt) {
        loop {
            let scanning = self.clone();
            let pending = tokio::task::spawn_blocking(move || scanning.scan())
                .await
                .map_err(crate::Error::from)
                .and_then(std::convert::identity)
                .map(|scanned| {
                    report_held(scanned.held);
                    scanned.pending
                });
            let deadline = if pending.is_ok() {
                self.deadline()
            } else {
                Instant::now() + RETRY
            };
            if let Ok(pending) = pending {
                // Each entry settles, waits or is dropped on its own: one
                // failure never holds back the entries behind it.
                let sending = async {
                    for path in pending {
                        if let Err(failure) = self.deliver(plane, path).await {
                            if halt.stops_on(&failure) {
                                return true;
                            }
                            waiting(&failure, EVENT_REPORT_WAITING);
                        }
                    }
                    false
                };
                tokio::select! {
                    () = halt.serving().cancelled() => return,
                    () = self.0.changed.notified() => continue,
                    () = tokio::time::sleep_until(deadline) => continue,
                    stopped = sending => if stopped { return; },
                }
            } else if let Err(failure) = pending {
                waiting(&failure, EVENT_CLEANUP_FAILED);
            }
            tokio::select! {
                () = halt.serving().cancelled() => return,
                () = self.0.changed.notified() => {},
                () = tokio::time::sleep_until(deadline.min(Instant::now() + RETRY)) => {},
            }
        }
    }

    /// Sends one entry. A settled reply removes it; a refusal the daemon would
    /// repeat word for word removes it too, so it cannot retry for the whole
    /// private retention window.
    async fn deliver(&self, plane: &ControlPlane, path: PathBuf) -> Result<()> {
        let (lease, report) = self.load(path.clone()).await?;
        let lease = Uuid7::parse(&lease)?;
        match plane.authoring_report(&lease, &report).await {
            Ok(reply) if !reply.settled => return Ok(()),
            Ok(_settled) => {}
            Err(refusal) if permanent(&refusal) => {
                let error_code = refusal.code().as_str();
                let (lease_id, report_id) = (lease.as_str(), &report.accounting().report_id);
                let event = EVENT_REPORT_REFUSED;
                tracing::error!(
                    error_code,
                    lease_id,
                    report_id,
                    event,
                    "the daemon refused this report for good; it is dropped"
                );
            }
            Err(failure) => return Err(failure),
        }
        tokio::task::spawn_blocking(move || files::remove(&path)).await??;
        Ok(())
    }

    fn deadline(&self) -> Instant {
        self.0
            .state
            .lock()
            .ok()
            .and_then(|state| state.deadlines.values().copied().min())
            .unwrap_or_else(|| Instant::now() + RETRY)
    }

    /// Startup and tests use the same cleanup boundary as online delivery.
    pub(crate) async fn sweep(&self) -> Result<()> {
        let scanning = self.clone();
        report_held(
            tokio::task::spawn_blocking(move || scanning.scan())
                .await??
                .held,
        );
        Ok(())
    }
}

/// A daemon refusal no later attempt could change, by the lease report
/// spool's own rule.
fn permanent(failure: &crate::Error) -> bool {
    failure.refusal_status().is_some() && !super::super::kept(failure)
}

/// Logs each entry whose cleanup failed: held, content untouched and unsent,
/// and retried next pass while its neighbours continue.
pub(super) fn report_held(held: Vec<(PathBuf, crate::Error)>) {
    for (path, failure) in held {
        let error_code = failure.code().as_str();
        let lease_id = path.file_name().and_then(|name| name.to_str());
        let event = EVENT_CLEANUP_FAILED;
        tracing::warn!(
            error_code,
            lease_id,
            event,
            "private report cleanup failed; the entry is held"
        );
    }
}

fn waiting(failure: &crate::Error, event: &'static str) {
    let error_code = failure.code().as_str();
    tracing::warn!(
        error_code,
        event,
        "private report delivery waits for recovery"
    );
}
