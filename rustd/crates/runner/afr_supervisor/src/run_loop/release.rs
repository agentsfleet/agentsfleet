//! A run's sandbox at its end, when it is not parked: killed now, so the report
//! can post, and released once the report is in.

use std::time::Instant;

use af_core::error_code::{Coded as _, Logged};
use afr_engine::{Killed, Sandbox};
use afr_telemetry::labels::SandboxRelease;
use afr_telemetry::record;

use super::Run;
use super::park::Fate;

const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";
const EVENT_KILL_STARTED: &str = "sandbox_kill_started";
const EVENT_KILL_COMPLETED: &str = "sandbox_kill_completed";
const EVENT_KILL_UNSETTLED: &str = "sandbox_kill_unsettled";

impl Run<'_> {
    /// Kills `sandbox` now, for its release after the report. A kill that
    /// cannot prove the tree empty still lets the report post, and warns.
    pub(super) async fn kill(&self, sandbox: Box<dyn Sandbox>) -> Fate {
        let run_id = self.ids.run.as_str();
        let event = EVENT_KILL_STARTED;
        tracing::debug!(run_id, event);
        match sandbox.kill().await {
            Killed::Settled => {
                let event = EVENT_KILL_COMPLETED;
                tracing::debug!(run_id, event);
            }
            Killed::Unsettled(failure) => {
                let Logged { error_code, reason } = failure.logged();
                let event = EVENT_KILL_UNSETTLED;
                {
                    let (message,) = (format_args!(
                        "a sandbox's processes were not proved gone; the report posts anyway"
                    ),);
                    tracing::warn!(message, error_code, run_id, reason, event);
                };
            }
        }
        Fate::Killed(sandbox)
    }

    /// Releases what a killed sandbox held, after the report: its egress
    /// rules, cgroup, mount, image and directory. Timed; a failure leaves its
    /// directory for the boot sweep.
    pub(super) async fn release(&self, fate: Fate) {
        let started = Instant::now();
        let destroyed = match fate {
            Fate::Gone | Fate::Parked(_) => return,
            Fate::Killed(sandbox) => self.destroy(sandbox).await,
            Fate::Unfiled(parked) => self.logged_destroy(parked.destroy().await),
        };
        let outcome = match destroyed {
            Ok(()) => SandboxRelease::Released,
            Err(_logged) => SandboxRelease::Failed,
        };
        record::sandbox_release(outcome, started.elapsed());
    }

    /// Ends every process in `sandbox` and removes it, logging a teardown
    /// that left something behind.
    ///
    /// # Errors
    /// The engine's own, once logged: what is left is the boot sweep's.
    pub(super) async fn destroy(&self, sandbox: Box<dyn Sandbox>) -> afr_engine::Result<()> {
        self.logged_destroy(sandbox.destroy().await)
    }

    /// `destroyed`, logged when its teardown left something behind.
    fn logged_destroy(&self, destroyed: afr_engine::Result<()>) -> afr_engine::Result<()> {
        if let Err(failure) = &destroyed {
            let code = failure.code().as_str();
            let run_id = self.ids.run.as_str();
            let event = EVENT_DESTROY_FAILED;
            {
                let (error_code, message) =
                    (&code, format_args!("a sandbox did not tear down cleanly"));
                tracing::warn!(message, error_code, run_id, event);
            };
        }
        destroyed
    }
}

#[cfg(test)]
#[path = "tests/release.rs"]
mod release_tests;
