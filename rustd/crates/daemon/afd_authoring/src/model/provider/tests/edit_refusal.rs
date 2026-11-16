//! An edit that would leave the setup invalid is refused with the checker's sentence.

use super::super::edits::tests::candidate;
use super::proposal;
use crate::{
    Result,
    model::provider::tools::{TOOL, tool},
};
use rig_core::message::{CallId, ToolCall, ToolFunction, ToolName};
use serde_json::{Value, json};

/// Text only the person wrote, which no refusal may carry back.
const PRIVATE: &str = "wake-the-night-owl-privately";

fn config_edit(path: &str, value: &Value) -> Result<ToolCall, Box<dyn std::error::Error>> {
    let base = candidate("Keep the existing rule.")?;
    Ok(ToolCall::new(
        CallId::from_wire("edit-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{
                "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
                "config_ops":[{"op":"add","path":path,"value":value}],"instruction_edits":[]
            }}),
        ),
    ))
}

fn decoded(path: &str, value: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let base = candidate("Keep the existing rule.")?;
    Ok(serde_json::to_value(proposal(
        &config_edit(path, value)?,
        Some(&base),
        None,
    )?)?)
}

fn cron() -> Value {
    json!([{"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":PRIVATE}])
}

// Value: protects=the model learns which rule an edit broke before commit refuses it blind;
// fails_when=an edit with an unwritable path or a misplaced append reaches commit, or its refusal carries no reason or the typed text.
#[test]
fn an_edit_the_checker_refuses_names_the_rule_and_never_the_value()
-> Result<(), Box<dyn std::error::Error>> {
    for (path, reason) in [
        (
            "/x-agentsfleet/triggers",
            "only config descendants and the description are writable",
        ),
        (
            "/config/x-agentsfleet/triggers/-",
            "the stored configuration could not be read",
        ),
    ] {
        let refused = decoded(path, &cron())?;
        assert_eq!(
            refused.get("reason"),
            Some(&json!("invalid_arguments")),
            "{path}"
        );
        let text = refused
            .pointer("/detail/text")
            .and_then(Value::as_str)
            .ok_or("a refusal with no sentence")?;
        assert!(text.contains(reason), "{path}: {text}");
        assert!(!refused.to_string().contains(PRIVATE), "{path}");
    }
    Ok(())
}

// Value: protects=a schedule added the way the form teaches is proposed, not refused;
// fails_when=the example path in the form stops being a valid edit of a setup without triggers.
#[test]
fn the_form_s_example_adds_a_schedule_to_a_setup_that_has_none()
-> Result<(), Box<dyn std::error::Error>> {
    let proposed = decoded("/config/x-agentsfleet/triggers", &cron())?;
    assert_eq!(proposed.get("id"), Some(&json!("edit-fixture")));
    assert!(proposed.get("proposal").is_some(), "{proposed}");
    assert!(proposed.get("reason").is_none(), "{proposed}");
    Ok(())
}

// Value: protects=the model is told the document root, which every edit path starts from;
// fails_when=the path description drops the /config root or the add-a-member rule.
#[test]
fn the_edit_form_teaches_where_a_path_starts() {
    let schema = tool(true);
    for alternative in ["0", "1"] {
        let help = schema
            .pointer(&format!(
                "/parameters/properties/proposal/properties/config_ops/items/oneOf/{alternative}/properties/path/description"
            ))
            .and_then(Value::as_str);
        assert!(
            help.is_some_and(|text| text.contains("/config/x-agentsfleet/triggers")),
            "{alternative}"
        );
    }
}
