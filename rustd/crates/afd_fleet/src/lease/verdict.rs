//! What a runner said happened, in the shape the terminal row needs.
//!
//! Split from [`super::finalize`] on the seam between deciding and writing: this
//! file turns three loosely-related wire fields into one value that cannot
//! contradict itself, and the module next door writes rows from it.
//!
//! # The trust boundary is structural, not checked
//!
//! `ReportRequest` carries an [`Outcome`], an `Option<FailureClass>` and a
//! detail string as three INDEPENDENT fields, so the wire permits a `processed`
//! run that also names a `timeout_kill` and explains itself. A runner cannot be
//! trusted not to send one — and validating it would mean a check every reader
//! has to remember to perform.
//!
//! [`Verdict`] removes the possibility instead. `Succeeded` has nowhere to put
//! a cause, so the contradiction is not rejected at a boundary; it cannot be
//! constructed at all, and every consumer downstream gets the guarantee for
//! free.

use afd_wire::report::{FailureClass, Outcome};

use afd_events::sql::MAX_FAILURE_DETAIL_BYTES;

/// The runner's verdict, in the shape the terminal row needs.
///
/// A two-variant enum where the wire carries an [`Outcome`] beside an
/// `Option<FailureClass>` and a detail string that are only meaningful on one
/// of them. The conversion is where the trust boundary becomes structural: a
/// cause cannot accompany a clean run, because [`Verdict::Succeeded`] has
/// nowhere to put one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict<'a> {
    /// The run finished and produced a result.
    Succeeded,
    /// The run failed, with the cause the classification site knew.
    Failed {
        /// The classified cause, when the runner classified one.
        class: Option<FailureClass>,
        /// The human-readable cause. Empty when none was given.
        detail: &'a str,
    },
}

impl<'a> Verdict<'a> {
    /// The verdict a report carries.
    ///
    /// `report_mapping.fromReport`'s job, and the reason it is a conversion
    /// rather than three fields read separately: the wire permits a
    /// `processed` outcome alongside a populated `failure_reason`, and this is
    /// the one place that decides such a report is a clean run. A runner
    /// cannot contradict itself into a row that says both.
    #[must_use]
    pub const fn of(outcome: Outcome, class: Option<FailureClass>, detail: &'a str) -> Self {
        match outcome {
            Outcome::Processed => Self::Succeeded,
            Outcome::FleetError => Self::Failed { class, detail },
        }
    }

    /// Whether the run finished cleanly — what the lifetime tally counts.
    #[must_use]
    pub const fn succeeded(self) -> bool {
        matches!(self, Self::Succeeded)
    }

    /// The status the event's terminal row takes.
    pub(super) const fn status(self) -> &'static str {
        match self {
            Self::Succeeded => afd_core::event::status::PROCESSED,
            Self::Failed { .. } => afd_core::event::status::FLEET_ERROR,
        }
    }

    /// The stored `failure_label`, which a clean run has none of.
    pub(super) fn label(self) -> Option<&'static str> {
        match self {
            Self::Succeeded => None,
            Self::Failed { class, .. } => match class {
                Some(class) => Some(class_label(class)),
                None => None,
            },
        }
    }

    /// The stored `failure_detail`, capped, which a clean run has none of.
    pub(super) fn detail(self) -> Option<&'a str> {
        match self {
            // A clean run has no cause, and a failure that named none stores
            // NULL rather than an empty string — so a consumer testing
            // `IS NULL` cannot be fooled by a runner that sent `""`.
            Self::Succeeded | Self::Failed { detail: "", .. } => None,
            Self::Failed { detail, .. } => Some(truncate(detail, MAX_FAILURE_DETAIL_BYTES)),
        }
    }
}

/// The stored spelling of one failure class.
///
/// `FailureClass.label()` in the Zig. Derived from the wire enum's own serde
/// renaming rather than restated, so a rename fails to compile instead of
/// writing rows nothing queries — the device [`crate::sql::event_type`] uses.
fn class_label(class: FailureClass) -> &'static str {
    match class {
        FailureClass::StartupPosture => "startup_posture",
        FailureClass::PolicyDeny => "policy_deny",
        FailureClass::TimeoutKill => "timeout_kill",
        FailureClass::OomKill => "oom_kill",
        FailureClass::ResourceKill => "resource_kill",
        FailureClass::RunnerCrash => "runner_crash",
        FailureClass::TransportLoss => "transport_loss",
        FailureClass::LandlockDeny => "landlock_deny",
        FailureClass::LeaseExpired => "lease_expired",
        FailureClass::RenewalTerminate => "renewal_terminate",
        // Spelled identically to the gate's own refusal label, which is why it
        // is imported rather than written out: one string for the ceiling hit
        // at issue and the ceiling hit mid-run, so an operator greps once.
        FailureClass::BudgetBreach => afd_core::event::label::BUDGET_BREACH,
    }
}

/// `text` capped to `max` bytes, never splitting a character.
///
/// [`str::is_char_boundary`] asks directly what `truncateUtf8` infers by
/// masking `0xC0` off each byte and walking back over the continuations. The
/// two agree on every input — a boundary is precisely a byte that is not a
/// continuation — but only one of them says so, and the hand-rolled version
/// sits twenty lines from a comment explaining UTF-8 to its reader.
///
/// The walk is bounded by three: no character encodes to more than four bytes,
/// so at most three continuations precede a boundary.
pub(super) fn truncate(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// What the terminal row records about a finished run.
///
/// A struct rather than four more parameters on [`Leases::mark_terminal`],
/// which would take eight. Two of them are same-typed `i64` measurements a
/// transposition would compile straight through and store as each other — the
/// same hazard [`crate::sql::lease::LeaseRow`] groups its binds against.
#[derive(Debug, Clone, Copy)]
pub struct Terminal<'a> {
    /// Whether the run finished, and why it did not.
    pub verdict: Verdict<'a>,
    /// The run's output, stored on both arms: a failure's CAUSE lives in
    /// `failure_detail`, and duplicating it here is how a consumer rendering
    /// the answer shows an error message as though the fleet had said it.
    pub response_text: &'a str,
    /// The run's total tokens, for reporting rather than billing.
    pub tokens: i64,
    /// Wall-clock milliseconds the run took.
    pub wall_ms: i64,
}

#[cfg(test)]
#[path = "verdict/tests.rs"]
mod tests;
