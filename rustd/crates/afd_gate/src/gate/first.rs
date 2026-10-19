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
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let fleet = request.fleet_id.as_str();
            let trigger_word = trigger.as_str();
            let Logged { error_code, reason } = fault.logged();
            tracing::error!(
                error_code,
                event = EVENT_PAUSE_FAILED,
                fleet_id = fleet,
                trigger = trigger_word,
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

    #[cfg(feature = "test-util")]
    #[expect(
        clippy::expect_used,
        reason = "test target: an unmet precondition should fail the test loudly"
    )]
    mod ordering {
        //! The arm the first-encounter pass reaches only if its caller's ordering
        //! broke: a recorded gate handed to the pass that asks policy.
        //!
        //! `check` answers a recorded gate before this pass runs, so no production
        //! path arrives here; what is pinned is the direction it fails in when one
        //! does — the event waits, and the line says the ordering was violated —
        //! because a pass that let policy speak over a recorded question would
        //! withdraw it from the human holding the card.

        use afd_core::id::Uuid7;
        use afd_core::test_util::trace::Capture;
        use afd_crypto::entropy::Entropy;
        use afd_dragonfly::Dragonfly;
        use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
        use afd_fleet_runtime::FleetConfig;

        use crate::gate::pass::{Check, Verdict, Waiting};
        use crate::gate::route::RefState;
        use crate::gate::store::Gates;

        /// A queue address nothing listens on.
        const NOWHERE: &str = "redis://127.0.0.1:1";

        /// A fleet whose only rule names another tool, so policy passes a chat outright.
        const APPROVE_EVERY_CHAT: &str = r#"{"name":"gate-unit","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"},"gates":{"rules":[{"tool":"shell","action":"user:other","behavior":"approve"}]}}}"#;

        const FLEET: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeee2";

        #[tokio::test]
        async fn a_recorded_gate_reaching_the_policy_pass_waits_and_says_so() {
            let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
                DragonflyRole::Default,
                NOWHERE.to_owned(),
            ))
            .expect("a lazy handle opens no socket and cannot fail");
            let gates = Gates::new(afd_db::test_util::unreachable_db(), queue, Entropy::new());
            let fleet = Uuid7::parse(FLEET).expect("a canonical identifier");
            let config =
                FleetConfig::stored(APPROVE_EVERY_CHAT).expect("the fixture policy resolves");
            let capture = Capture::install();

            let verdict = gates
                .judge_first_encounter(
                    &Check {
                        fleet_id: &fleet,
                        workspace_id: &fleet,
                        event_id: "event-ordering-broke",
                        event_type: "chat",
                        actor: "user:fixture",
                        request_json: "{}",
                        config: &config,
                    },
                    RefState::Found,
                    jiff::Timestamp::UNIX_EPOCH,
                )
                .await;

            assert_eq!(
                verdict,
                Verdict::Await(Waiting::Unreadable),
                "policy says pass, and still the recorded question is not withdrawn"
            );
            let logged = capture.only("gate_route_unreachable");
            assert_eq!(logged.field("fleet_id"), Some(FLEET));
            assert_eq!(
                logged.field("agentsfleet_event_id"),
                Some("event-ordering-broke")
            );
        }
    }
}
