//! What a finished run hands the supervisor: its verdict, its output and its
//! numbers.
//!
//! Runner-internal. The loop builds these and the supervisor folds them into
//! the report it sends, so nothing here crosses the wire and none of it
//! derives serde.

use std::borrow::Cow;

use afd_wire::report::FailureClass;

/// A clean finish. Empty by construction: the run's numbers live on the result
/// itself, shared by both verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Completed;

/// Why a run failed.
///
/// `class` is `None` only when the failure was never classified. A cause is
/// never guessed from a bare failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure<'a> {
    /// The classified cause, when there is one.
    pub class: Option<FailureClass>,
    /// Human-readable cause from the classification site.
    pub detail: Cow<'a, str>,
}

/// The run's verdict. `Completed` carries no cause because a clean run has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultOutcome<'a> {
    /// The run finished cleanly.
    Completed(Completed),
    /// The run failed.
    Failed(Failure<'a>),
}

/// The terminal stage result the runner produces and the report consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult<'a> {
    /// Whether the run finished or failed, and why.
    pub outcome: ResultOutcome<'a>,
    /// The run's output.
    pub content: Cow<'a, str>,
    /// Total tokens, for reporting rather than billing.
    pub token_count: u64,
    /// Wall-clock seconds the run took.
    pub wall_seconds: u64,
    /// Peak resident bytes observed.
    pub memory_peak_bytes: u64,
    /// Milliseconds the run spent throttled.
    pub cpu_throttled_ms: u64,
    /// Cumulative prompt tokens for the whole run.
    pub input_tokens: u64,
    /// Cumulative cache-read tokens for the whole run.
    pub cached_input_tokens: u64,
    /// Cumulative completion tokens for the whole run.
    pub output_tokens: u64,
}
