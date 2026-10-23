//! Model-visible types and daemon decoding agree on the revision representation.

use super::super::edits::tests::candidate;
use super::proposal;
use crate::Result;
use crate::model::provider::prompt::{TOOL, tool};
use rig_core::message::{CallId, ToolCall, ToolFunction, ToolName};
use serde_json::{Value, json};

fn edit(revision: &Value) -> Result<ToolCall, Box<dyn std::error::Error>> {
    Ok(ToolCall::new(
        CallId::from_wire("proposal-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{
                "kind":"edit", "base_candidate_id":"01900000-0000-7000-8000-000000000001",
                "base_candidate_revision":revision, "config_ops":[],
                "instruction_edits":[{"kind":"replace","old_text":"","text":"Answer directly."}]
            }}),
        ),
    ))
}

// Value: protects=an edit's revision is visible as a string in the model's schema;
// fails_when=an opaque JSON string hides the native field types; seam=tool boundary.
#[test]
fn the_model_sees_the_native_edit_revision_type() {
    let schema = tool(true);
    assert_eq!(
        schema.pointer("/parameters/properties/proposal/properties/base_candidate_revision/type"),
        Some(&json!("string"))
    );
    assert!(
        schema
            .pointer("/parameters/properties/proposal_json")
            .is_none()
    );
}

// Value: protects=new drafts expose native policy types before a paid tool call;
// fails_when=the model guesses empty strings for arrays or quotes a gate timeout.
#[test]
fn initialization_exposes_policy_collection_and_scalar_types() {
    let schema = tool(false);
    let block = "/parameters/properties/proposal/properties/definition/properties/config/properties/x-agentsfleet/properties";
    for path in [
        "tools",
        "credentials",
        "triggers",
        "network/properties/allow",
        "gates/properties/rules",
    ] {
        assert_eq!(
            schema.pointer(&format!("{block}/{path}/type")),
            Some(&json!("array"))
        );
    }
    assert_eq!(
        schema.pointer(&format!("{block}/gates/properties/timeout_ms/type")),
        Some(&json!("integer"))
    );
    assert_eq!(
        schema.pointer(&format!("{block}/budget/properties/daily_nanos/type")),
        Some(&json!("string"))
    );
}

// Value: protects=only the action permitted by the locked draft consumes context;
// fails_when=initialization fields are sent during edits or edits without a base.
#[test]
fn proposal_shapes_match_the_authoritative_candidate_state() {
    for (editing, kind) in [(false, "initialize"), (true, "edit")] {
        let schema = tool(editing);
        let fields = "/parameters/properties/proposal/properties";
        assert_eq!(
            schema.pointer(&format!("{fields}/kind/enum")),
            Some(&json!([kind]))
        );
        assert_eq!(
            schema.pointer(&format!("{fields}/definition")).is_some(),
            !editing
        );
        assert_eq!(
            schema
                .pointer(&format!("{fields}/base_candidate_id"))
                .is_some(),
            editing
        );
    }
}

// Value: protects=typed initial policy survives native decoding, while malformed
// empty collections cannot be coerced into authority by the provider adapter.
#[test]
fn initialization_requires_native_policy_types() -> Result<(), Box<dyn std::error::Error>> {
    let definition = serde_json::to_value(candidate("Keep replies concise.")?.definition)?;
    for malformed in [false, true] {
        let mut proposed = definition.clone();
        if malformed {
            *proposed
                .pointer_mut("/config/x-agentsfleet/tools")
                .ok_or("missing tools")? = json!("");
        }
        let call = ToolCall::new(
            CallId::from_wire("initialize-fixture"),
            ToolFunction::new(
                ToolName::new(TOOL)?,
                json!({"proposal":{"kind":"initialize","definition":proposed}}),
            ),
        );
        let result = serde_json::to_value(proposal(&call, None)?)?;
        assert_eq!(result.get("reason").is_some(), malformed);
        assert_eq!(result.get("proposal").is_some(), !malformed);
        if malformed {
            assert_eq!(result.get("reason"), Some(&json!("invalid_arguments")));
        }
    }
    Ok(())
}

// Value: protects=the model selects exact source text instead of guessing UTF-8 offsets;
// fails_when=the tool requires byte arithmetic that can cut intact instruction words.
#[test]
fn instruction_edits_use_exact_text_anchors_instead_of_model_counted_offsets() {
    let schema = tool(true);
    let fields =
        "/parameters/properties/proposal/properties/instruction_edits/items/oneOf/0/properties";
    assert_eq!(
        schema.pointer(&format!("{fields}/old_text/type")),
        Some(&json!("string"))
    );
    assert!(schema.pointer(&format!("{fields}/start_byte")).is_none());
    assert!(schema.pointer(&format!("{fields}/end_byte")).is_none());
}

// Value: protects=valid native edits decode while numeric revisions cannot be coerced;
// fails_when=the tool decoder disagrees with its schema or weakens native validation.
#[test]
fn structured_edits_decode_without_weakening_native_revision_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let base = candidate("")?;
    let accepted = proposal(&edit(&json!("1"))?, Some(&base))?;
    assert_eq!(
        serde_json::to_value(accepted)?.pointer("/proposal/base_candidate_revision"),
        Some(&json!("1"))
    );
    for invalid in [
        json!(1),
        json!(null),
        json!("1.0"),
        json!("-1"),
        json!(true),
    ] {
        assert_eq!(
            serde_json::to_value(proposal(&edit(&invalid)?, Some(&base))?)?,
            json!({"id":"proposal-fixture","reason":"invalid_arguments"})
        );
    }
    Ok(())
}
