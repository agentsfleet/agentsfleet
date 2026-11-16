//! A Save is answered, never edited; an edit that breaks the setup names its rule.

use super::super::edits::tests::candidate;
use super::{Decoded, proposal, refuse_for_save, refuse_invalid_edits};
use crate::{
    Result,
    model::{
        ModelCallInput, ModelCallResult,
        provider::tools::{TOOL, tool},
    },
};
use af_wire::authoring::{ModelCall, ModelMessage, ModelRequest, ModelUsage};
use rig_core::message::{CallId, ToolCall, ToolFunction, ToolName};
use serde_json::{Value, json};

/// Text only the person wrote, which no refusal may carry back.
const PRIVATE: &str = "wake-the-night-owl-privately";

fn edit_call(
    config_ops: &Value,
    instruction_edits: &Value,
) -> Result<ToolCall, Box<dyn std::error::Error>> {
    let base = candidate("Keep the existing rule.")?;
    Ok(ToolCall::new(
        CallId::from_wire("edit-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{
                "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
                "config_ops":config_ops,"instruction_edits":instruction_edits
            }}),
        ),
    ))
}

fn config_edit(path: &str, value: &Value) -> Result<ToolCall, Box<dyn std::error::Error>> {
    edit_call(&json!([{"op":"add","path":path,"value":value}]), &json!([]))
}

fn input() -> Result<garde::Valid<ModelCallInput>, Box<dyn std::error::Error>> {
    Ok(garde::Unvalidated::new(ModelCall {
        capability_token: "x".repeat(43),
        attempt_id: "01900000-0000-7000-8000-000000000001".into(),
        fence: "1".into(),
        dispatch_id: "01900000-0000-7000-8000-000000000002".into(),
        call_index: 1,
        request: ModelRequest {
            messages: vec![ModelMessage::User {
                text: "Save".into(),
            }],
            max_output_tokens: 1_024,
        },
    })
    .validate()?)
}

/// What decoding leaves for the checks that follow it: the decoded calls, a stored digest.
fn decoded(calls: &[&ToolCall]) -> Result<Decoded, Box<dyn std::error::Error>> {
    let base = candidate("Keep the existing rule.")?;
    let usage = || ModelUsage::Known {
        input_tokens: 1,
        output_tokens: 1,
        provider_cost_nanos: "1".into(),
    };
    Ok(Decoded {
        result: Some(ModelCallResult {
            dispatch_id: "01900000-0000-7000-8000-000000000002".into(),
            call_index: 1,
            replayed: false,
            compacted: false,
            text: String::new(),
            proposal_calls: calls
                .iter()
                .map(|call| proposal(call, Some(&base), None))
                .collect::<Result<_, _>>()?,
            usage: usage(),
            cut: false,
        }),
        continuation: Some(json!({"digest":"before"})),
        usage: usage(),
        output: 1,
        summary: None,
    })
}

/// The calls a check left, and whether it moved the stored digest.
fn left(decoded: Decoded) -> Result<(Vec<Value>, bool), Box<dyn std::error::Error>> {
    let moved = decoded
        .continuation
        .as_ref()
        .and_then(|native| native.get("digest"))
        != Some(&json!("before"));
    let calls = decoded
        .result
        .map_or_else(Vec::new, |result| result.proposal_calls)
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;
    Ok((calls, moved))
}

/// The one call after the setup check ran, and whether its stored digest moved.
fn checked(call: &ToolCall) -> Result<(Value, bool), Box<dyn std::error::Error>> {
    let mut decoded = decoded(&[call])?;
    refuse_invalid_edits(
        &mut decoded,
        Some(&candidate("Keep the existing rule.")?),
        &input()?,
    )?;
    let (mut calls, moved) = left(decoded)?;
    Ok((calls.pop().ok_or("the call is gone")?, moved))
}

fn cron() -> Value {
    json!([{"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":PRIVATE}])
}

