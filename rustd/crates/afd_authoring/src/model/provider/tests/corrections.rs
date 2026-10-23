//! Recoverable edit errors retain call identity while withholding executable changes.

use super::super::edits::tests::candidate;
use super::proposal;
use crate::{Result, model::provider::prompt::TOOL};
use rig_core::message::{CallId, ToolCall, ToolFunction, ToolName};
use serde_json::{Value, json};

// Value: protects=incorrect source text or revision shape gives bounded correction feedback;
// fails_when=one model argument error ends the whole private conversation.
#[test]
fn invalid_model_arguments_become_nonexecutable_refusals() -> Result<(), Box<dyn std::error::Error>>
{
    let base = candidate("Keep the existing rule.")?;
    for (revision, anchor, reason) in [
        (json!("1"), "missing text", "instructions_missing"),
        (json!(1), "Keep", "invalid_arguments"),
    ] {
        let call = ToolCall::new(
            CallId::from_wire("rejected-fixture"),
            ToolFunction::new(
                ToolName::new(TOOL)?,
                json!({"proposal":{
                    "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":revision,
                    "config_ops":[],"instruction_edits":[{"kind":"replace","old_text":anchor,"text":"Changed"}]
                }}),
            ),
        );
        let refused: Value = serde_json::to_value(proposal(&call, Some(&base))?)?;
        assert_eq!(
            refused,
            json!({"outcome":"refused","id":"rejected-fixture","reason":reason})
        );
        assert_eq!(base.definition.instructions_md(), "Keep the existing rule.");
    }
    Ok(())
}
