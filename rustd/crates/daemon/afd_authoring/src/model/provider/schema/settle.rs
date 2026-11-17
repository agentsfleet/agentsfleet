//! What the model's call means before the native checker reads it: a first setup's
//! omitted settings mean none, an edit's omitted lists mean none, and an edit's
//! list arrives as a list.
//!
//! The form tells the model to leave out what the person did not ask for. A
//! stored document must still say outright that a fleet has no tools, and an
//! empty repository list is refused there, because a binding that names nothing
//! is not "every repository". In a model's first setup only, an omitted tool
//! list and an empty repository list both mean none, so the model is never
//! refused for doing what the form asked.
//!
//! The model writes a list it was not told is a list as an object that wraps
//! each entry in `item`, its tool-call encoding for repeated elements. An edit's
//! `value` has no declared type, so it is the one place that happens; replaying
//! the same model over the typed first-setup form wrote real arrays every time.

use serde_json::{Map, Value, json};

use super::{
    KEY_CONFIG, KEY_CONFIG_OPS, KEY_DEFINITION, KEY_INSTRUCTION_EDITS, KEY_KIND, KEY_POLICY,
    KEY_REPOSITORIES, KEY_TOOLS, KEY_VALUE, KIND_EDIT, KIND_INITIALIZE, PROPOSAL_ARGUMENT,
    REPOSITORY_SETTINGS,
};

/// The key the model wraps a list's entries in.
const ITEM: &str = "item";

/// The tool arguments with a first setup's omissions spelled as none, an edit's
/// omitted lists spelled as empty and each edited value's wrapped list unwrapped.
pub(in crate::model::provider) fn settle(mut arguments: Value) -> Value {
    if let Some(proposal) = edit_proposal(&mut arguments) {
        for list in [KEY_CONFIG_OPS, KEY_INSTRUCTION_EDITS] {
            proposal.entry(list).or_insert_with(|| json!([]));
        }
    }
    for operation in edit_operations(&mut arguments) {
        if let Some(value) = operation.get_mut(KEY_VALUE) {
            unwrap_item(value);
        }
    }
    if let Some(settings) = first_setup_settings(&mut arguments) {
        settings.entry(KEY_TOOLS).or_insert_with(|| json!([]));
        if settings
            .get(KEY_REPOSITORIES)
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            settings.remove(KEY_REPOSITORIES);
            for setting in REPOSITORY_SETTINGS {
                settings.remove(setting);
            }
        }
    }
    arguments
}

/// `{"item": entry}` is the list `[entry]`, and `{"item": [entries]}` is that list.
fn unwrap_item(value: &mut Value) {
    let Some(entries) = value
        .as_object_mut()
        .filter(|members| members.len() == 1)
        .and_then(|members| members.remove(ITEM))
    else {
        return;
    };
    *value = match entries {
        Value::Array(list) => Value::Array(list),
        entry => Value::Array(vec![entry]),
    };
}

/// An edit proposal's members; a first setup or a malformed call has none.
fn edit_proposal(arguments: &mut Value) -> Option<&mut Map<String, Value>> {
    let proposal = arguments.get_mut(PROPOSAL_ARGUMENT)?;
    if proposal.get(KEY_KIND).and_then(Value::as_str) != Some(KIND_EDIT) {
        return None;
    }
    proposal.as_object_mut()
}

/// The configuration operations of an edit; a first setup or a malformed call has none.
fn edit_operations(arguments: &mut Value) -> impl Iterator<Item = &mut Value> {
    arguments
        .get_mut(PROPOSAL_ARGUMENT)
        .and_then(|proposal| proposal.get_mut(KEY_CONFIG_OPS))
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
}

/// The `x-agentsfleet` block of a first setup; an edit or a malformed call has none.
fn first_setup_settings(arguments: &mut Value) -> Option<&mut Map<String, Value>> {
    let proposal = arguments.get_mut(PROPOSAL_ARGUMENT)?;
    if proposal.get(KEY_KIND).and_then(Value::as_str) != Some(KIND_INITIALIZE) {
        return None;
    }
    proposal
        .get_mut(KEY_DEFINITION)?
        .get_mut(KEY_CONFIG)?
        .get_mut(KEY_POLICY)?
        .as_object_mut()
}
