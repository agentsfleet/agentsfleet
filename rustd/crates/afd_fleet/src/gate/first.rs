//! The pass for an event no gate has been raised for yet.
//!
//! Split from [`pass`](super::pass) because the two halves of the gate answer
//! different questions — "what did a human already say" and "does anyone need
//! to be asked" — and only this one reads policy at all. The ordering that
//! keeps them apart, and why it is a security property, is documented there.
//!
//! # The write-kind park runs before the rules, and before the no-gates return
//!
//! A fleet whose repository binding declares WRITE access parks EVERY
//! first-encounter event. Gate rules cannot hold that boundary: auto-approve is
//! their no-match fallthrough, and they ride `config_json`, which a PATCH
//! reaches under the same `fleet:write` scope that wakes the fleet. So an
//! emptied `rules` list would release every action of a fleet that can push to
//! a repository — which is exactly the fleet that must not be released.
//!
//! Anomaly counters are skipped on that path. Each event there costs one card
//! and executes nothing until a human answers, so the human is the runaway
//! brake and a counter would only stop the fleet for being asked patiently.

use afd_core::clock::UnixMillis;
use afd_fleet_runtime::FleetConfig;
use afd_fleet_runtime::config::{Access, DEFAULT_TIMEOUT_MS};
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
        now: UnixMillis,
    ) -> Verdict {
        // KIND-PARK, and it runs BEFORE the rules walk and before the no-gates
        // return below. Gate rules cannot hold this boundary: auto-approve is
        // their no-match fallthrough, and they ride `config_json`, which a PATCH
        // reaches under the same `fleet:write` scope that wakes the fleet.
        if writes_to_a_repository(request.config) {
            return self.park_write_kind(request, state, now).await;
        }

        let Some(policy) = request.config.gates() else {
            return Verdict::Pass;
        };

        // Reached only on a first encounter — see the module note on why that
        // matters for an increment. Anomaly counters are also skipped entirely
        // on the write-kind path above: each event there costs one card and
        // executes nothing until a human answers, so the human IS the brake.
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
            return self.stop(request, Trigger::Anomaly, now).await;
        }

        let context = parse_context(request.request_json);
        let matched = match_rule(policy, request.event_type, request.actor, context.as_ref());

        match route(state, Some(Decision::of(matched))) {
            Route::Pass => Verdict::Pass,
            Route::Kill => self.stop(request, Trigger::Policy, now).await,
            // An unreadable lookup must not become a SECOND card for this
            // event: wait a poll rather than re-notify a human who may already
            // hold one.
            Route::Wait => Verdict::Await(Waiting::Unreadable),
            Route::RequestNew => {
                // The matched rule carries the workspace copy the decision
                // discards, and it is the SAME match — so the card cannot
                // describe a different rule from the one that fired.
                let timeout = timeout_of(policy.timeout_ms());
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

    /// Park every first-encounter event of a fleet that can WRITE to a
    /// repository.
    ///
    /// An unreadable lookup waits a poll rather than raising a possible second
    /// card — the same discipline the rules path's [`Route::Wait`] holds.
    async fn park_write_kind(
        &self,
        request: &Check<'_>,
        state: RefState,
        now: UnixMillis,
    ) -> Verdict {
        if state == RefState::Unreadable {
            return Verdict::Await(Waiting::Unreadable);
        }
        // No rule carries workspace copy here, so the kind, the radius and the
        // ceiling are the daemon's own, and the timeout is the default rather
        // than a policy value a PATCH could stretch.
        let stated = Stated::of(
            request.event_type,
            request.actor,
            request.event_id,
            request.config.repository_binding(),
            timeout_of(DEFAULT_TIMEOUT_MS),
        )
        .write_kind();
        let context = parse_context(request.request_json);
        self.raise(request, stated, context.as_ref(), now).await
    }

    /// Raise the card and say what the caller does next.
    async fn raise(
        &self,
        request: &Check<'_>,
        stated: Stated<'_>,
        context: Option<&Value>,
        now: UnixMillis,
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
    async fn stop(&self, request: &Check<'_>, trigger: Trigger, now: UnixMillis) -> Verdict {
        if let Err(fault) = self.pause(request.fleet_id, trigger, now).await {
            let fleet = request.fleet_id.as_str();
            let reason = fault.to_string();
            tracing::error!(
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

/// Whether the fleet's repository binding declares WRITE access.
///
/// The one kind that parks unconditionally. A fleet with no binding, or a
/// read-only one, is judged by its rules like any other.
fn writes_to_a_repository(config: &FleetConfig) -> bool {
    config
        .repository_binding()
        .is_some_and(|binding| binding.access() == Access::Write)
}

/// The event body as a condition context, when it is one.
///
/// `None` for a body that is absent, empty, or will not parse — every one of
/// which resolves the same way downstream, because a rule that cannot be
/// answered FIRES. See [`match_rule`].
fn parse_context(request_json: &str) -> Option<Value> {
    serde_json::from_str(request_json).ok()
}

/// A policy timeout as the signed milliseconds a deadline is computed in.
///
/// Saturating rather than `as`: the config clamps to a day, so the cast is
/// provably safe today — and a silent wrap if that clamp ever moves would
/// produce a deadline in the past, which reads as an instantly-expired gate
/// rather than as the bug it is.
fn timeout_of(millis: u64) -> i64 {
    i64::try_from(millis).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{parse_context, timeout_of, writes_to_a_repository};
    use afd_fleet_runtime::FleetConfig;
    use afd_fleet_runtime::config::{Access, DEFAULT_TIMEOUT_MS, Mode};
    use afd_fleet_runtime::provider::StaticRegistry;
    use serde_json::json;

    /// A stored document declaring a repository binding at `access`, or none.
    ///
    /// Driven through the real `parse` rather than a hand-built struct: the
    /// question here is what a STORED config resolves to, and a constructor
    /// that skipped validation would prove the helper rather than the path.
    fn config(access: Option<&str>) -> FleetConfig {
        let binding = access.map_or_else(String::new, |access| {
            let base = if access == "write" {
                r#","repository_base":"main""#
            } else {
                ""
            };
            format!(r#","repositories":["acme/widgets"],"repository_access":"{access}"{base}"#)
        });
        // `triggers` is required, and `api` is the variant carrying no config
        // of its own — the smallest document that resolves.
        let document = format!(
            r#"{{"name":"probe","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],"budget":{{"daily_dollars":1.0}}{binding}}}}}"#
        );
        FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
            .expect("a stored document resolves")
    }

    #[test]
    fn only_a_write_binding_parks_unconditionally() {
        // The boundary the whole KIND-PARK path exists for. A fleet with no
        // binding and one that may only READ are judged by their rules like
        // any other; only the one that can push is parked on sight.
        assert!(writes_to_a_repository(&config(Some("write"))));
        assert!(!writes_to_a_repository(&config(Some("read"))));
        assert!(!writes_to_a_repository(&config(None)));
    }

    #[test]
    fn the_write_binding_this_reads_is_the_one_the_mint_scopes_by() {
        // Not a second notion of "can write" — the same `Access` the credential
        // mint enforces, so the card cannot promise a reach the token does not
        // have.
        let writing = config(Some("write"));
        let binding = writing
            .repository_binding()
            .expect("a declared binding resolves");

        assert_eq!(binding.access(), Access::Write);
    }

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

    #[test]
    fn a_timeout_never_becomes_a_deadline_in_the_past() {
        // The cast is provably safe under today's clamp, so what this pins is
        // the direction it fails if that clamp ever moves: saturating, never
        // wrapping. A wrapped timeout is a gate that expires the instant it is
        // raised, which reads as a lapsed approval rather than as the bug it is.
        assert_eq!(timeout_of(DEFAULT_TIMEOUT_MS), 3_600_000);
        assert_eq!(timeout_of(0), 0);
        assert_eq!(timeout_of(u64::MAX), i64::MAX);
        assert!(timeout_of(u64::MAX) > 0);
    }
}
