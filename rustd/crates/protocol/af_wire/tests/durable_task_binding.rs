//! A task tool's binding: one item, a new task with its statements or a steer
//! of an unfinished one, on a span of a person's message.

#![expect(
    clippy::expect_used,
    reason = "wire fixtures must parse or fail the test"
)]

use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskInputAction,
    STATEMENT_ID_MAX_BYTES,
};
use garde::Validate as _;
use serde_json::{Value, json};

const TURN: &str = "019a0000-0000-7000-8000-000000000001";
const TASK: &str = "019a0000-0000-7000-8000-000000000002";
const DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One statement, with its id and the given question and criteria.
fn statement_with(id: &str, question: &str, yes: &str, no: &str) -> Value {
    json!({"id": id, "question": question, "criteria": {"true": yes, "false": no}})
}

fn statement(id: &str) -> Value {
    statement_with(
        id,
        "Are PR 88's checks green?",
        "Every required check passed.",
        "A check failed.",
    )
}

fn new_work(statements: &Value) -> Value {
    json!({"action": "new_work", "source_start": 0, "source_end": 12, "title": "Fix the cart",
        "target": {"kind": "api_only"}, "statements": statements})
}

fn input(items: &Value) -> Value {
    json!({"fencing_token": 9, "source_turn_id": TURN, "plan_digest": DIGEST, "items": items})
}

fn decoded(body: Value) -> DurableTaskBindingsInput {
    serde_json::from_value(body).expect("a binding decodes")
}

/// Whether `body` decodes and passes its bounds.
fn admitted(body: Value) -> bool {
    decoded(body).validate().is_ok()
}

#[test]
fn a_new_work_item_carries_its_statements() {
    let binding = decoded(input(&json!([new_work(&json!([statement("pr_green")]))])));

    binding.validate().expect("a complete new task");
    let [item] = binding.items.as_slice() else {
        unreachable!("one item was sent");
    };
    assert_eq!(item.action(), DurableTaskInputAction::NewWork);
    assert_eq!(item.span(), (0, 12));
    assert!(matches!(
        item,
        DurableTaskBindingItem::NewWork { statements, .. } if statements.len() == 1
    ));
}

#[test]
fn a_steer_item_names_its_task_and_span() {
    let steer =
        json!({"action": "steer", "source_start": 3, "source_end": 9, "durable_task_id": TASK});
    let binding = decoded(input(&json!([steer])));

    binding.validate().expect("a steer of a named task");
    let [item] = binding.items.as_slice() else {
        unreachable!("one item was sent");
    };
    assert_eq!(item.action(), DurableTaskInputAction::Steer);
    assert_eq!(item.span(), (3, 9));
}

#[test]
fn a_binding_takes_exactly_one_item() {
    let one = new_work(&json!([statement("a")]));
    for items in [json!([]), json!([one.clone(), one])] {
        assert!(!admitted(input(&items)), "{items}");
    }
}

#[test]
fn a_new_task_needs_a_statement_with_a_unique_id() {
    for statements in [json!([]), json!([statement("a"), statement("a")])] {
        assert!(
            !admitted(input(&json!([new_work(&statements)]))),
            "{statements}"
        );
    }
}

#[test]
fn a_statement_outside_its_bounds_is_refused() {
    let past = "a".repeat(STATEMENT_ID_MAX_BYTES + 1);
    for refused in [
        statement_with("a", "   ", "yes", "no"),
        statement_with("a", "q", "yes", "no\u{0}"),
        statement_with("a", "q", "", "no"),
        statement("a b"),
        statement("ünï"),
        statement(""),
        statement(&past),
    ] {
        assert!(
            !admitted(input(&json!([new_work(&json!([refused]))]))),
            "{refused}"
        );
    }
    let at_ceiling = statement(&"a".repeat(STATEMENT_ID_MAX_BYTES));
    assert!(admitted(input(&json!([new_work(&json!([at_ceiling]))]))));
}

#[test]
fn a_retired_field_or_action_fails_to_decode() {
    let item = new_work(&json!([statement("a")]));
    let with_revision = json!({"fencing_token": 9, "source_turn_id": TURN, "plan_digest": DIGEST,
        "accepted_revision_id": TURN, "items": [item]});
    let with_decision = json!({"fencing_token": 9, "source_turn_id": TURN, "plan_digest": DIGEST,
        "message_decision_call_id": TURN, "items": [item]});
    let steer_with_title = json!({"action": "steer", "source_start": 0, "source_end": 1,
        "durable_task_id": TASK, "title": "t"});
    let conversation = json!({"action": "conversation", "source_start": 0, "source_end": 1});
    let statement_extra = json!({"id": "a", "question": "q",
        "criteria": {"true": "y", "false": "n"}, "weight": 2});
    for body in [
        with_revision,
        with_decision,
        input(&json!([steer_with_title])),
        input(&json!([conversation])),
        input(&json!([new_work(&json!([statement_extra]))])),
    ] {
        let refused = serde_json::from_value::<DurableTaskBindingsInput>(body.clone()).err();
        assert!(refused.is_some(), "{body}");
    }
}

#[test]
fn the_interpretation_excludes_the_run_fence() {
    let first = decoded(input(&json!([new_work(&json!([statement("a")]))])));
    let mut refenced = first.clone();
    refenced.fencing_token = 10;

    let digest = |binding: &DurableTaskBindingsInput| {
        serde_json::to_string(&binding.interpretation()).expect("an interpretation serializes")
    };
    assert_eq!(digest(&first), digest(&refenced));
    assert!(digest(&first).contains(TURN));
}
