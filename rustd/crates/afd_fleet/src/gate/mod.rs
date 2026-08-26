//! Whether a human has to say yes before this event runs.
//!
//! # One traversal, not two
//!
//! `evaluateGate` walks the rules and answers a `GateDecision`; `matchRule`
//! walks them again and answers the rule. Both exist because the decision
//! discards which rule produced it and the approval card needs that rule's
//! workspace-authored copy — and the second carries a comment insisting it uses
//! "the same traversal and same first-match-wins order … so the two can never
//! disagree about which rule applied".
//!
//! An invariant a comment has to insist on is one the code is not holding.
//! Here there is one walk — [`match_rule`] — and the decision is a pure
//! function OF its result, [`Decision::of`]. They cannot disagree about which
//! rule applied because only one of them looks.
//!
//! # What this module does not do
//!
//! It does not park an event, raise a card, count an anomaly, or read a
//! recorded gate. Every one of those is Redis or Postgres, and every one of
//! them is downstream of a decision made here — [`park`](Gates::park),
//! [`pause`](Gates::pause) and [`anomaly`](Gates::anomaly) are the sibling
//! modules that do them. [`route`] is the ordering those I/O outcomes compose
//! through, and it is pure for the same reason: the ORDER in which a recorded
//! gate and the current policy bind is a security property, and a security
//! property should be pinned by a unit test rather than by a live datastore.
//!
//! # The card's two halves are two types
//!
//! [`Stated`] is what the daemon and the workspace assert, and a human may read
//! it as fact. [`Claim`] is what a language model wrote, and they may not.
//! `approval_gate_detail.zig` keeps that boundary with a comment; here it is
//! the type signature of everything downstream, including the renderer that
//! lands in a later milestone.

mod anomaly;
mod claim;
mod decision;
mod detail;
mod park;
mod pause;
mod pending;
mod route;
mod store;

use afd_fleet_runtime::config::{Behavior, Condition, GatePolicy, GateRule};
use serde_json::Value;

pub use self::anomaly::Anomaly;
pub use self::claim::{Claim, MAX_EVIDENCE_BYTES, MAX_PROPOSED_ACTION_BYTES, NO_EVIDENCE};
pub use self::decision::{Answer, DECISION_APPROVE, DECISION_DENY, Status};
pub use self::detail::{
    KIND_REPOSITORY_WRITE, RADIUS_REPOSITORY_WRITE, REPOSITORY_WRITE_SPEND_CEILING, Stated,
};
pub use self::park::{Park, Parked};
pub use self::pause::Trigger;
pub use self::pending::{Evaluation, GateRef, evaluate};
pub use self::route::{RefState, Route, route};
pub use self::store::{Gates, key};

/// The rule field that matches any tool, or any action.
const WILDCARD: &str = "*";

/// What the fleet's policy says about one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing matched, or nothing asked for a human. The event runs.
    AutoApprove,
    /// A human must say yes first.
    RequiresApproval,
    /// The fleet is paused and the event ends.
    AutoKill,
}

impl Decision {
    /// What a matched rule — or the absence of one — decides.
    ///
    /// `None` is the auto-approve fallthrough, and it is the reason the rules
    /// walk cannot be the only gate for a fleet with write access to a
    /// repository: an emptied `rules` list makes every action reach this arm.
    #[must_use]
    pub const fn of(matched: Option<&GateRule>) -> Self {
        match matched {
            None => Self::AutoApprove,
            Some(rule) => match rule.behavior {
                Behavior::Approve => Self::RequiresApproval,
                Behavior::AutoKill => Self::AutoKill,
            },
        }
    }
}

/// The first rule in `policy` that fires for this action.
///
/// First match wins, in authored order — a fleet author reading their own list
/// top to bottom sees the same precedence the daemon applies.
///
/// `context` is the event's request body, when it parsed. Every way it can fail
/// to answer a condition — absent, not an object, missing the field, holding a
/// non-string — resolves the SAME way: the rule fires. That is fail-safe in the
/// direction that costs a human a question rather than the direction that lets
/// an ungated action through, and it is `evaluateCondition`'s posture exactly.
#[must_use]
pub fn match_rule<'a>(
    policy: &'a GatePolicy,
    tool: &str,
    action: &str,
    context: Option<&Value>,
) -> Option<&'a GateRule> {
    policy
        .rules()
        .iter()
        .find(|rule| matches_action(rule, tool, action) && fires(rule, context))
}

/// Whether `rule` names this tool and action, wildcards included.
fn matches_action(rule: &GateRule, tool: &str, action: &str) -> bool {
    (&*rule.tool == WILDCARD || &*rule.tool == tool)
        && (&*rule.action == WILDCARD || &*rule.action == action)
}

/// Whether `rule`'s condition — if it has one — is met by `context`.
///
/// A rule with no condition always fires. A rule WITH one fires unless the
/// context positively contradicts it, which is the fail-safe direction: the
/// chain of `and_then` below answers `None` at every step that cannot decide,
/// and `is_none_or` turns each of those into "fire".
fn fires(rule: &GateRule, context: Option<&Value>) -> bool {
    let Some(condition) = rule.condition.as_deref() else {
        return true;
    };
    // An unparseable condition fires. Write-time validation refuses these for
    // stored policies, so reaching here means a rule built in code — but the
    // posture has to be stated, because the alternative is a malformed
    // condition silently disabling a gate.
    let Some(condition) = Condition::parse(condition) else {
        return true;
    };
    context
        .and_then(Value::as_object)
        .and_then(|object| object.get(condition.field))
        .and_then(Value::as_str)
        .is_none_or(|actual| condition.is_satisfied_by(actual))
}