// Value: protects=an edit with no operation is told what it lacks, once the intent check has let it through;
// fails_when=a no-operation edit is proposed, or refused before the intent check sees it.
#[test]
fn an_edit_with_no_operation_is_refused_with_the_rule() -> Result<(), Box<dyn std::error::Error>> {
    let (refused, moved) = checked(&edit_call(&json!([]), &json!([]))?)?;
    assert_eq!(refused.get("reason"), Some(&json!("invalid_arguments")));
    let text = refused
        .pointer("/detail/text")
        .and_then(Value::as_str)
        .ok_or("a refusal with no sentence")?;
    assert!(text.contains("at least one operation"), "{text}");
    assert!(moved);
    Ok(())
}

fn initialize_call() -> Result<ToolCall, Box<dyn std::error::Error>> {
    let base = candidate("Keep the existing rule.")?;
    Ok(ToolCall::new(
        CallId::from_wire("first-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{"kind":"initialize","definition":base.definition}}),
        ),
    ))
}

// Value: protects=a typed Save is answered in text whatever the model proposes and whether or not an intent model exists;
// fails_when=a proposal on a Save turn is kept, corrected as malformed, or left for the intent check.
#[test]
fn a_save_refuses_every_proposal_as_no_change_requested() -> Result<(), Box<dyn std::error::Error>>
{
    let edit = config_edit("/config/x-agentsfleet/triggers", &cron())?;
    let first = initialize_call()?;
    let mut decoded = decoded(&[&edit, &first])?;
    refuse_for_save(&mut decoded, &input()?)?;
    let (calls, moved) = left(decoded)?;
    assert_eq!(
        calls,
        vec![
            json!({"id":"edit-fixture","reason":"edit_not_requested"}),
            json!({"id":"first-fixture","reason":"edit_not_requested"}),
        ]
    );
    assert!(moved, "the stored digest still names the proposed calls");
    Ok(())
}

// Value: protects=a Save turn's text answer, and a call already refused for its own reason, are left exactly as they were;
// fails_when=the Save refusal rewrites a refused call's reason or touches the stored digest without a change.
#[test]
fn a_save_leaves_an_answer_and_an_earlier_refusal_alone() -> Result<(), Box<dyn std::error::Error>>
{
    let mut none = decoded(&[])?;
    refuse_for_save(&mut none, &input()?)?;
    assert_eq!(left(none)?, (Vec::<Value>::new(), false));

    let base = candidate("Keep the existing rule.")?;
    let stale = ToolCall::new(
        CallId::from_wire("stale-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{
                "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":"9",
                "config_ops":[],"instruction_edits":[{"kind":"append","text":"More."}]
            }}),
        ),
    );
    let mut refused = decoded(&[&stale])?;
    refuse_for_save(&mut refused, &input()?)?;
    assert_eq!(
        left(refused)?,
        (
            vec![json!({"id":"stale-fixture","reason":"stale_candidate"})],
            false
        )
    );
    Ok(())
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
        let (refused, moved) = checked(&config_edit(path, &cron())?)?;
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
        assert!(
            moved,
            "{path}: the stored digest still names the proposed call"
        );
    }
    Ok(())
}

// Value: protects=a schedule added the way the form teaches is proposed, not refused;
// fails_when=the example path in the form stops being a valid edit of a setup without triggers.
#[test]
fn the_form_s_example_adds_a_schedule_to_a_setup_that_has_none()
-> Result<(), Box<dyn std::error::Error>> {
    let (proposed, moved) = checked(&config_edit("/config/x-agentsfleet/triggers", &cron())?)?;
    assert_eq!(proposed.get("id"), Some(&json!("edit-fixture")));
    assert!(proposed.get("proposal").is_some(), "{proposed}");
    assert!(proposed.get("reason").is_none(), "{proposed}");
    assert!(!moved, "an unchanged call keeps its stored digest");
    Ok(())
}

/// One schedule, as the form teaches it.
fn trigger() -> Value {
    json!({"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":PRIVATE})
}

