//! The form the model fills: only the action the draft allows, the native
//! types and the help that keeps a plain fleet plain.

use serde_json::{Value, json};

use super::parameters;
use crate::setup::DESCRIPTION;

/// The form's sentence for every optional setting.
const OMIT: &str = "Omit unless the person asks for it.";
/// Where a first setup's runtime settings sit in the form.
const BLOCK: &str =
    "/properties/proposal/properties/definition/properties/config/properties/x-agentsfleet";

// Value: protects=a plain fleet is offered only the settings a person asks for;
// fails_when=the form shows gates as open objects, requires tools or advertises defaults.
#[test]
fn the_first_setup_form_asks_only_for_a_name_and_a_budget() {
    let schema = parameters(false);
    assert_eq!(
        schema.pointer(&format!("{BLOCK}/required")),
        Some(&json!(["budget"]))
    );
    assert!(
        schema
            .pointer(&format!("{BLOCK}/properties/gates"))
            .is_none()
    );
    for setting in ["tools", "credentials", "network"] {
        assert_eq!(
            schema.pointer(&format!("{BLOCK}/properties/{setting}/description")),
            Some(&json!(OMIT)),
            "{setting}"
        );
    }
    // Triggers add the one thing a plain fleet is tempted to declare: chat is never one.
    let triggers = schema
        .pointer(&format!("{BLOCK}/properties/triggers/description"))
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
        .pointer(&format!("{BLOCK}/description"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        placement.contains("budget included, never beside it"),
        "{placement}"
    );
    let text = schema.to_string();
    assert!(
        !text.contains("Default ") && !DESCRIPTION.contains("Default "),
        "a description advertises a default"
    );
    assert!(
        DESCRIPTION.contains("ask them one question instead of calling this tool"),
        "{DESCRIPTION}"
    );
    assert!(
        DESCRIPTION.contains("ask them to confirm or change it before they save"),
        "{DESCRIPTION}"
    );
    assert!(text.contains("never invent approval or hashes"));
}

// Value: protects=new drafts expose native policy types before a paid tool call;
// fails_when=the model guesses empty strings for arrays or a number for a decimal string.
#[test]
fn initialization_exposes_policy_collection_and_scalar_types() {
    let schema = parameters(false);
    let block = format!("{BLOCK}/properties");
    for path in [
        "tools",
        "credentials",
        "triggers",
        "network/properties/allow",
    ] {
        assert_eq!(
            schema.pointer(&format!("{block}/{path}/type")),
            Some(&json!("array"))
        );
    }
    assert_eq!(
        schema.pointer(&format!("{block}/budget/properties/daily_nanos/type")),
        Some(&json!("string"))
    );
}

// Value: protects=customer-requested HTTP settings are visible to the setup model;
// fails_when=the closed proposal schema excludes fields accepted by native validation.
#[test]
fn initialization_exposes_reviewed_http_destination_settings() {
    let schema = parameters(false);
    let network = format!("{BLOCK}/properties/network/properties");
    assert_eq!(
        schema.pointer(&format!("{network}/http_egress/enum")),
        Some(&json!(["restricted", "public"]))
    );
    assert_eq!(
        schema.pointer(&format!("{network}/package_manager_domains/type")),
        Some(&json!("boolean"))
    );
}

// Value: protects=only the action permitted by the locked draft consumes context;
// fails_when=initialization fields are sent during edits or edits without a base.
#[test]
fn proposal_shapes_match_the_authoritative_candidate_state() {
    for (editing, kind) in [(false, "initialize"), (true, "edit")] {
        let schema = parameters(editing);
        let fields = "/properties/proposal/properties";
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

// Value: protects=the model selects exact source text instead of guessing UTF-8 offsets;
// fails_when=the tool requires byte arithmetic that can cut intact instruction words.
#[test]
fn instruction_edits_use_exact_text_anchors_instead_of_model_counted_offsets() {
    let schema = parameters(true);
    let fields = "/properties/proposal/properties/instruction_edits/items/oneOf/0/properties";
    assert_eq!(
        schema.pointer(&format!("{fields}/old_text/type")),
        Some(&json!("string"))
    );
    assert!(schema.pointer(&format!("{fields}/start_byte")).is_none());
    assert!(schema.pointer(&format!("{fields}/end_byte")).is_none());
}

// Value: protects=the model is not told that chat or steer messages are triggers;
// fails_when=the triggers setting drops the sentence that says they never are.
#[test]
fn the_form_says_chat_is_never_a_trigger() {
    let help = parameters(false)
        .pointer(&format!("{BLOCK}/properties/triggers/description"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    assert!(help.is_some_and(|text| text.contains("Chat and steer messages always reach a fleet")));
    assert_eq!(
        parameters(true).pointer("/properties/proposal/required"),
        Some(&json!([
            "kind",
            "base_candidate_id",
            "base_candidate_revision"
        ]))
    );
}

// Value: protects=the model is told the document root, which every edit path starts from;
// fails_when=the path description drops the /config root or the add-a-member rule.
#[test]
fn the_edit_form_teaches_where_a_path_starts() {
    let schema = parameters(true);
    for alternative in ["0", "1"] {
        let help = schema
            .pointer(&format!(
                "/properties/proposal/properties/config_ops/items/oneOf/{alternative}/properties/path/description"
            ))
            .and_then(Value::as_str);
        assert!(
            help.is_some_and(|text| text.contains("/config/x-agentsfleet/triggers")),
            "{alternative}"
        );
    }
}
