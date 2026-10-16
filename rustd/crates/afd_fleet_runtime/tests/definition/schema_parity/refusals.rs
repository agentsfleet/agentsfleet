//! Documents the parser refuses, each of which the published schema must
//! refuse too.

use afd_fleet_runtime::Definition;
use afd_fleet_runtime::definition::{
    ConfigOp, InstructionEdit, MAX_DESCRIPTION_CHARS, MAX_INSTRUCTION_OFFSET,
    MAX_INSTRUCTIONS_CHARS,
};
use garde::Validate as _;
use serde_json::{Value, json};
use utoipa::ToSchema as _;

use super::{components, definition_component};

/// The runtime block every runtime pointer below starts from.
const RUNTIME: &str = "/config/x-agentsfleet";

/// A two-byte character: a text bound counted in bytes would refuse a cap of
/// these at half its length, which the schema's `maxLength` never would.
const TWO_BYTE: &str = "é";

/// One change to the shared complete fixture.
enum Change {
    /// Replace the value at a pointer the fixture carries.
    Set(String, Value),
    /// Drop the member at a pointer the fixture carries.
    Remove(String),
    /// Add a member to the object at a pointer.
    Insert(String, &'static str, Value),
}

/// A pointer under the runtime block.
fn runtime(rest: &str) -> String {
    format!("{RUNTIME}{rest}")
}

impl Change {
    fn apply(self, document: &mut Value) {
        let (parent, key, value) = match self {
            Self::Set(pointer, value) => {
                super::super::set(document, &pointer, value);
                return;
            }
            Self::Remove(pointer) => {
                let (parent, key) = pointer.rsplit_once('/').expect("a member pointer");
                (parent.to_owned(), key.to_owned(), None)
            }
            Self::Insert(parent, key, value) => (parent, key.to_owned(), Some(value)),
        };
        let object = document
            .pointer_mut(&parent)
            .and_then(Value::as_object_mut)
            .expect("the change names an object the fixture carries");
        match value {
            Some(value) => object.insert(key, value),
            None => object.remove(&key),
        };
    }
}

/// Refusals in the Definition's own and config keys, each named by the
/// mistake an author would make.
fn shape_refusals() -> Vec<(&'static str, Change)> {
    use Change::{Insert, Remove, Set};
    vec![
        (
            "an unknown top-level key",
            Insert(String::new(), "notes", json!("x")),
        ),
        (
            "a schema version the parser does not read",
            Set("/schema_version".into(), json!(2)),
        ),
        (
            "a retired skill member",
            Insert(
                String::new(),
                "skill",
                json!({"name": "checkout-watch", "description": "x", "version": "1.0.0"}),
            ),
        ),
        (
            "runner tags at the top level",
            Insert(String::new(), "tags", json!(["gpu"])),
        ),
        (
            "runner tags in the runtime block",
            Insert(RUNTIME.into(), "tags", json!(["gpu"])),
        ),
        (
            "a description of null",
            Set("/description".into(), Value::Null),
        ),
        ("no description", Remove("/description".into())),
        (
            "an empty description",
            Set("/description".into(), json!("")),
        ),
        (
            "a description one character over its cap",
            Set(
                "/description".into(),
                json!(TWO_BYTE.repeat(MAX_DESCRIPTION_CHARS + 1)),
            ),
        ),
        (
            "instructions one character over their cap",
            Set(
                "/instructions_md".into(),
                json!(TWO_BYTE.repeat(MAX_INSTRUCTIONS_CHARS + 1)),
            ),
        ),
        ("no config name", Remove("/config/name".into())),
        (
            "a config name of null",
            Set("/config/name".into(), Value::Null),
        ),
        ("an empty config", Set("/config".into(), json!({}))),
        (
            "an unknown config key",
            Insert("/config".into(), "owner", json!("x")),
        ),
        (
            "a runtime key one level too high",
            Insert("/config".into(), "tools", json!([])),
        ),
    ]
}

/// Refusals of the runtime block and its triggers.
fn trigger_refusals() -> Vec<(&'static str, Change)> {
    use Change::{Insert, Remove, Set};
    vec![
        ("no runtime block", Remove(RUNTIME.into())),
        ("a runtime block of null", Set(RUNTIME.into(), Value::Null)),
        (
            "an unknown runtime key",
            Insert(RUNTIME.into(), "retries", json!(3)),
        ),
        ("triggers of null", Set(runtime("/triggers"), Value::Null)),
        (
            "an unknown trigger type",
            Set(runtime("/triggers/0/type"), json!("email")),
        ),
        (
            "a signature naming a header",
            Set(
                runtime("/triggers"),
                json!([{"type": "webhook", "source": "github",
                    "signature": {"secret_ref": "github_secret", "header": "x-hub-signature-256"}}]),
            ),
        ),
        (
            "a cron trigger with no schedule",
            Remove(runtime("/triggers/0/schedule")),
        ),
    ]
}

/// Refusals of the runtime block's tools and budget.
fn budget_refusals() -> Vec<(&'static str, Change)> {
    use Change::{Insert, Remove, Set};
    vec![
        ("no tools", Remove(runtime("/tools"))),
        ("tools of null", Set(runtime("/tools"), Value::Null)),
        ("no budget", Remove(runtime("/budget"))),
        ("an empty budget", Set(runtime("/budget"), json!({}))),
        (
            "a fractional daily ceiling",
            Set(runtime("/budget/daily_nanos"), json!("12.5")),
        ),
        (
            "a daily ceiling in words",
            Set(runtime("/budget/daily_nanos"), json!("ten")),
        ),
        (
            "a daily ceiling as a number",
            // pin test: literal is the contract
            Set(runtime("/budget/daily_nanos"), json!(1000)),
        ),
        (
            "a daily ceiling of null",
            Set(runtime("/budget/daily_nanos"), Value::Null),
        ),
        (
            "a monthly ceiling of null",
            Set(runtime("/budget/monthly_nanos"), Value::Null),
        ),
        (
            "a dollar budget key",
            Insert(runtime("/budget"), "daily_usd", json!(1)),
        ),
    ]
}

/// Refusals in the network, context and gate policy.
fn gate_refusals() -> Vec<(&'static str, Change)> {
    use Change::{Insert, Remove, Set};
    vec![
        (
            "an unknown network key",
            Insert(runtime("/network"), "deny", json!([])),
        ),
        (
            "an unknown context key",
            Insert(runtime("/context"), "window", json!(1)),
        ),
        (
            "a chunk threshold above one",
            Set(runtime("/context/stage_chunk_threshold"), json!(1.5)),
        ),
        (
            "an unknown gates key",
            Insert(runtime("/gates"), "mode", json!("strict")),
        ),
        (
            "an unknown gate rule key",
            Insert(runtime("/gates/rules/0"), "note", json!("x")),
        ),
        (
            "a gate rule with no tool",
            Remove(runtime("/gates/rules/0/tool")),
        ),
        (
            "a zero anomaly threshold",
            Set(runtime("/gates/anomaly_rules/0/threshold_count"), json!(0)),
        ),
        (
            "an anomaly threshold above its cap",
            Set(
                runtime("/gates/anomaly_rules/0/threshold_count"),
                json!(u32::MAX),
            ),
        ),
        (
            "an unknown anomaly rule key",
            Insert(runtime("/gates/anomaly_rules/0"), "scope", json!("run")),
        ),
    ]
}

#[test]
fn every_refusal_is_refused_by_the_parser_and_outside_the_schema() {
    let components = components();
    for (mistake, change) in shape_refusals()
        .into_iter()
        .chain(trigger_refusals())
        .chain(budget_refusals())
        .chain(gate_refusals())
    {
        let mut document = super::super::value();
        change.apply(&mut document);
        assert!(
            Definition::parse(&document.to_string()).is_err(),
            "the parser accepts {mistake}"
        );
        assert!(
            !components.admits(&definition_component(), &document),
            "the published schema admits {mistake}, which the parser refuses"
        );
    }
}

/// The parser drops an unknown key inside a trigger instead of refusing it, so
/// the schema leaves trigger variants open. When the parser starts refusing,
/// this fails, and `config::raw::openapi::trigger` should close the variants.
#[test]
fn an_unknown_trigger_key_is_accepted_by_both() {
    let mut document = super::super::value();
    Change::Insert(runtime("/triggers/0"), "every", json!("hour")).apply(&mut document);
    Definition::parse(&document.to_string()).expect("the parser drops the unknown trigger key");
    assert!(components().admits(&definition_component(), &document));
}

/// Whether the edit parser refuses `operation`, by shape or by bound.
fn refuses_op(operation: &Value) -> bool {
    !serde_json::from_value::<ConfigOp>(operation.clone())
        .is_ok_and(|parsed| parsed.validate().is_ok())
}

#[test]
fn every_refused_config_op_is_outside_the_schema() {
    let components = components();
    let refused = [
        json!({"op": "remove", "path": "/config/name", "reason": "x"}),
        json!({"op": "remove", "path": ""}),
        json!({"op": "remove", "path": null}),
        json!({"op": "copy", "path": "/config/name", "from": "/description"}),
        json!({"op": "add", "path": "/config/name"}),
    ];
    for operation in refused {
        assert!(
            refuses_op(&operation),
            "the edit parser accepts {operation}"
        );
        assert!(
            !components.admits(&ConfigOp::name(), &operation),
            "the published schema admits {operation}, which the edit parser refuses"
        );
    }
    let accepted = json!({"op": "replace", "path": "/config/name", "value": "renamed"});
    assert!(!refuses_op(&accepted));
    assert!(components.admits(&ConfigOp::name(), &accepted));
}

#[test]
fn every_refused_instruction_edit_is_outside_the_schema() {
    let components = components();
    let past_the_end = MAX_INSTRUCTION_OFFSET + 1;
    let refused = [
        json!({"start_byte": 0, "end_byte": 0, "text": "x", "note": "y"}),
        json!({"start_byte": past_the_end, "end_byte": past_the_end, "text": ""}),
        json!({"start_byte": 0, "end_byte": 0, "text": null}),
        json!({"start_byte": 0, "end_byte": 0, "text": TWO_BYTE.repeat(MAX_INSTRUCTIONS_CHARS + 1)}),
    ];
    for edit in refused {
        let parsed = serde_json::from_value::<InstructionEdit>(edit.clone());
        assert!(
            !parsed.is_ok_and(|parsed| parsed.validate().is_ok()),
            "the edit parser accepts {edit}"
        );
        assert!(
            !components.admits(&InstructionEdit::name(), &edit),
            "the published schema admits {edit}, which the edit parser refuses"
        );
    }
}
