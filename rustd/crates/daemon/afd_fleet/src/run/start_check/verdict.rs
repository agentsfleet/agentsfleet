//! What a gate answer means to the start check.
//!
//! A mapping in its own module for two reasons. It is a statement about THIS
//! pass's vocabulary, so the gate does not have to know what a run does with
//! its answer — and it is the one part of the pass whose whole behaviour is a
//! total function over a small enum, which means it can be proven exhaustively
//! without a datastore. Everything else in `start_check` needs one.

use crate::run::start_check::{Refusal, StartCheck, Transient};
use afd_gate::gate::{Refused, Verdict};

/// The gate that could not be raised, for a transient's `at`.
const GATE: &str = "approval_gate";

impl StartCheck {
    /// What a gate [`Verdict`] means for the start check.
    ///
    /// `None` is the one answer that CONTINUES the pass — every other arm ends
    /// it — which is what lets the caller write `if let Some(stop) = …` beside
    /// the money gates that answer the same shape.
    #[must_use]
    pub const fn of_gate(verdict: Verdict) -> Option<Self> {
        match verdict {
            // The one arm that continues the pass rather than ending it.
            Verdict::Pass => None,
            Verdict::Await(waiting) => Some(Self::Await(waiting)),
            Verdict::Refuse(Refused::Denied) => Some(Self::Refuse(Refusal::labelled(
                af_core::turn::label::APPROVAL_DENIED,
            ))),
            Verdict::Unavailable => Some(Self::Retry(Transient { at: GATE })),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::run::start_check::StartCheck;
    use afd_gate::gate::{Refused, Verdict, Waiting};

    /// Every verdict the gate can answer, so the table below is walked in full
    /// rather than sampled — a new arm shows up here as a missing row.
    const EVERY_VERDICT: [Verdict; 6] = [
        Verdict::Pass,
        Verdict::Await(Waiting::Parked),
        Verdict::Await(Waiting::Pending),
        Verdict::Await(Waiting::Unreadable),
        Verdict::Refuse(Refused::Denied),
        Verdict::Unavailable,
    ];

    #[test]
    fn only_a_passing_gate_continues_the_pass() {
        // The property the `Option` carries: exactly one verdict lets the run
        // go on toward its start, and every other one stops it. An arm that
        // answered `None` by mistake would start an event a human refused.
        for verdict in EVERY_VERDICT {
            let stops = StartCheck::of_gate(verdict).is_some();
            assert_eq!(stops, verdict != Verdict::Pass, "{verdict:?}");
        }
    }

    #[test]
    fn only_a_terminal_answer_ends_the_event() {
        // A terminal row is a one-way door, so which verdicts write one is the
        // sharpest question in this mapping. One does: a human's refusal. A
        // lapsed deadline writes none: the event waits for a late answer.
        let ends: Vec<_> = EVERY_VERDICT
            .into_iter()
            .filter(|verdict| matches!(StartCheck::of_gate(*verdict), Some(StartCheck::Refuse(_))))
            .collect();

        assert_eq!(ends, vec![Verdict::Refuse(Refused::Denied)]);
    }

    #[test]
    fn a_refusal_carries_the_label_its_dashboard_greps() {
        // These strings are read by the webhook path, the steer path and the
        // dashboard, so a second spelling would silently stop matching.
        assert_eq!(
            StartCheck::of_gate(Verdict::Refuse(Refused::Denied)),
            Some(StartCheck::Refuse(
                crate::run::start_check::Refusal::labelled(af_core::turn::label::APPROVAL_DENIED)
            ))
        );
    }

    #[test]
    fn waiting_on_a_human_is_not_a_retry() {
        // Durably identical — no row, delivery leasable — and a separate arm
        // anyway, because a queue waiting on people and a datastore falling
        // over must not land in one graph. This is the assertion that keeps a
        // later "simplification" from collapsing them.
        for waiting in [Waiting::Parked, Waiting::Pending, Waiting::Unreadable] {
            assert_eq!(
                StartCheck::of_gate(Verdict::Await(waiting)),
                Some(StartCheck::Await(waiting)),
                "{waiting:?}"
            );
        }
        // And an unraisable gate IS a retry: nothing was decided, so the next
        // poll asks again.
        assert!(matches!(
            StartCheck::of_gate(Verdict::Unavailable),
            Some(StartCheck::Retry(_))
        ));
    }
}
