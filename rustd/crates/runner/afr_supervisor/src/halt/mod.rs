//! How the runner stops, at three depths, and the one place each is decided.
//!
//! ```text
//!   stop_running  take no new run; the heartbeat and runs in flight go on
//!   shutdown      (the caller's token) also stop beating; runs in flight
//!                 retain work and report delivery for the supervisor's bounded grace
//!   stop          also end runs in flight: the daemon said stop, or it
//!                 refused this runner's token and no call can succeed
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;

use crate::error::Error;

const EVENT_TOKEN_REFUSED: &str = "runner_token_refused";

/// The runner's stop signals.
#[derive(Debug)]
pub(crate) struct Halt {
    serving: CancellationToken,
    run_plane: CancellationToken,
    running: CancellationToken,
    stopped: CancellationToken,
    refused: AtomicBool,
}

impl Halt {
    /// Signals under the caller's `shutdown`, which stops serving and running.
    pub(crate) fn new(shutdown: CancellationToken) -> Self {
        Self {
            run_plane: shutdown.child_token(),
            serving: shutdown,
            running: CancellationToken::new(),
            stopped: CancellationToken::new(),
            refused: AtomicBool::new(false),
        }
    }

    /// Cancelled once the runner should stop beating and taking offers.
    pub(crate) const fn serving(&self) -> &CancellationToken {
        &self.serving
    }

    /// Cancelled once the runner should take no new run.
    pub(crate) const fn run_plane(&self) -> &CancellationToken {
        &self.run_plane
    }

    /// Cancelled once runs in flight should end.
    pub(crate) const fn running(&self) -> &CancellationToken {
        &self.running
    }

    /// Cancelled when the owner ends delivery as well as work.
    pub(crate) const fn stopped(&self) -> &CancellationToken {
        &self.stopped
    }

    /// Ends everything, runs in flight included.
    pub(crate) fn stop(&self) {
        self.stopped.cancel();
        self.running.cancel();
        self.serving.cancel();
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
            {
                let (error_code, message) = (
                    &code,
                    format_args!("the daemon refused this runner's token"),
                );
                tracing::error!(message, error_code, event);
            };
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
mod tests;
