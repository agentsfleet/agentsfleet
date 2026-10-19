//! The pass for an event no gate has been raised for yet.
//!
//! Split from [`pass`](super::pass) because the two halves of the gate answer
//! different questions — "what did a human already say" and "does anyone need
//! to be asked" — and only this one reads policy at all. The ordering that
//! keeps them apart, and why it is a security property, is documented there.
//!
//! # The rules walk is the only first-encounter path
//!
//! A fleet whose repository binding declared WRITE access used to park EVERY
//! first-encounter event here, ahead of the rules. It asked a person again on
//! every model turn — a continuation gets a fresh event identifier, so each
//! turn re-parked — and it bought nothing the standing integration grant did
//! not already carry. The grant names the fleet, the fleet's binding names the
//! repositories and the access level, and the mint narrows the token to exactly
//! those. What bounds a write fleet now is that grant, the fleet's
//! `budget.daily_nanos`, and `agentsfleet grant delete`.

use afd_core::error_code::{Coded as _, Logged};
use jiff::Timestamp;
use serde_json::Value;

use crate::gate::claim::Claim;
use crate::gate::detail::Stated;
use crate::gate::park::{Park, Parked};
use crate::gate::pass::{Check, Verdict, Waiting};
use crate::gate::pause::Trigger;
use crate::gate::route::{RefState, Route, route};
use crate::gate::store::Gates;
use crate::gate::{Anomaly, Decision, match_rule};

/// A fleet could not be stopped after a gate decided it should be.
const EVENT_PAUSE_FAILED: &str = "gate_pause_failed";

/// The pass reached an arm its own ordering has already answered.
const EVENT_ROUTE_UNREACHABLE: &str = "gate_route_unreachable";

impl Gates {
    /// The pass for an event no gate has been raised for yet.
    ///
    /// Split from [`Gates::check`] because the two halves answer different
    /// questions — "what did a human already say" and "does anyone need to be
    /// asked" — and only the second reads policy at all.
    pub(super) async fn judge_first_encounter(
        &self,
        request: &Check<'_>,
        state: RefState,
        now: Timestamp,
    ) -> Verdict {
        let Some(policy) = request.config.gates() else {
            return Verdict::Pass;
        };

        // Reached only on a first encounter — see the module note on why that
        // matters for an increment.
        if self
            .anomaly(
                request.fleet_id,
                request.event_type,
                request.actor,
                policy.anomaly_rules(),
            )
            .await
            == Anomaly::AutoKill
        {
            return self.stop(request, Trigger::Anomaly).await;
        }

        let context = parse_context(request.request_json);
        let matched = match_rule(policy, request.event_type, request.actor, context.as_ref());

        match route(state, Some(Decision::of(matched))) {
            Route::Pass => Verdict::Pass,
            Route::Kill => self.stop(request, Trigger::Policy).await,
            // An unreadable lookup must not become a SECOND card for this
            // event: wait a poll rather than re-notify a human who may already
            // hold one.
            Route::Wait => Verdict::Await(Waiting::Unreadable),
            Route::RequestNew => {
                // The matched rule carries the workspace copy the decision
                // discards, and it is the SAME match — so the card cannot
                // describe a different rule from the one that fired.
                let timeout = policy.timeout();
                let stated = Stated::of(
                    request.event_type,
                    request.actor,
                    request.event_id,
                    request.config.repository_binding(),
                    timeout,
                );
                let stated = matched.map_or(stated, |rule| stated.under(rule));
                self.raise(request, stated, context.as_ref(), now).await
            }
            // Answered by `check` before any policy was read, so reaching here
            // means the two disagree about the ordering this module exists to
            // hold. Waiting is the fail-safe direction and a panic in a daemon
            // is not, so it is logged loudly and waits.
            Route::EvaluateRecorded => {
                let fleet = request.fleet_id.as_str();
                tracing::error!(
                    event = EVENT_ROUTE_UNREACHABLE,
                    fleet_id = fleet,
                    agentsfleet_event_id = request.event_id,
                    "a recorded gate reached the first-encounter pass; the event waits"
                );
                Verdict::Await(Waiting::Unreadable)
            }
        }
    }

    /// Raise the card and say what the caller does next.
    async fn raise(
        &self,
        request: &Check<'_>,
        stated: Stated<'_>,
        context: Option<&Value>,
        now: Timestamp,
    ) -> Verdict {
        let claim = Claim::of(context);
        let parked = self
            .park(
                Park {
                    fleet_id: request.fleet_id,
                    workspace_id: request.workspace_id,
                    event_id: request.event_id,
                    stated,
                    claim: &claim,
                },
                now,
            )
            .await;

        match parked {
            Parked::Awaiting(_) => Verdict::Await(Waiting::Parked),
            Parked::Unavailable => Verdict::Unavailable,
        }
    }

    /// Stop the fleet, and say which trigger did it.
    ///
    /// A failed pause does NOT change the verdict: the gate decided this fleet
    /// should stop, and reporting that it did not while also admitting the
    /// event would be the worst of both. The event stays leasable either way,
    /// so a fleet that could not be paused re-decides on the next poll.
    async fn stop(&self, request: &Check<'_>, trigger: Trigger) -> Verdict {
        if let Err(fault) = self.pause(request.fleet_id, trigger).await {
            let fleet = request.fleet_id.as_str();
            let Logged { error_code, reason } = fault.logged();
            tracing::error!(
                error_code,
                event = EVENT_PAUSE_FAILED,
                fleet_id = fleet,
                trigger = trigger.as_str(),
                reason,
                "the gate stopped this fleet and the row would not flip; it stays active"
            );
        }
        Verdict::Killed(trigger)
    }
}

/// The event body as a condition context, when it is one.
///
/// `None` for a body that is absent, empty, or will not parse — every one of
/// which resolves the same way downstream, because a rule that cannot be
/// answered FIRES. See [`match_rule`].
fn parse_context(request_json: &str) -> Option<Value> {
    serde_json::from_str(request_json).ok()
}

#[cfg(test)]
mod tests {
    use super::parse_context;
    use serde_json::json;

    #[test]
    fn an_unusable_body_is_no_context_at_all() {
        // All of these resolve the same way downstream, because a rule whose
        // condition cannot be answered FIRES — the fail-safe direction. What
        // this pins is that none of them is an ERROR that could strand a poll.
        for unusable in ["", "{", "not json", "\u{0}"] {
            assert!(parse_context(unusable).is_none(), "{unusable:?}");
        }
        // And a body that does parse arrives whole, including the non-object
        // shapes `match_rule` treats as undecidable rather than refusing.
        assert_eq!(
            parse_context(r#"{"branch":"main"}"#),
            Some(json!({"branch": "main"}))
        );
        assert_eq!(parse_context("[]"), Some(json!([])));
        assert_eq!(parse_context("null"), Some(json!(null)));
    }
}
