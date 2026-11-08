//! The values a provider measurement's labels may take, and nothing else.
//!
//! Every label is a closed set: a provider is a bounded public vendor name or
//! `_other`, and every outcome and reason is an enum. A label written from a
//! string a lease chose would be one more series than the census admits.

/// Terminal outcome of one provider stream request.
pub use af_observability::metrics::label::models::CallOutcome;

/// The label key a provider is attributed under.
pub const LABEL_PROVIDER: &str = "provider";

/// The label key a decision's purpose is attributed under.
pub const LABEL_PURPOSE: &str = "purpose";

/// The value everything past a closed set is attributed under: the spelling
/// the daemon's runner table already uses for "past the set", so one panel
/// reads both.
pub const OTHER: &str = af_observability::runner::OVERFLOW_RUNNER;

/// The status a provider rate-limits with.
const TOO_MANY_REQUESTS: u16 = 429;

/// A canonical public vendor name, or [`OTHER`] for a workspace's own provider name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider(&'static str);

impl Provider {
    /// How many values this label can take: every canonical public provider,
    /// and [`OTHER`].
    #[must_use]
    pub fn count() -> usize {
        af_observability::semconv::provider::count() + 1
    }

    /// The label for the provider a lease configured.
    #[must_use]
    pub fn of(configured: &str) -> Self {
        Self(af_observability::semconv::provider::normalize(configured).unwrap_or(OTHER))
    }

    /// The label value, byte-exact.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

af_observability::closed_set! {
    /// Why a turn's send was retried.
    RetryReason {
        /// The provider answered 429.
        RateLimited => "rate_limited",
        /// The provider answered 5xx.
        ServerError => "server_error",
        /// The send could not connect or timed out.
        Transport => "transport",
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

af_observability::closed_set! {
    /// Native decision protocol, independently of coding-provider vendor names.
    DecisionProvider {
        /// `TypeSafe` System One.
        Typesafe => "typesafe",
        /// `OpenAI` Decisions.
        Openai => "openai",
        /// Perplexity Decisions.
        Perplexity => "perplexity",
        /// Cloudflare Clef.
        Cloudflare => "cloudflare",
    }
}

af_observability::closed_set! {
    /// The native runtime's reason for evaluating evidence.
    DecisionPurpose {
        /// Interpret the newest input.
        Intent => "intent",
        /// Assess progress against the accepted objective.
        Goal => "goal",
    }
}

af_observability::closed_set! {
    /// What became of one bounded evaluation, including a denied preflight.
    DecisionOutcome {
        /// Complete typed answers arrived.
        Completed => "completed",
        /// Provider refused a required question.
        Refused => "refused",
        /// Reported model was not accepted by the frozen profile.
        InvalidModel => "invalid_model",
        /// Required accounting was missing or inconsistent.
        InvalidUsage => "invalid_usage",
        /// Required answers were incomplete or inconsistent.
        InvalidAnswers => "invalid_answers",
        /// Input failed before transport began.
        NotSent => "not_sent",
        /// The provider requested slower traffic.
        RateLimited => "rate_limited",
        /// A different provider status refused the call.
        ProviderFailure => "provider_failure",
        /// The physical send or complete response exceeded its time bound.
        Timeout => "timeout",
        /// The transport ended without a confirmed result.
        Unknown => "unknown",
        /// Response bytes exceeded their bound.
        Oversize => "oversize",
        /// Response bytes were not JSON.
        Malformed => "malformed",
        /// The caller stopped a dispatched evaluation.
        Cancelled => "cancelled",
    }
}
