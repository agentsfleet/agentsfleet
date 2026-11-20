//! Model-visible types and daemon decoding agree on the revision representation.

use super::super::tests::candidate;
use super::proposal;
use crate::Result;
use crate::model::provider::tools::{TOOL, tool};
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
        let result = serde_json::to_value(proposal(&call, None, None)?)?;
        assert_eq!(result.get("reason").is_some(), malformed);
        assert_eq!(result.get("proposal").is_some(), !malformed);
        if malformed {
            assert_eq!(result.get("reason"), Some(&json!("invalid_arguments")));
        }
    }
    Ok(())
}

// Value: protects=valid native edits decode while numeric revisions cannot be coerced;
// fails_when=the tool decoder disagrees with its schema or weakens native validation.
#[test]
fn structured_edits_decode_without_weakening_native_revision_validation()
-> Result<(), Box<dyn std::error::Error>> {
    let base = candidate("")?;
    let accepted = proposal(&edit(&json!("1"))?, Some(&base), None)?;
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
            serde_json::to_value(proposal(&edit(&invalid)?, Some(&base), None)?)?,
            json!({"id":"proposal-fixture","reason":"invalid_arguments"})
        );
    }
    Ok(())
}
