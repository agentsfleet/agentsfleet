//! The values a runner family's labels may take, and nothing else.
//!
//! Every label is a closed set, by construction rather than by review: a
//! provider is a name the provider registry ships or `_other`, a tool is a
//! published catalog name or `_other`, and every outcome and reason is an enum.
//! A label written from a string the model or the lease chose would be one
//! more series than the census ceiling admits, and the SDK would fold live
//! data into its overflow marker.

use std::collections::HashMap;
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

/// The provider table the runner dials, read here for its names alone.
///
/// The same file `afr_providers` routes from, so the label set is exactly the
/// set of providers a lease can name. That crate depends on this one, so the
/// table is read from its file rather than through its types.
const REGISTRY: &str = include_str!("../../afr_providers/assets/providers.json");

/// One registry entry, as far as a label needs it.
#[derive(serde::Deserialize)]
struct Named {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

/// Every provider name and alias, to the name it selects.
struct Names {
    /// The registry's names, in table order: the label's values.
    canonical: Vec<Box<str>>,
    /// Each name and alias, to the index of the name it selects.
    selects: HashMap<Box<str>, usize>,
}

impl Names {
    /// The names `table` declares. A table that will not parse declares
    /// none, which labels every provider `_other`; `afr_providers` refuses
    /// boot on the same file long before a label is written.
    fn read(table: &str) -> Self {
        let entries: Vec<Named> = serde_json::from_str(table).unwrap_or_default();
        let mut canonical = Vec::with_capacity(entries.len());
        let mut selects = HashMap::new();
        for (index, entry) in entries.into_iter().enumerate() {
            for alias in entry.aliases {
                selects.insert(alias.into_boxed_str(), index);
            }
            let name = entry.name.into_boxed_str();
            selects.insert(name.clone(), index);
            canonical.push(name);
        }
        Self { canonical, selects }
    }

    /// The registry's name for `configured`, when it names one.
    fn label(&'static self, configured: &str) -> Option<&'static str> {
        let index = *self.selects.get(configured)?;
        self.canonical.get(index).map(AsRef::as_ref)
    }
}

/// The registry's names, read once.
static NAMES: LazyLock<Names> = LazyLock::new(|| Names::read(REGISTRY));

/// A model provider, as a label: the registry's name for it, an alias folded
/// into the name it selects, or [`OTHER`] for a `custom:` endpoint.
///
/// Not the `gen_ai.provider.name` the `invoke_agent` span carries. That
/// attribute is OpenTelemetry's well-known vocabulary, which names six of the
/// registry's providers; labelled by it, every other provider a runner dials
/// would share one `_other` series and a slow one could not stand out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider(&'static str);

impl Provider {
    /// How many values this label can take: every registered provider, and
    /// [`OTHER`].
    #[must_use]
    pub fn count() -> usize {
        NAMES.canonical.len() + 1
    }

    /// The label for the provider a lease configured.
    #[must_use]
    pub fn of(configured: &str) -> Self {
        Self(NAMES.label(configured).unwrap_or(OTHER))
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
