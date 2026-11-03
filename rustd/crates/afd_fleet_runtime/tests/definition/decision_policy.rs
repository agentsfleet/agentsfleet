//! Owner criteria survive review edits without mutating accepted definitions.

use afd_core::error_code;
use afd_fleet_runtime::{
    Definition,
    definition::{ConfigOp, EditBatch, canonical_digest},
};
use serde_json::{Value, json};

use super::{parse, value};

const POLICY_PATH: &str = "/config/x-agentsfleet/decision_policy";

pub(super) fn document() -> Value {
    let mut document = value();
    let policy = serde_json::from_str(&crate::support::fixture("goal-repair/decision-policy.json"))
        .expect("shared owner-reviewed question fixture");
    document
        .pointer_mut("/config/x-agentsfleet")
        .and_then(Value::as_object_mut)
        .expect("runtime policy object")
        .insert("decision_policy".into(), policy);
    document
}

#[test]
fn owner_questions_roundtrip_with_every_role_criteria_and_stable_statement_order() {
    let definition = parse(&document());
    let policy = definition
        .runtime()
        .decision_policy()
        .expect("reviewed policy");
    let routing = policy.routing();
    assert_eq!(routing.intent().criteria().len(), 6);
    assert_eq!(routing.association().criteria().len(), 3);
    assert_eq!(routing.multiple_requests().criteria().len(), 2);
    assert_eq!(
        routing.intent().criterion("steer"),
        Some("Change the requested approach for an existing repair.")
    );
    assert_eq!(policy.completion().next_action().criteria().len(), 4);
    assert_eq!(
        policy
            .completion()
            .statements()
            .iter()
            .map(afd_fleet_runtime::config::decision::CompletionStatement::id)
            .collect::<Vec<_>>(),
        ["defect-fixed", "verification-current"]
    );
    let statement = policy
        .completion()
        .statements()
        .first()
        .expect("first completion statement");
    assert_eq!(
        statement.question().text(),
        "Does the patch fix the task's accepted Linkwarden defect?"
    );
    assert_eq!(
        statement.question().criterion("true"),
        Some("Current code and regression evidence demonstrate the accepted behavior.")
    );
    assert_eq!(statement.question().criterion("DONE"), None);
    let restored = Definition::parse(
        &serde_json::to_string(&definition).expect("serialize accepted definition"),
    )
    .expect("stored policy uses the same parser");
    assert_eq!(restored, definition);
    assert_eq!(
        canonical_digest(&restored).expect("restored digest"),
        canonical_digest(&definition).expect("original digest")
    );
    assert!(
        parse(&value()).runtime().decision_policy().is_none(),
        "ordinary fleets acquire no hidden questions"
    );
}

