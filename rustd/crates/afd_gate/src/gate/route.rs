//! The ordering decision, isolated from its I/O.
//!
//! The gate reads two things — whether this event already has a recorded gate,
//! and what the fleet's policy currently says — and the ORDER in which those
//! two bind is a security property rather than a style choice. This module is
//! that order, as a pure function over two small enums, so it is pinned by unit
//! tests instead of by a live Redis and Postgres.

use crate::gate::Decision;

/// The recorded-gate lookup outcome, without its payload.
///
/// `Unreadable` stays distinct from `Absent` because collapsing them is unsafe
/// in BOTH directions: absent means this event was never parked, unreadable
/// means we cannot tell — and raising a SECOND approval card for an event that
/// may already hold one is worse than waiting a poll. A three-arm enum is what
/// keeps a caller from spelling that as `Option` and losing the middle case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefState {
    /// This event already has a recorded gate.
    Found,
    /// It has none.
    Absent,
    /// The lookup failed and we cannot tell.
    Unreadable,
}

/// What the caller does once the lookup and the policy have both spoken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The recorded gate decides; policy is not consulted at all.
    EvaluateRecorded,
    /// The event runs.
    Pass,
    /// The fleet is paused and the event ends.
    Kill,
    /// Policy wants a gate and none is recorded — raise one.
    RequestNew,
    /// Policy wants a gate but we could not read whether one already exists.
    Wait,
}

/// The joint meaning of a recorded-gate lookup and a policy decision.
///
/// `decision` is `None` when the fleet declares no gate policy at all.
///
/// [`RefState::Found`] outranks EVERY policy outcome — including both that
/// would otherwise pass: no policy at all, and [`Decision::AutoApprove`] from
/// an emptied rules list. Those two are exactly what a mid-flight `config_json`
/// PATCH produces, and honouring the recorded gate ahead of them is what stops
/// such a PATCH from silently withdrawing a question already put to a human.
///
/// Waking a fleet and reconfiguring one are ONE scope today, so that PATCH asks
/// for no approval of its own; splitting the two is separate work, and a gate
/// this daemon already raised does not have to wait for it.
#[must_use]
pub const fn route(state: RefState, decision: Option<Decision>) -> Route {
    // Checked before `decision` is even looked at, which is the ordering the
    // whole module exists to state.
    if matches!(state, RefState::Found) {
        return Route::EvaluateRecorded;
    }
    match decision {
        None | Some(Decision::AutoApprove) => Route::Pass,
        Some(Decision::AutoKill) => Route::Kill,
        Some(Decision::RequiresApproval) => match state {
            // Answered above. Spelled rather than left to a catch-all so that
            // adding a state to `RefState` fails the build here, at the one
            // place that has to decide what it means.
            RefState::Found => Route::EvaluateRecorded,
            RefState::Unreadable => Route::Wait,
            RefState::Absent => Route::RequestNew,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{RefState, Route, route};
    use crate::gate::Decision;

    #[test]
    fn a_recorded_gate_outranks_every_policy_outcome() {
        // The first two rows carry the security property: both are what a
        // mid-flight config PATCH produces, and under the other ordering both
        // released the parked event while its approval card still sat
        // unanswered.
        assert_eq!(route(RefState::Found, None), Route::EvaluateRecorded);
        assert_eq!(
            route(RefState::Found, Some(Decision::AutoApprove)),
            Route::EvaluateRecorded
        );
        // The rest, so "outranks EVERY outcome" is asserted rather than claimed
        // by the two rows above.
        assert_eq!(
            route(RefState::Found, Some(Decision::RequiresApproval)),
            Route::EvaluateRecorded
        );
        assert_eq!(
            route(RefState::Found, Some(Decision::AutoKill)),
            Route::EvaluateRecorded
        );
    }

    #[test]
    fn with_no_recorded_gate_policy_decides() {
        assert_eq!(route(RefState::Absent, None), Route::Pass);
        assert_eq!(
            route(RefState::Absent, Some(Decision::AutoApprove)),
            Route::Pass
        );
        assert_eq!(
            route(RefState::Absent, Some(Decision::AutoKill)),
            Route::Kill
        );
        assert_eq!(
            route(RefState::Absent, Some(Decision::RequiresApproval)),
            Route::RequestNew
        );
    }

    #[test]
    fn an_unreadable_lookup_waits_rather_than_raising_a_second_card() {
        // A Redis blip must not re-notify a human who may already hold a card
        // for this exact event — but it must also not stall a fleet that wants
        // no gate, which is why only the gated row waits.
        assert_eq!(
            route(RefState::Unreadable, Some(Decision::RequiresApproval)),
            Route::Wait
        );
        assert_eq!(route(RefState::Unreadable, None), Route::Pass);
        assert_eq!(
            route(RefState::Unreadable, Some(Decision::AutoApprove)),
            Route::Pass
        );
        assert_eq!(
            route(RefState::Unreadable, Some(Decision::AutoKill)),
            Route::Kill
        );
    }

    #[test]
    fn every_pairing_is_decided() {
        // The table is small enough to walk in full, so "total" is a fact
        // rather than a claim — and a new `RefState` or `Decision` arm shows up
        // here as a missing row rather than as a runtime surprise.
        let states = [RefState::Found, RefState::Absent, RefState::Unreadable];
        let decisions = [
            None,
            Some(Decision::AutoApprove),
            Some(Decision::RequiresApproval),
            Some(Decision::AutoKill),
        ];

        let decided = states
            .iter()
            .flat_map(|state| {
                decisions
                    .iter()
                    .map(move |decision| route(*state, *decision))
            })
            .count();
        assert_eq!(decided, states.len() * decisions.len());
    }
}
