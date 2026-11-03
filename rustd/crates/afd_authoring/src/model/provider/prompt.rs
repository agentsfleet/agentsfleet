//! Versioned daemon instructions and the single proposal tool.

use super::{KEY_DESCRIPTION, KEY_TYPE};
use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::config::{
    KIND_CRON, KIND_MENTION, KIND_WEBHOOK, WEBHOOK_SOURCE_GITHUB, WEBHOOK_SOURCE_LINEAR,
    WEBHOOK_SOURCE_SLACK,
};
use afd_fleet_runtime::definition::{MAX_INITIAL_INSTRUCTIONS_CHARS, MAX_INSTRUCTIONS_CHARS};
use serde_json::{Value, json};

pub(in crate::model) const TOOL: &str = "propose_setup";
const INITIAL_GATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

// Only platform authoring guidance is compiled in; customer instructions live in revisions.
const SYSTEM: &str = include_str!("prompt/system.md");
const SAVE: &str = include_str!("prompt/save.md");
const CONVERSATION: &str = include_str!("prompt/conversation.md");

fn initial() -> String {
    format!(
        include_str!("prompt/initialize.md"),
        gate_timeout_ms = INITIAL_GATE_TIMEOUT.as_millis(),
        daily_nanos = NANOS_PER_USD,
        webhook = KIND_WEBHOOK,
        github = WEBHOOK_SOURCE_GITHUB,
        linear = WEBHOOK_SOURCE_LINEAR,
        slack = WEBHOOK_SOURCE_SLACK,
        cron = KIND_CRON,
        mention = KIND_MENTION,
        instructions_chars = MAX_INITIAL_INSTRUCTIONS_CHARS,
    )
}

pub(in crate::model) fn system(editing: bool, save: bool) -> String {
    let guidance = if editing {
        format!(
            include_str!("prompt/edit.md"),
            instructions_chars = MAX_INSTRUCTIONS_CHARS
        )
    } else {
        initial()
    };
    format!(
        "{SYSTEM}\n{guidance}\n{}\nDeclarable webhook recipes: {}",
        if save { SAVE } else { CONVERSATION },
        afd_fleet_runtime::config::WebhookFamily::ALL
            .iter()
            .map(|family| family.token())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(in crate::model) fn tool(editing: bool) -> Value {
    json!({
        KEY_TYPE:"function", "name":TOOL,
        KEY_DESCRIPTION:"Propose a native setup object for validation. This grants no authority and does not save the fleet.",
        "parameters":super::schema::parameters(editing)
    })
}

#[cfg(test)]
mod tests {
    use afd_fleet_runtime::config::{TRIGGER_KINDS, VERIFIED_WEBHOOK_SOURCES};

    use super::initial;

    #[test]
    fn the_prompt_lists_exactly_the_declarable_families() {
        let prompt = super::system(false, false);
        let recipes = prompt
            .split_once("Declarable webhook recipes: ")
            .map(|(_, recipes)| recipes.split(", ").collect::<Vec<_>>());
        assert_eq!(
            recipes,
            Some(
                afd_fleet_runtime::config::WebhookFamily::ALL
                    .iter()
                    .map(|family| family.token())
                    .collect::<Vec<_>>()
            )
        );
    }

    /// How the prompt opens one trigger shape.
    const SHAPE_OPENER: &str = "type:\"";

    /// The prompt names every trigger kind the parser accepts and no other, so
    /// a model is never told a kind the daemon refuses or kept from one it
    /// accepts. `TRIGGER_KINDS` is pinned to the parser in `afd_fleet_runtime`.
    #[test]
    fn the_prompt_names_exactly_the_trigger_kinds_the_parser_accepts() {
        for kind in TRIGGER_KINDS {
            assert!(
                initial().contains(&format!("{SHAPE_OPENER}{kind}\"")),
                "the prompt omits the `{kind}` trigger"
            );
        }
        assert_eq!(
            initial().matches(SHAPE_OPENER).count(),
            TRIGGER_KINDS.len(),
            "the prompt names a trigger kind the parser does not accept"
        );
    }

    /// The prompt names every webhook source a route can verify unsigned, so a
    /// model never proposes a bare source Save would refuse.
    #[test]
    fn the_prompt_names_every_verified_webhook_source() {
        for source in VERIFIED_WEBHOOK_SOURCES {
            assert!(
                initial().contains(source),
                "the prompt omits the `{source}` webhook source"
            );
        }
    }
}