#[test]
fn question_edits_change_the_next_content_identity_and_leave_the_accepted_setup_untouched() {
    let original = parse(&document());
    let changed = original.edit(EditBatch {
        config_ops: vec![ConfigOp::Replace {
            path: format!("{POLICY_PATH}/completion/statements/0/criteria/true"),
            value: json!("The regression test reproduces the original defect and passes for the proposed patch."),
        }],
        ..EditBatch::default()
    }).expect("owner can revise criteria through normal setup editing");
    assert_ne!(
        canonical_digest(&changed).expect("new digest"),
        canonical_digest(&original).expect("accepted digest")
    );
    assert_eq!(
        original,
        parse(&document()),
        "no older accepted setup is edited"
    );
    assert_eq!(changed.instructions_md(), original.instructions_md());
    assert_eq!(
        changed
            .runtime()
            .decision_policy()
            .expect("policy")
            .completion()
            .statements()
            .first()
            .expect("completion statement")
            .id(),
        "defect-fixed"
    );
    let invalid = changed.edit(EditBatch {
        config_ops: vec![ConfigOp::Replace {
            path: format!("{POLICY_PATH}/completion/next_action/criteria"),
            value: json!({"CONTINUE":"Go", "WAIT":"Later", "DONE":"Done", "PUBLISH":"Write anywhere"}),
        }],
        ..EditBatch::default()
    }).expect_err("question edits cannot introduce native actions");
    assert_eq!(invalid.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(original, parse(&document()));
}

#[test]
fn incomplete_wrong_shaped_and_unknown_policies_never_become_runtime_defaults() {
    for replacement in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"routing":{}}),
        json!({"completion":{}}),
    ] {
        let mut candidate = document();
        *candidate.pointer_mut(POLICY_PATH).expect("decision policy") = replacement;
        refused(&candidate);
    }
    for (pointer, replacement) in [
        (
            "/routing/intent",
            json!(["Question", {"conversation":"Talk"}]),
        ),
        ("/routing/intent/question", Value::Null),
        (
            "/routing/association/criteria",
            json!({"matching_task":"Yes", "new_work":"New", "publish":"Unbounded"}),
        ),
        ("/routing/multiple_requests/criteria", json!({"true":"Yes"})),
        (
            "/completion/statements",
            json!([["id", "Question", {"true":"Yes", "false":"No"}]]),
        ),
        (
            "/completion/next_action/criteria",
            json!({"CONTINUE":"Go", "WAIT":"Wait", "DONE":"Done", "publish":"Wrong"}),
        ),
    ] {
        let mut candidate = document();
        *candidate
            .pointer_mut(&format!("{POLICY_PATH}{pointer}"))
            .expect("fixture field") = replacement;
        refused(&candidate);
    }
    let mut unknown = document();
    unknown
        .pointer_mut(&format!("{POLICY_PATH}/completion"))
        .and_then(Value::as_object_mut)
        .expect("completion object")
        .insert("platform_override".into(), json!(true));
    refused(&unknown);
}

#[test]
fn full_question_and_statement_limits_reject_ambiguous_or_unusable_policy_without_truncation() {
    for (pointer, replacement) in [
        ("/routing/intent/question", json!(" \n\t")),
        ("/routing/intent/question", json!("private\0question")),
        ("/routing/intent/question", json!("界".repeat(200))),
        ("/routing/intent/question", json!("界".repeat(512))),
        ("/completion/statements/0/criteria/true", json!("   ")),
        (
            "/completion/statements/0/criteria/false",
            json!("No\0evidence"),
        ),
        ("/completion/statements/0/id", json!("wrong id")),
        ("/completion/statements/1/id", json!("defect-fixed")),
        ("/completion/statements", json!([])),
    ] {
        let mut candidate = document();
        *candidate
            .pointer_mut(&format!("{POLICY_PATH}{pointer}"))
            .expect("fixture field") = replacement;
        refused(&candidate);
    }
    let mut full = document();
    let first = full
        .pointer(&format!("{POLICY_PATH}/completion/statements/0"))
        .expect("first statement")
        .clone();
    let statements = full
        .pointer_mut(&format!("{POLICY_PATH}/completion/statements"))
        .and_then(Value::as_array_mut)
        .expect("statement array");
    statements.clear();
    for index in 0..6 {
        let mut statement = first.clone();
        statement
            .as_object_mut()
            .expect("statement object")
            .insert("id".into(), json!(format!("statement-{index}")));
        statements.push(statement);
    }
    parse(&full)
        .validate_for_save()
        .expect("six complete statements are usable");
    full.pointer_mut(&format!("{POLICY_PATH}/completion/statements"))
        .and_then(Value::as_array_mut)
        .expect("array")
        .push(first);
    refused(&full);
}

#[test]
fn unicode_question_text_is_preserved_at_the_wire_byte_limit() {
    for text in ["界".repeat(160), "🙂".repeat(120), "A".repeat(512)] {
        let mut candidate = document();
        *candidate
            .pointer_mut(&format!("{POLICY_PATH}/completion/statements/0/question"))
            .expect("statement question") = json!(text);
        let definition = parse(&candidate);
        assert_eq!(
            definition
                .runtime()
                .decision_policy()
                .expect("policy")
                .completion()
                .statements()
                .first()
                .expect("completion statement")
                .question()
                .text(),
            text
        );
    }
}

fn refused(candidate: &Value) {
    let failure = Definition::parse(&candidate.to_string())
        .expect_err("invalid policy is refused atomically");
    assert_eq!(failure.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert!(
        !failure.to_string().contains("private\0question"),
        "refusal never echoes private question values"
    );
}
