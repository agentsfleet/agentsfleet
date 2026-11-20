//! A first setup is proved by the checker before it is kept, and its refusal
//! names the rule to fix without the value that broke it.

use af_core::error_code;
use serde_json::{Map, Value, json};

use super::{initial_refusal, invalid_setup};
use crate::definition::MAX_INSTRUCTIONS_CHARS;

/// A first setup built like `FIXTURE_CONFIG`, which the checker accepts, with
/// the description and extra settings a test chooses, as JSON text.
fn setup(description: &str, extra: Option<(&str, Value)>) -> String {
    let mut settings = Map::new();
    settings.insert("triggers".to_owned(), json!([]));
    settings.insert("tools".to_owned(), json!([]));
    settings.insert("budget".to_owned(), json!({"daily_nanos": "1000000000"}));
    if let Some((key, value)) = extra {
        settings.insert(key.to_owned(), value);
    }
    json!({
        "schema_version": 1,
        "description": description,
        "instructions_md": "Reply briefly.",
        "config": {"name": "fixture", "x-agentsfleet": settings},
    })
    .to_string()
}

#[test]
fn a_valid_first_setup_is_accepted() {
    let refused = initial_refusal(&setup("Answers support questions", None));

    assert!(refused.is_none(), "{refused:?}");
}

#[test]
fn a_missing_description_names_the_field_to_the_model() -> Result<(), Box<dyn std::error::Error>> {
    let detail = initial_refusal(&setup("", None)).ok_or("an empty description was accepted")?;

    assert_eq!(
        invalid_setup(&detail.told()),
        "No changes were applied. The setup is invalid: `description` is required and was not set. \
         Fix that, keep only the settings the person asked for, and return a corrected propose_setup call."
    );
    assert_eq!(detail.field(), Some("description"));
    Ok(())
}

#[test]
fn an_unknown_setup_key_names_that_key_and_logs_only_the_block()
-> Result<(), Box<dyn std::error::Error>> {
    let unknown = setup(
        "Answers support questions",
        Some(("schedule_hint", json!("Every weekday at nine"))),
    );
    let detail = initial_refusal(&unknown).ok_or("an unknown key was accepted")?;

    assert_eq!(
        detail.told(),
        "`schedule_hint` is not a known `x-agentsfleet` key"
    );
    // The model hears the key it wrote; the log names only the block, and the
    // value it held reaches neither.
    assert_eq!(detail.field(), Some("x-agentsfleet"));
    assert!(!invalid_setup(&detail.told()).contains("Every weekday"));
    Ok(())
}

#[test]
fn a_value_of_the_wrong_shape_is_named_by_position_never_by_value()
-> Result<(), Box<dyn std::error::Error>> {
    let shaped = setup(
        "Answers support questions",
        Some(("tools", json!("Private wording from the person"))),
    );
    let detail = initial_refusal(&shaped).ok_or("a text tool list was accepted")?;

    assert!(
        !detail.told().contains("Private wording"),
        "{}",
        detail.told()
    );
    assert_eq!(detail.field(), None);
    Ok(())
}

#[test]
fn first_instructions_over_the_instruction_bound_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let mut long: Value = serde_json::from_str(&setup("Answers support questions", None))?;
    // Multi-byte characters, so the bound is counted in characters, not bytes.
    *long
        .get_mut("instructions_md")
        .ok_or("the fixture has instructions")? = json!("é".repeat(MAX_INSTRUCTIONS_CHARS + 1));

    let detail =
        initial_refusal(&long.to_string()).ok_or("an oversized first setup was accepted")?;

    assert_eq!(detail.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert!(!detail.told().contains('é'), "{}", detail.told());
    Ok(())
}
