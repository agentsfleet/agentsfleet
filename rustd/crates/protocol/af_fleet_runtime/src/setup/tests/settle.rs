//! What the model's call means before the checker reads it: a wrapped list is
//! the list it names, and an edit's omitted list is an empty one.

use serde_json::{Value, json};

use super::settle;

/// One schedule, as the form teaches it.
fn trigger() -> Value {
    json!({"type":"cron","schedule":"0 9 * * *","timezone":"UTC","message":"wake-the-night-owl-privately"})
}

// Value: protects=a list the model wrote as an object wrapping `item` is the list it meant;
// fails_when=a wrapped entry or list reaches the checker as an object, or a real object is unwrapped.
#[test]
fn a_value_wrapped_in_item_is_the_list_it_names() {
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

// Value: protects=an edit that leaves out a list it does not use is read as an empty list, as the form says;
// fails_when=a call without instruction_edits or config_ops is refused as malformed, or a first setup gains the lists.
#[test]
fn an_edit_that_leaves_out_a_list_means_none_of_that_kind() {
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
