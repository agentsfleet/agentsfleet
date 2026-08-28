//! What a gate answer means to the admission pass.
//!
//! A mapping in its own module for two reasons. It is a statement about THIS
//! pass's vocabulary, so the gate does not have to know what a lease does with
//! its answer — and it is the one part of the pass whose whole behaviour is a
//! total function over a small enum, which means it can be proven exhaustively
//! without a datastore. Everything else in `admit` needs one.

use crate::lease::admit::{Admission, Refusal, Transient};
use afd_gate::gate::{Refused, Verdict};

/// The gate that stopped a fleet, for a transient's `at`.
const STOPPED: &str = "gate_auto_kill";

/// The gate that could not be raised, for a transient's `at`.
const GATE: &str = "approval_gate";

impl Admission {
    /// What a gate [`Verdict`] means for the admission pass.
    ///
    /// `None` is the one answer that CONTINUES the pass — every other arm ends
    /// it — which is what lets the caller write `if let Some(stop) = …` beside
    /// the money gates that answer the same shape.
    ///
    /// [`Verdict::Killed`] becomes a [`Admission::Retry`] and not a refusal,
    /// which reads backwards until you see what was stopped: the FLEET changed
    /// state, and the event was written nothing, so a resume re-delivers it.
    /// Ending the event as well would punish one delivery for a fleet-level
    /// decision it happens to have triggered.
    #[must_use]
    pub const fn of_gate(verdict: Verdict) -> Option<Self> {
        match verdict {
            // The one arm that continues the pass rather than ending it.
            Verdict::Pass => None,
            Verdict::Await(waiting) => Some(Self::Await(waiting)),
            Verdict::Refuse(Refused::Denied) => Some(Self::Refuse(Refusal::labelled(
                afd_core::event::label::APPROVAL_DENIED,
            ))),
            Verdict::Refuse(Refused::Expired) => Some(Self::Refuse(Refusal::labelled(
                afd_core::event::label::APPROVAL_EXPIRED,
            ))),
            Verdict::Killed(_) => Some(Self::Retry(Transient { at: STOPPED })),
            Verdict::Unavailable => Some(Self::Retry(Transient { at: GATE })),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lease::admit::Admission;
    use afd_gate::gate::{Refused, Trigger, Verdict, Waiting};

    /// Every verdict the gate can answer, so the table below is walked in full
    /// rather than sampled — a new arm shows up here as a missing row.
    const EVERY_VERDICT: [Verdict; 8] = [
        Verdict::Pass,
        Verdict::Await(Waiting::Parked),
        Verdict::Await(Waiting::Pending),
        Verdict::Await(Waiting::Unreadable),
        Verdict::Refuse(Refused::Denied),
        Verdict::Refuse(Refused::Expired),
        Verdict::Killed(Trigger::Anomaly),
        Verdict::Unavailable,
    ];

    #[test]
    fn only_a_passing_gate_continues_the_pass() {
        // The property the `Option` carries: exactly one verdict lets the lease
        // go on being admitted, and every other one stops it. An arm that
        // answered `None` by mistake would admit an event a human refused.
        for verdict in EVERY_VERDICT {
            let stops = Admission::of_gate(verdict).is_some();
            assert_eq!(stops, verdict != Verdict::Pass, "{verdict:?}");
        }
    }

    #[test]
    fn only_a_human_decision_ends_the_event() {
        // A terminal row is a one-way door, so which verdicts write one is the
        // sharpest question in this mapping. Two do: a refusal and a lapse —
        // both of them a human's answer, one given and one not given in time.
        let ends: Vec<_> = EVERY_VERDICT
            .into_iter()
            .filter(|verdict| matches!(Admission::of_gate(*verdict), Some(Admission::Refuse(_))))
            .collect();

        assert_eq!(
            ends,
            vec![
                Verdict::Refuse(Refused::Denied),
                Verdict::Refuse(Refused::Expired)
            ]
        );
    }

    #[test]
    fn a_refusal_carries_the_label_its_dashboard_greps() {
        // These strings are read by the webhook path, the steer path and the
        // dashboard, so a second spelling would silently stop matching.
        assert_eq!(
            Admission::of_gate(Verdict::Refuse(Refused::Denied)),
            Some(Admission::Refuse(crate::lease::admit::Refusal::labelled(
                afd_core::event::label::APPROVAL_DENIED
            )))
        );
        assert_eq!(
            Admission::of_gate(Verdict::Refuse(Refused::Expired)),
            Some(Admission::Refuse(crate::lease::admit::Refusal::labelled(
                afd_core::event::label::APPROVAL_EXPIRED
            )))
        );
    }

    #[test]
    fn a_stopped_fleet_does_not_end_the_delivery_that_stopped_it() {
        // The arm that reads backwards until you see what was stopped: the
        // FLEET changed state, and this delivery was written nothing, so a
        // resume re-delivers it. A refusal here would punish one event for a
        // fleet-level decision it happened to trigger — and the event would be
        // gone when an operator resumed the fleet to run it.
        for trigger in [Trigger::Anomaly, Trigger::Policy] {
            assert!(
                matches!(
                    Admission::of_gate(Verdict::Killed(trigger)),
                    Some(Admission::Retry(_))
                ),
                "{trigger:?}"
            );
        }
    }

    #[test]
    fn waiting_on_a_human_is_not_a_retry() {
        // Durably identical — no row, delivery leasable — and a separate arm
        // anyway, because a queue waiting on people and a datastore falling
        // over must not land in one graph. This is the assertion that keeps a
        // later "simplification" from collapsing them.
        for waiting in [Waiting::Parked, Waiting::Pending, Waiting::Unreadable] {
            assert_eq!(
                Admission::of_gate(Verdict::Await(waiting)),
                Some(Admission::Await(waiting)),
                "{waiting:?}"
            );
        }
        // And an unraisable gate IS a retry: nothing was decided, so the next
        // poll asks again.
        assert!(matches!(
            Admission::of_gate(Verdict::Unavailable),
            Some(Admission::Retry(_))
        ));
    }
}
