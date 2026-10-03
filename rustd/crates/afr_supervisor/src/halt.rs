//! How the runner stops, at three depths, and the one place each is decided.
//!
//! ```text
//!   stop_leasing  take no new lease; the heartbeat and leases in flight go on
//!   shutdown      (the caller's token) also stop beating; leases in flight
//!                 still run to their reports
//!   stop          also end leases in flight: the daemon said stop, or it
//!                 refused this runner's token and no call can succeed
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;

use crate::error::Error;

const EVENT_TOKEN_REFUSED: &str = "runner_token_refused";
const EVENT_LEASING_STOPPED: &str = "runner_leasing_stopped";

/// The runner's stop signals.
#[derive(Debug)]
pub(crate) struct Halt {
    serving: CancellationToken,
    leasing: CancellationToken,
    running: CancellationToken,
    refused: AtomicBool,
}

impl Halt {
    /// Signals under the caller's `shutdown`, which stops serving and leasing.
    pub(crate) fn new(shutdown: CancellationToken) -> Self {
        Self {
            leasing: shutdown.child_token(),
            serving: shutdown,
            running: CancellationToken::new(),
            refused: AtomicBool::new(false),
        }
    }

    /// Cancelled once the runner should stop beating and polling.
    pub(crate) const fn serving(&self) -> &CancellationToken {
        &self.serving
    }

    /// Cancelled once the runner should take no new lease.
    pub(crate) const fn leasing(&self) -> &CancellationToken {
        &self.leasing
    }

    /// Cancelled once leases in flight should end.
    pub(crate) const fn running(&self) -> &CancellationToken {
        &self.running
    }

    /// Ends everything, leases in flight included.
    pub(crate) fn stop(&self) {
        self.running.cancel();
        self.serving.cancel();
    }

    /// Takes no new lease; whatever is running finishes.
    pub(crate) fn stop_leasing(&self) {
        let code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
        let event = EVENT_LEASING_STOPPED;
        tracing::warn!(error_code = code, event, "this runner takes no new lease");
        self.leasing.cancel();
    }

    /// Stops the runner when `failure` is the daemon refusing its token, and
    /// returns whether it did. Every path that talks to the daemon asks this,
    /// so the decision is made in one place.
    pub(crate) fn stops_on(&self, failure: &Error) -> bool {
        if !failure.is_unauthorized() {
            return false;
        }
        if !self.refused.swap(true, Ordering::SeqCst) {
            let code = failure.code().as_str();
            let event = EVENT_TOKEN_REFUSED;
            tracing::error!(
                error_code = code,
                event,
                "the daemon refused this runner's token"
            );
        }
        self.stop();
        true
    }

    /// Whether the runner stopped because its token was refused.
    pub(crate) fn token_refused(&self) -> bool {
        self.refused.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[path = "halt/tests.rs"]
mod tests;
