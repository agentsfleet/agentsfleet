//! A first setup is proved by the daemon's validator before it is submitted;
//! what the validator says is `af_fleet_runtime::setup`'s, tested there.

use super::initial_refusal;
use crate::{Result, error};
use af_wire::authoring::{Proposal, REFUSAL_TEXT_BYTES_MAX};
use serde_json::{Map, Value, json};

/// A first setup built like `af_fleet_runtime`'s `FIXTURE_CONFIG`, which the
/// daemon accepts, with the description and extra settings a test chooses.
fn setup(description: &str, extra: Option<(&str, Value)>) -> Proposal {
    let mut settings = Map::new();
    settings.insert("triggers".to_owned(), json!([]));
    settings.insert("tools".to_owned(), json!([]));
    settings.insert("budget".to_owned(), json!({"daily_nanos": "1000000000"}));
    if let Some((key, value)) = extra {
        settings.insert(key.to_owned(), value);
    }
    Proposal::Initialize {
        definition: Box::new(json!({
            "schema_version": 1,
            "description": description,
            "instructions_md": "Reply briefly.",
            "config": {"name": "fixture", "x-agentsfleet": settings},
        })),
    }
}

#[test]
fn a_valid_first_setup_is_submitted_unchanged() {
    assert_eq!(
        initial_refusal(&setup("Answers support questions", None)),
        None
    );
}

#[test]
fn an_edit_is_left_to_the_daemon_which_holds_the_current_setup() {
    let edit = Proposal::Edit {
        base_candidate_id: "0".repeat(36),
        base_candidate_revision: "1".to_owned(),
        config_ops: vec![json!({"op": "unknown"})],
        instruction_edits: Vec::new(),
    };
    assert_eq!(initial_refusal(&edit), None);
}

#[test]
fn the_feedback_stays_within_its_bound_on_a_character_boundary() -> Result<()> {
    let key = "é".repeat(2_000);
    let Some(detail) = initial_refusal(&setup(
        "Answers support questions",
        Some((&key, json!(true))),
    )) else {
        return Err(error::config("an unknown key was accepted"));
    };
    assert!(
        detail.text().len() <= REFUSAL_TEXT_BYTES_MAX,
        "{}",
        detail.text().len()
    );
    assert!(detail.text().starts_with("`é"), "{}", detail.text());
    Ok(())
}
