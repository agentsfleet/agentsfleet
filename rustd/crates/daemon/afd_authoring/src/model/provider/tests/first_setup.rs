//! A first setup holds only what the person asked for, and a refusal names what to fix.

use super::proposal;
use crate::model::provider::tools::{TOOL, tool};
use af_core::money::NANOS_PER_USD;
use rig_core::message::{CallId, ToolCall, ToolFunction, ToolName};
use serde_json::{Value, json};

/// The form's sentence for every optional setting.
const OMIT: &str = "Omit unless the person asks for it.";

/// A first setup that acknowledges messages, with the configuration a test chooses.
fn first_setup(config: &Value) -> Result<ToolCall, Box<dyn std::error::Error>> {
    Ok(ToolCall::new(
        CallId::from_wire("first-setup"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{"kind":"initialize","definition":{
                "schema_version":1,"description":"Acknowledges each message.",
                "instructions_md":"Reply with ACK and the message.","config":config
            }}}),
        ),
    ))
}

fn budget() -> Value {
    json!({"daily_nanos": NANOS_PER_USD.to_string()})
}

/// The decoded call: `Proposed` keeps its settings, `Refused` its detail.
fn decoded(config: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::to_value(proposal(
        &first_setup(config)?,
        None,
        None,
    )?)?)
}

// Value: protects=a plain fleet is offered only the settings a person asks for;
// fails_when=the form shows gates as open objects, requires tools or advertises defaults.
#[test]
fn the_first_setup_form_asks_only_for_a_name_and_a_budget() {
    let schema = tool(false);
    let block = "/parameters/properties/proposal/properties/definition/properties/config/properties/x-agentsfleet";
    assert_eq!(
        schema.pointer(&format!("{block}/required")),
        Some(&json!(["budget"]))
    );
    assert!(
        schema
            .pointer(&format!("{block}/properties/gates"))
            .is_none()
    );
    for setting in ["tools", "credentials", "network"] {
        assert_eq!(
            schema.pointer(&format!("{block}/properties/{setting}/description")),
            Some(&json!(OMIT)),
            "{setting}"
        );
    }
    // Triggers add the one thing a plain fleet is tempted to declare: chat is never one.
    let triggers = schema
        .pointer(&format!("{block}/properties/triggers/description"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        triggers.starts_with("Omit unless the person asks for one."),
        "{triggers}"
    );
    assert!(
        triggers.contains("Chat and steer messages always reach a fleet"),
        "{triggers}"
    );
    let placement = schema
        .pointer(&format!("{block}/description"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        placement.contains("budget included, never beside it"),
        "{placement}"
    );
    let text = schema.to_string();
    assert!(
        !text.contains("Default "),
        "a description advertises a default"
    );
    let guidance = schema
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        guidance.contains("ask them one question instead of calling this tool"),
        "{guidance}"
    );
    assert!(
        guidance.contains("ask them to confirm or change it before they save"),
        "{guidance}"
    );
    assert!(text.contains("never invent approval or hashes"));
}

// Value: protects=omitting tools and naming no repository mean none in a model's first setup;
// fails_when=the model is refused for leaving out settings the form told it to omit.
#[test]
fn an_omitted_tool_list_and_an_empty_repository_list_mean_none()
-> Result<(), Box<dyn std::error::Error>> {
    let result = decoded(&json!({"name":"message-acknowledger","x-agentsfleet":{
        "budget":budget(),"repositories":[],"repository_access":"read",
        "repository_publication":"event_draft"
    }}))?;
    let settings = result
        .pointer("/proposal/definition/config/x-agentsfleet")
        .ok_or("an accepted first setup")?;
    assert_eq!(settings.get("tools"), Some(&json!([])));
    for gone in [
        "repositories",
        "repository_access",
        "repository_publication",
    ] {
        assert!(settings.get(gone).is_none(), "{gone}");
    }
    Ok(())
}

// Value: protects=a half-declared repository binding is refused with the checker's own rule;
// fails_when=an access level without a repository is dropped silently or refused unnamed.
#[test]
fn an_access_level_without_a_repository_is_refused_by_name()
-> Result<(), Box<dyn std::error::Error>> {
    let result = decoded(&json!({"name":"pull-request-review","x-agentsfleet":{
        "budget":budget(),"repository_access":"read"
    }}))?;
    assert_eq!(result.get("reason"), Some(&json!("invalid_arguments")));
    assert_eq!(
        result.pointer("/detail/field"),
        Some(&json!("repositories"))
    );
    let text = result
        .pointer("/detail/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(text.contains("optional together, not separately"), "{text}");
    Ok(())
}

// Value: protects=a setting written beside the runtime block is named to the model;
// fails_when=the checker's sentence is dropped and the model hears only general rules.
#[test]
fn a_budget_beside_the_runtime_block_is_named_to_the_model()
-> Result<(), Box<dyn std::error::Error>> {
    let result = decoded(&json!({"name":"chat-and-steer","budget":budget(),
        "x-agentsfleet":{"tools":[]}}))?;
    assert_eq!(
        result.get("detail"),
        Some(&json!({"field":"budget",
            "text":"`budget` belongs under the `x-agentsfleet` block, not at the top level"}))
    );
    Ok(())
}

// Value: protects=an invented key inside a setting is named by its path, not just refused;
// fails_when=the model cannot tell which key to remove and shrinks the whole setup instead.
#[test]
fn an_invented_gate_rule_key_is_named_by_its_path() -> Result<(), Box<dyn std::error::Error>> {
    let result = decoded(&json!({"name":"message-acknowledger","x-agentsfleet":{
        "tools":["http_request"],"budget":budget(),
        "gates":{"rules":[{"tool":"http_request","action":"post","severity":"high"}]}
    }}))?;
    assert_eq!(
        result.get("detail"),
        Some(&json!({"field":"gates",
            "text":"`x-agentsfleet.gates.rules.0.severity` is an unsupported configuration key"}))
    );
    Ok(())
}

// Value: protects=a refused value never reaches the model's feedback or the log field;
// fails_when=serde's own message, which quotes the value, is passed through.
#[test]
fn a_value_of_the_wrong_shape_never_reaches_the_refusal() -> Result<(), Box<dyn std::error::Error>>
{
    let private = "Reply to Dana about the March invoice";
    let result = decoded(&json!({"name":"message-acknowledger","x-agentsfleet":{
        "tools":private,"budget":budget()
    }}))?;
    assert_eq!(result.get("reason"), Some(&json!("invalid_arguments")));
    assert!(result.pointer("/detail/field").is_none());
    let text = result
        .pointer("/detail/text")
        .and_then(Value::as_str)
        .ok_or("a detail")?;
    assert!(!text.contains("Dana"), "{text}");
    assert!(!result.to_string().contains("March invoice"));
    Ok(())
}
