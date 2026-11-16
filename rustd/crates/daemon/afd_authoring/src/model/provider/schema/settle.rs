//! A first setup's omitted settings mean none before the native checker reads it.
//!
//! The form tells the model to leave out what the person did not ask for. A
//! stored document must still say outright that a fleet has no tools, and an
//! empty repository list is refused there, because a binding that names nothing
//! is not "every repository". In a model's first setup only, an omitted tool
//! list and an empty repository list both mean none, so the model is never
//! refused for doing what the form asked.

use serde_json::{Map, Value, json};

use super::{
    KEY_CONFIG, KEY_DEFINITION, KEY_KIND, KEY_POLICY, KEY_REPOSITORIES, KEY_TOOLS, KIND_INITIALIZE,
    PROPOSAL_ARGUMENT, REPOSITORY_SETTINGS,
};

/// The tool arguments with a first setup's omissions spelled as none.
pub(in crate::model::provider) fn settle(mut arguments: Value) -> Value {
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
