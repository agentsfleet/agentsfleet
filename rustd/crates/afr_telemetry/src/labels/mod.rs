//! The values a runner family's labels may take, and nothing else.
//!
//! Every label is a closed set, by construction rather than by review: a
//! provider is a bounded SDK vendor name or `_other`, a tool is a
//! published catalog name or `_other`, and every outcome and reason is an enum.
//! A label written from a string the model or the lease chose would be one
//! more series than the census ceiling admits, and the SDK would fold live
//! data into its overflow marker.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use afr_tools::catalog;

/// The label key a provider is attributed under.
pub const LABEL_PROVIDER: &str = "provider";

/// The label key a tool is attributed under.
pub const LABEL_TOOL: &str = "tool";

/// The spelling every set with a failure member gives it, so one query
/// matches a failed turn, a failed sandbox start and a failed call alike.
const FAILED: &str = "failed";

/// The spelling for a request that never reached the other side, shared by
/// a retried send and a lost push.
const TRANSPORT: &str = "transport";

/// The value everything past a closed set is attributed under: the spelling
/// the daemon's runner table already uses for "past the set", so one panel
/// reads both.
pub const OTHER: &str = afd_observability::runner::OVERFLOW_RUNNER;

/// Bounded metric labels follow the SDK vocabulary, independently of persisted routes.
static NAMES: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| {
    rig_core::providers::registry::ProviderId::all()
        .map(|id| id.vendor())
        .collect()
});

/// A bounded SDK vendor name, or [`OTHER`] for a workspace's own provider name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider(&'static str);

impl Provider {
    /// How many values this label can take: every registered provider, and
    /// [`OTHER`].
    #[must_use]
    pub fn count() -> usize {
        NAMES.len() + 1
    }

    /// The label for the provider a lease configured.
    #[must_use]
    pub fn of(configured: &str) -> Self {
        Self(NAMES.get(configured).copied().unwrap_or(OTHER))
    }

    /// The label value, byte-exact.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// A tool, as a label: its published catalog name, or [`OTHER`] for a name
/// the model made up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool(&'static str);

impl Tool {
    /// How many values this label can take: every published tool, and
    /// [`OTHER`].
    pub const COUNT: usize = catalog::PUBLISHED.len() + 1;

    /// The label for a call the model made to `called`.
    #[must_use]
    pub fn of(called: &str) -> Self {
        Self(catalog::published(called).map_or(OTHER, |entry| entry.name()))
    }

    /// The label value, byte-exact.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

afd_observability::closed_set! {
    /// How one provider turn ended.
    TurnOutcome {
        /// The provider streamed the turn to its end.
        Completed => "completed",
        /// The provider failed the turn, and the run with it.
        Failed => FAILED,
        /// The lease stopped the turn before it ended.
        Stopped => "stopped",
    }
}

afd_observability::closed_set! {
    /// Why a turn's send was retried.
    RetryReason {
        /// The provider answered 429.
        RateLimited => "rate_limited",
        /// The provider answered 5xx.
        ServerError => "server_error",
        /// The send could not connect or timed out.
        Transport => TRANSPORT,
        /// The stream broke after it opened and the turn was opened again.
        StreamReopened => "stream_reopened",
    }
}

impl RetryReason {
    /// The reason a send that failed with `status` was retried; none for a
    /// send that never got a status.
    #[must_use]
    pub const fn of_status(status: Option<u16>) -> Self {
        match status {
            Some(TOO_MANY_REQUESTS) => Self::RateLimited,
            Some(_server_error) => Self::ServerError,
            None => Self::Transport,
        }
    }
}

/// The status a provider rate-limits with.
const TOO_MANY_REQUESTS: u16 = 429;

afd_observability::closed_set! {
    /// How a sandbox start ended.
    SandboxStart {
        /// The sandbox's executor answered.
        Ready => "ready",
        /// The host could not build it, and the lease ended at startup.
        Failed => FAILED,
    }
}

afd_observability::closed_set! {
    /// What became of a sandbox held for its fleet between two leases: held,
    /// reused, or released for the reason given.
    SandboxHold {
        /// A processed lease's sandbox was frozen and held.
        Parked => "parked",
        /// The fleet's next lease took it, and it thawed and answered.
        Reused => "reused",
        /// Its idle window ran out.
        Expired => "expired",
        /// The runner's last free worker took a lease for another fleet.
        Saturated => "saturated",
        /// The runner held as many as it has workers, and this was the oldest.
        Capped => "capped",
        /// The next lease wanted another size or policy.
        Mismatch => "mismatch",
        /// The daemon named its fleet: halted, deleted, or leased by another
        /// runner since.
        Inactive => "inactive",
        /// The runner takes no new lease, so no lease could take it: leasing
        /// stopped, or the runner is shutting down or stopped.
        Shutdown => "shutdown",
        /// It would not thaw, or its executor did not answer once thawed.
        ThawFailed => "thaw_failed",
        /// It no longer carries the fleet's latest run: the daemon refused
        /// the report of the lease that left it, as settled without it or for
        /// good, or never received it; the fleet's next lease was not told to
        /// resume it; a newer park of the same fleet replaced it; or the lease
        /// that asked for it stopped waiting before taking it.
        Superseded => "superseded",
    }
}

afd_observability::closed_set! {
    /// Why live-tail frames never left the runner.
    FrameDrop {
        /// Every batch slot was held, so a full batch was dropped.
        Backpressure => "backpressure",
        /// The daemon did not take the batch, and activity is not retried.
        PostFailed => "post_failed",
        /// The lease ended while the daemon was still slow to take the live
        /// tail, and what was still held went with the lease.
        Abandoned => "abandoned",
    }
}

afd_observability::closed_set! {
    /// Why a memory push did not land.
    PushFailure {
        /// The daemon answered 5xx or 429 past the retries.
        Upstream => "upstream",
        /// The daemon refused it with a 4xx; a stale fence is one.
        Refused => "refused",
        /// The push never reached the daemon.
        Transport => TRANSPORT,
        /// Anything else: a body that would not encode, a reply that would not
        /// decode.
        Internal => "internal",
    }
}

afd_observability::closed_set! {
    /// How one tool call ended, as its trace row says.
    ///
    /// The wire's `ToolCallStatus`, mirrored member for member on purpose: a
    /// label set is a census promise, and a status added to the wire must not
    /// become a new series before the census says it may.
    ToolOutcome {
        /// The tool ran and returned what it was asked for.
        Succeeded => "succeeded",
        /// The tool ran and reported an error, or exited non-zero.
        Failed => FAILED,
        /// The run ended before the call did.
        Interrupted => "interrupted",
    }
}

#[cfg(test)]
mod tests;