#[cfg(test)]
mod tests {
    use super::{Decision, match_rule};
    use afd_fleet_runtime::config::{Behavior, GatePolicy, GateRule};
    use serde_json::json;

    fn rule(tool: &str, action: &str, condition: Option<&str>, behavior: Behavior) -> GateRule {
        GateRule {
            tool: tool.into(),
            action: action.into(),
            condition: condition.map(Into::into),
            behavior,
            gate_kind: "deploy".into(),
            blast_radius: "production".into(),
        }
    }

    fn policy(rules: Vec<GateRule>) -> GatePolicy {
        GatePolicy::from_parts(rules, Vec::new(), 900_000)
    }

    #[test]
    fn a_matching_rule_decides_and_an_unmatched_action_approves() {
        let policy = policy(vec![rule("shell", "run", None, Behavior::Approve)]);

        let matched = match_rule(&policy, "shell", "run", None);
        assert_eq!(Decision::of(matched), Decision::RequiresApproval);
        assert_eq!(
            Decision::of(match_rule(&policy, "http", "get", None)),
            Decision::AutoApprove
        );
    }

    #[test]
    fn a_wildcard_matches_either_half() {
        let policy = policy(vec![
            rule("*", "delete", None, Behavior::AutoKill),
            rule("shell", "*", None, Behavior::Approve),
        ]);

        assert_eq!(
            Decision::of(match_rule(&policy, "anything", "delete", None)),
            Decision::AutoKill
        );
        assert_eq!(
            Decision::of(match_rule(&policy, "shell", "whatever", None)),
            Decision::RequiresApproval
        );
        assert_eq!(
            Decision::of(match_rule(&policy, "http", "get", None)),
            Decision::AutoApprove
        );
    }

    #[test]
    fn the_first_authored_rule_wins() {
        // Authored order is what a fleet author reads, so it is the precedence
        // they get — not "most specific", which would need them to hold a
        // ranking in their head.
        let policy = policy(vec![
            rule("*", "*", None, Behavior::Approve),
            rule("shell", "run", None, Behavior::AutoKill),
        ]);

        assert_eq!(
            Decision::of(match_rule(&policy, "shell", "run", None)),
            Decision::RequiresApproval
        );
    }

    #[test]
    fn a_condition_narrows_which_actions_fire() {
        let policy = policy(vec![rule(
            "deploy",
            "push",
            Some("branch == 'main'"),
            Behavior::Approve,
        )]);
        let fires = json!({"branch": "main"});
        let does_not = json!({"branch": "topic"});

        assert!(match_rule(&policy, "deploy", "push", Some(&fires)).is_some());
        assert!(match_rule(&policy, "deploy", "push", Some(&does_not)).is_none());
    }

    #[test]
    fn a_negated_condition_fires_on_anything_but_its_value() {
        let policy = policy(vec![rule(
            "deploy",
            "push",
            Some("env != 'staging'"),
            Behavior::Approve,
        )]);

        assert!(match_rule(&policy, "deploy", "push", Some(&json!({"env": "prod"}))).is_some());
        assert!(match_rule(&policy, "deploy", "push", Some(&json!({"env": "staging"}))).is_none());
    }

    #[test]
    fn every_undecidable_context_fires_the_gate() {
        // Five distinct ways a condition cannot be answered, and the property
        // is that they all answer the SAME way. A fail-safe that is safe for
        // four of five inputs is not a fail-safe — and the unsafe direction
        // here lets an ungated action through.
        let policy = policy(vec![rule(
            "deploy",
            "push",
            Some("branch == 'main'"),
            Behavior::Approve,
        )]);

        for undecidable in [
            None,
            Some(json!(null)),
            Some(json!(["not", "an", "object"])),
            // The object is fine; the FIELD is missing.
            Some(json!({"other": "main"})),
            // The field is there; it is not a string.
            Some(json!({"branch": 7})),
        ] {
            assert!(
                match_rule(&policy, "deploy", "push", undecidable.as_ref()).is_some(),
                "{undecidable:?} must fire the gate rather than skip it"
            );
        }
    }

    #[test]
    fn an_unparseable_condition_fires_rather_than_disabling_the_gate() {
        let policy = policy(vec![rule(
            "deploy",
            "push",
            Some("garbage"),
            Behavior::Approve,
        )]);

        assert!(match_rule(&policy, "deploy", "push", Some(&json!({"branch": "main"}))).is_some());
    }

    #[test]
    fn a_rule_with_no_condition_always_fires() {
        let policy = policy(vec![rule("deploy", "push", None, Behavior::Approve)]);

        assert!(match_rule(&policy, "deploy", "push", None).is_some());
        assert!(match_rule(&policy, "deploy", "push", Some(&json!({}))).is_some());
    }
}
