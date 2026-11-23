//! The setup report's Definition bound: what the Definition parser admits, a
//! report carries.

use std::borrow::Cow;

use af_validate::{DESCRIPTION_MAX_CHARS, TEXT_MAX_CHARS};
use af_wire::run::{SETUP_DEFINITION_BYTES_MAX, SetupProposal, SetupReport};
use garde::Validate as _;

/// A Definition whose description and instructions sit at their caps, every
/// character `widest`.
fn definition(widest: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "description": widest.repeat(DESCRIPTION_MAX_CHARS),
        "instructions_md": widest.repeat(TEXT_MAX_CHARS),
        "config": {"name": "maximal"},
    })
    .to_string()
}

fn report(definition: String) -> SetupReport<'static> {
    SetupReport {
        proposal: Some(SetupProposal {
            base_candidate_id: None,
            base_revision: 0,
            definition: Cow::Owned(definition),
        }),
    }
}

/// Instructions take 1,048,576 characters; a proposal carrying that many
/// passes the report bound, even with every character at its widest JSON
/// escape.
#[test]
fn a_proposal_with_instructions_at_the_text_ceiling_passes_the_report_bound() {
    assert_eq!(TEXT_MAX_CHARS, 1_048_576);
    for widest in ["a", "\u{1}"] {
        let text = definition(widest);
        assert!(text.len() > 128 * 1024, "past the old 128 KiB cap");
        assert!(
            report(text).validate().is_ok(),
            "{widest:?} at the ceiling is refused"
        );
    }
}

/// The bound still refuses one byte past it.
#[test]
fn a_proposal_one_byte_past_the_bound_is_refused() {
    let at_limit = "a".repeat(SETUP_DEFINITION_BYTES_MAX);
    assert!(report(at_limit).validate().is_ok(), "at the bound");
    let past = "a".repeat(SETUP_DEFINITION_BYTES_MAX + 1);
    assert!(report(past).validate().is_err(), "one byte past the bound");
}
