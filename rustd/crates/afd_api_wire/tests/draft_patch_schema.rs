//! The draft patch request's published schema against the validator that
//! bounds its edit batch.
//!
//! The wire parser reads any number of edits; the batch validator refuses one
//! over either cap; the published schema must refuse exactly the same batch.
//! The daemon's own validator, `afd_authoring::DraftPatch`, declares the same
//! two garde bounds over the same constants as `EditBatch` read here.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::authoring::DraftPatchRequest;
use afd_fleet_runtime::definition::{
    ConfigOp, EditBatch, InstructionEdit, MAX_CONFIG_OPS, MAX_INSTRUCTION_EDITS,
};
use garde::Validate as _;
use serde_json::{Value, json};
use utoipa::{PartialSchema, ToSchema};

/// The request as the daemon reads it.
type Request = DraftPatchRequest<ConfigOp, InstructionEdit>;

/// The published schema, as a client reads it.
fn published() -> Value {
    serde_json::to_value(<Request as PartialSchema>::schema()).expect("a schema serializes")
}

/// The published `maxItems` on one of the request's lists.
fn published_cap(schema: &Value, key: &str) -> usize {
    schema
        .pointer(&format!("/properties/{key}/maxItems"))
        .and_then(Value::as_u64)
        .and_then(|cap| usize::try_from(cap).ok())
        .unwrap_or_else(|| panic!("`{key}` publishes no maxItems"))
}

/// What the parser, the validator and the schema each say about one body.
struct Verdicts {
    validator: bool,
    schema: bool,
}

/// Reads `count` copies of `entry` under `key` through all three.
fn verdicts(key: &str, entry: &Value, count: usize) -> Verdicts {
    let body = json!({ key: vec![entry.clone(); count] });
    let request: Request = serde_json::from_value(body)
        .unwrap_or_else(|failure| panic!("the parser reads {count} `{key}`: {failure}"));
    let batch = EditBatch {
        config_ops: request.config_ops,
        instruction_edits: request.instruction_edits,
    };
    Verdicts {
        validator: batch.validate().is_ok(),
        schema: count <= published_cap(&published(), key),
    }
}

#[test]
fn one_edit_over_either_cap_is_refused_by_the_validator_and_the_schema() {
    let cases = [
        (
            "config_ops",
            json!({"op": "remove", "path": "/config/x-agentsfleet/model"}),
            MAX_CONFIG_OPS,
        ),
        (
            "instruction_edits",
            json!({"start_byte": 0, "end_byte": 0, "text": ""}),
            MAX_INSTRUCTION_EDITS,
        ),
    ];
    for (key, entry, cap) in cases {
        let at_cap = verdicts(key, &entry, cap);
        assert!(at_cap.validator, "the validator refuses {cap} `{key}`");
        assert!(at_cap.schema, "the schema refuses {cap} `{key}`");

        let over = verdicts(key, &entry, cap + 1);
        assert!(!over.validator, "the validator accepts {} `{key}`", cap + 1);
        assert!(!over.schema, "the schema admits {} `{key}`", cap + 1);
    }
}

#[test]
fn the_schema_lists_exactly_the_requests_fields() {
    // No `..`: a field added to the request fails to compile here until it is
    // named, and naming it fails the assertion until the twin carries it.
    let DraftPatchRequest::<Value, Value> {
        instance_name,
        title,
        authoring_model_ref,
        runtime_model_ref,
        config_ops,
        instruction_edits,
    } = serde_json::from_value(json!({})).expect("an empty patch parses");
    assert!(
        [instance_name, title, authoring_model_ref, runtime_model_ref]
            .iter()
            .all(Option::is_none)
            && config_ops.is_empty()
            && instruction_edits.is_empty(),
        "an empty patch changes nothing"
    );
    let fields = [
        "instance_name",
        "title",
        "authoring_model_ref",
        "runtime_model_ref",
        "config_ops",
        "instruction_edits",
    ];
    let schema = published();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("the request publishes its properties");
    assert_eq!(properties.len(), fields.len());
    for field in fields {
        assert!(properties.contains_key(field), "`{field}` is not published");
    }
    assert_eq!(
        schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn the_request_publishes_the_edit_components_it_references() {
    let mut collected = Vec::new();
    <Request as ToSchema>::schemas(&mut collected);
    let names: Vec<&str> = collected.iter().map(|(name, _)| name.as_str()).collect();
    for referenced in [ConfigOp::name(), InstructionEdit::name()] {
        assert!(
            names.contains(&referenced.as_ref()),
            "`{referenced}` is not collected"
        );
    }
    assert_eq!(<Request as ToSchema>::name(), "DraftPatchRequest");
}