// Value: protects=a list the model wrote as an object wrapping `item` is the list it meant;
// fails_when=a wrapped entry or list reaches the checker as an object, or a real object is unwrapped.
#[test]
fn a_value_wrapped_in_item_is_the_list_it_names() {
    use crate::model::provider::schema::settle;
    let edit = |value: Value| json!({"proposal":{"kind":"edit","config_ops":[{"op":"add","path":"/config/x-agentsfleet/triggers","value":value}],"instruction_edits":[]}});
    assert_eq!(
        settle(edit(json!({"item": trigger()}))),
        edit(json!([trigger()]))
    );
    assert_eq!(
        settle(edit(json!({"item": [trigger()]}))),
        edit(json!([trigger()]))
    );
    for untouched in [
        json!({"item": 1, "more": 2}),
        json!({"daily_nanos": "1"}),
        json!([trigger()]),
        json!("text"),
    ] {
        assert_eq!(settle(edit(untouched.clone())), edit(untouched));
    }
}

// Value: protects=adding a schedule to a fleet that has no triggers works the way the model writes it;
// fails_when=a wrapped trigger reaches the setup check as an object and the edit is refused as unreadable.
#[test]
fn a_wrapped_trigger_on_a_fleet_without_triggers_is_a_valid_edit()
-> Result<(), Box<dyn std::error::Error>> {
    let wrapped = json!({"item": trigger()});
    let (proposed, moved) = checked(&config_edit("/config/x-agentsfleet/triggers", &wrapped)?)?;
    assert!(proposed.get("proposal").is_some(), "{proposed}");
    assert!(proposed.get("reason").is_none(), "{proposed}");
    assert!(!moved);
    Ok(())
}

// Value: protects=an edit that leaves out a list it does not use is read as an empty list, as the form says;
// fails_when=a call without instruction_edits or config_ops is refused as malformed, or a first setup gains the lists.
#[test]
fn an_edit_that_leaves_out_a_list_means_none_of_that_kind() {
    use crate::model::provider::schema::settle;
    let edit = |members: Value| {
        let mut proposal =
            json!({"kind":"edit","base_candidate_id":"x","base_candidate_revision":"1"});
        if let (Some(base), Some(extra)) = (proposal.as_object_mut(), members.as_object()) {
            base.extend(extra.clone());
        }
        json!({"proposal": proposal})
    };
    let only_config = settle(edit(
        json!({"config_ops":[{"op":"remove","path":"/config/x"}]}),
    ));
    assert_eq!(
        only_config.pointer("/proposal/instruction_edits"),
        Some(&json!([]))
    );
    let only_text = settle(edit(
        json!({"instruction_edits":[{"kind":"append","text":"More."}]}),
    ));
    assert_eq!(only_text.pointer("/proposal/config_ops"), Some(&json!([])));
    let neither = settle(edit(json!({})));
    assert_eq!(neither.pointer("/proposal/config_ops"), Some(&json!([])));
    assert_eq!(
        neither.pointer("/proposal/instruction_edits"),
        Some(&json!([]))
    );
    let first = settle(json!({"proposal":{"kind":"initialize"}}));
    assert_eq!(first.pointer("/proposal/config_ops"), None);
}

// Value: protects=a schedule edit that names no instruction change is proposed, as the model writes it;
// fails_when=the missing instruction_edits list makes decoding refuse an edit whose operation is right.
#[test]
fn a_schedule_edit_without_instruction_edits_is_proposed() -> Result<(), Box<dyn std::error::Error>>
{
    let base = candidate("Keep the existing rule.")?;
    let call = ToolCall::new(
        CallId::from_wire("edit-fixture"),
        ToolFunction::new(
            ToolName::new(TOOL)?,
            json!({"proposal":{
                "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
                "config_ops":[{"op":"add","path":"/config/x-agentsfleet/triggers","value":[trigger()]}]
            }}),
        ),
    );
    let (proposed, _) = checked(&call)?;
    assert!(proposed.get("proposal").is_some(), "{proposed}");
    Ok(())
}

// Value: protects=the model is not told that chat or steer messages are triggers;
// fails_when=the triggers setting drops the sentence that says they never are.
#[test]
fn the_form_says_chat_is_never_a_trigger() {
    let block = "/parameters/properties/proposal/properties/definition/properties/config/properties/x-agentsfleet/properties/triggers/description";
    let help = tool(false)
        .pointer(block)
        .and_then(Value::as_str)
        .map(str::to_owned);
    assert!(help.is_some_and(|text| text.contains("Chat and steer messages always reach a fleet")),);
    assert_eq!(
        tool(true).pointer("/parameters/properties/proposal/required"),
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
