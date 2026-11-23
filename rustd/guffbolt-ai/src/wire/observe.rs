//! What a turn tells its embedder as it goes: content-free timings, counts
//! and retries, never a prompt, a reply, a URL or a key.
//!
//! The wires log nothing themselves. An embedder that wants a log line, a
//! metric or a trace for any of these writes it in its [`Observer`].

use std::time::Duration;

use super::retry::Retrying;
use crate::Error;

/// Where a turn's observations go. Every method does nothing unless the
/// embedder writes it.
pub trait Observer: Send + Sync {
    /// A turn's stream was opened.
    fn turn_started(&self) {}

    /// The turn's first nonempty text arrived, this long after its opening.
    fn first_text(&self, _after: Duration) {}

    /// The turn on the model named ended, once, including cancellation and failed
    /// streams.
    fn turn_ended(&self, _model: &str, _turn: &TurnObservation) {}

    /// The input a turn was measured at, beside the input the provider
    /// reported for it.
    fn input_measured(&self, _calibration: &Calibration<'_>) {}

    /// A send is about to be retried.
    fn retrying(&self, _retry: Retrying) {}

    /// A turn the provider ended early, before anything visible went out, is
    /// opened again, after the failure of the opening numbered from 1.
    fn reopened(&self, _attempt: usize, _failure: &Error) {}
}

/// Content-free classification; the original error remains in the turn's
/// result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// The caller's hard deadline or the shared inactivity bound elapsed.
    Deadline,
    /// The caller cancelled the turn.
    Cancelled,
    /// The transport or provider ended the response unsuccessfully.
    Provider,
}

impl FailureCause {
    /// A bounded diagnostic word, independent of untrusted provider messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deadline => "deadline",
            Self::Cancelled => "cancelled",
            Self::Provider => "provider_interrupted",
        }
    }
}

/// The tokens a provider reported for one request; a count it left out stays
/// unknown rather than zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObservedTokens {
    /// Prompt tokens, cache reads and writes included.
    pub input: Option<u64>,
    /// Completion tokens.
    pub output: Option<u64>,
    /// Prompt tokens read from the provider's cache.
    pub cache_read: Option<u64>,
    /// Prompt tokens written to the provider's cache.
    pub cache_write: Option<u64>,
}

/// Timing for one provider stream request; usage recovery is excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnObservation {
    /// What the provider reported, partial usage included.
    pub tokens: ObservedTokens,
    /// Request-to-terminal duration.
    pub elapsed: Duration,
    /// First nonempty text, absent for tool-only or failed-before-text calls.
    pub first_text: Option<Duration>,
    /// The stream's terminal failure, if any.
    pub cause: Option<FailureCause>,
    /// The bytes of answer text the turn streamed.
    pub text_bytes: usize,
}

/// One turn's local input measurement beside the provider's own count, so an
/// estimate can be calibrated. Digests and closed words only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration<'a> {
    /// A SHA-256 digest of the route and model the measurement was for.
    pub route_id: &'a str,
    /// A SHA-256 digest of the model id.
    pub model_id: &'a str,
    /// The route's API, in its snake-case name.
    pub provider_api: &'static str,
    /// `messages`, or `tools` when the request offered tools.
    pub request_kind: &'static str,
    /// How the measurement was taken: `tokenizer_estimate`,
    /// `o200k_proxy_estimate` or `provider_count`.
    pub method: &'static str,
    /// The measuring code's revision.
    pub revision: &'static str,
    /// The input the measurement allowed for.
    pub measured_input_tokens: u64,
    /// The input the provider reported.
    pub actual_input_tokens: u64,
    /// How long the local measurement took.
    pub measure_us: u64,
}
