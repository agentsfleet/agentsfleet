//! The daemon's check on a report's proposal: what it accepts, what it refuses,
//! under which code, and what the refusal logs.

use std::borrow::Cow;

use af_core::{error_code, id::Uuid7, test_util::trace::Capture};
use af_wire::run::SetupProposal;
use serde_json::json;

use super::{DETAIL_BASE_MOVED, EVENT_PROPOSAL_REFUSED, Head, Refusal, check};

/// The head candidate every edit case stands on.
const HEAD: &str = "0199a0b0-0000-7000-8000-0000000000c1";
/// Another candidate, which the head moved away from.
const OTHER: &str = "0199a0b0-0000-7000-8000-0000000000c2";
/// The draft and run a refusal names.
const DRAFT: &str = "0199a0b0-0000-7000-8000-0000000000d1";
const RUN: &str = "0199a0b0-0000-7000-8000-0000000000e1";

/// A Definition whose one cron trigger fires on `schedule`, with `instructions`.
fn definition(schedule: &str, instructions: &str) -> String {
    json!({
        "schema_version": 1,
        "description": "Checks the deploy every morning.",
        "instructions_md": instructions,
        "config": {"name": "deploy-check", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": schedule, "timezone": "UTC", "message": "Daily check"}],
            "tools": ["agentmail"],
            "budget": {"daily_nanos": "1000000000"}
        }}
    })
    .to_string()
}

/// A proposal on `base` at `revision` carrying `definition`.
fn proposal<'a>(base: Option<&'a str>, revision: u64, definition: &'a str) -> SetupProposal<'a> {
    SetupProposal {
        base_candidate_id: base.map(Cow::Borrowed),
        base_revision: revision,
        definition: Cow::Borrowed(definition),
    }
}

fn uuid(text: &str) -> Result<Uuid7, Box<dyn std::error::Error>> {
    Ok(Uuid7::parse(text)?)
}

/// Whether `outcome` is a refusal under `code`.
fn refused_with<T>(outcome: &Result<T, Refusal>, code: error_code::ErrorCode) -> bool {
    matches!(outcome, Err(refusal) if refusal.code == code)
}

#[test]
fn a_proposal_on_the_head_is_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let head = uuid(HEAD)?;
    let text = definition("0 9 * * *", "Check the deploy.");
    let accepted = check(
        Head {
            candidate: Some(&head),
            revision: 2,
        },
        &proposal(Some(HEAD), 2, &text),
    );
    assert!(
        matches!(&accepted, Ok(parsed) if parsed.instructions_md() == "Check the deploy."),
        "{accepted:?}"
    );
    Ok(())
}

/// A base candidate that is no longer the head, a revision that moved, and a
/// first-setup proposal on a draft that has a head now each answer
/// `UZ-AUTHOR-003` with the detail that sends the person to ask again.
#[test]
fn a_proposal_off_the_head_is_refused_as_stale() -> Result<(), Box<dyn std::error::Error>> {
    let head = uuid(HEAD)?;
    let on_head = Head {
        candidate: Some(&head),
        revision: 2,
    };
    let text = definition("0 9 * * *", "Check the deploy.");
    for stale in [
        proposal(Some(OTHER), 2, &text),
        proposal(Some(HEAD), 1, &text),
        proposal(None, 0, &text),
    ] {
        let outcome = check(on_head, &stale);
        assert!(
            refused_with(&outcome, error_code::AUTHORING_STALE),
            "{outcome:?}"
        );
        assert!(
            matches!(&outcome, Err(refusal) if refusal.detail() == DETAIL_BASE_MOVED),
            "{outcome:?}"
        );
    }
    let first = check(
        Head {
            candidate: None,
            revision: 0,
        },
        &proposal(Some(HEAD), 2, &text),
    );
    assert!(
        refused_with(&first, error_code::AUTHORING_STALE),
        "an edit on a draft with no head yet: {first:?}"
    );
    Ok(())
}

/// A Definition the runner accepted and the daemon's parser refuses keeps the
/// parser's code and names the field the model got wrong.
#[test]
fn a_definition_the_parser_refuses_keeps_its_code_and_field()
-> Result<(), Box<dyn std::error::Error>> {
    let head = uuid(HEAD)?;
    let text = definition("61 * * * *", "Check the deploy.");
    let outcome = check(
        Head {
            candidate: Some(&head),
            revision: 2,
        },
        &proposal(Some(HEAD), 2, &text),
    );
    assert!(
        refused_with(&outcome, error_code::AGENTSFLEET_INVALID_CONFIG),
        "{outcome:?}"
    );
    assert!(
        matches!(&outcome, Err(refusal) if refusal.detail().contains("`schedule`")),
        "{outcome:?}"
    );
    Ok(())
}

/// Text that is no Definition at all, and an empty one, are the parser's
/// refusals too, and neither detail quotes the text it refused.
#[test]
fn text_that_is_no_definition_is_refused_without_being_quoted()
-> Result<(), Box<dyn std::error::Error>> {
    let head = uuid(HEAD)?;
    let on_head = Head {
        candidate: Some(&head),
        revision: 2,
    };
    for text in ["{\"schema_version\": \"wake-the-night-owl\"", "", "🦉"] {
        let outcome = check(on_head, &proposal(Some(HEAD), 2, text));
        assert!(
            refused_with(&outcome, error_code::AGENTSFLEET_INVALID_CONFIG),
            "{outcome:?}"
        );
        assert!(
            matches!(&outcome, Err(refusal) if !refusal.detail().contains("night-owl")),
            "{outcome:?}"
        );
    }
    Ok(())
}

/// A first setup and an edit share one instruction bound: the bound itself is
/// kept, and one character past it is refused.
#[test]
fn a_first_setup_takes_the_same_instruction_bound_as_an_edit()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = af_fleet_runtime::definition::MAX_INSTRUCTIONS_CHARS;
    let first = |instructions: &str| {
        check(
            Head {
                candidate: None,
                revision: 0,
            },
            &proposal(None, 0, &definition("0 9 * * *", instructions)),
        )
    };
    let full = "x".repeat(bound);
    assert!(
        matches!(&first(&full), Ok(parsed) if parsed.instructions_md().len() == bound),
        "a first setup may carry the full bound"
    );
    let over = first(&"x".repeat(bound + 1));
    assert!(
        refused_with(&over, error_code::AGENTSFLEET_INVALID_CONFIG),
        "{over:?}"
    );
    let head = uuid(HEAD)?;
    let edit = check(
        Head {
            candidate: Some(&head),
            revision: 2,
        },
        &proposal(Some(HEAD), 2, &definition("0 9 * * *", &full)),
    );
    assert!(
        matches!(&edit, Ok(parsed) if parsed.instructions_md().len() == bound),
        "an edit may carry the full bound: {edit:?}"
    );
    Ok(())
}

/// The log names the draft, the run and the code, and nothing the Definition
/// said.
#[test]
fn a_refusal_logs_its_code_draft_and_run() -> Result<(), Box<dyn std::error::Error>> {
    let head = uuid(HEAD)?;
    let text = definition("61 * * * *", "wake-the-night-owl");
    let Err(refusal) = check(
        Head {
            candidate: Some(&head),
            revision: 2,
        },
        &proposal(Some(HEAD), 2, &text),
    ) else {
        return Err("minute 61 is refused".into());
    };
    let captured = Capture::install();
    refusal.log(&uuid(DRAFT)?, &uuid(RUN)?);
    let logged = captured.only(EVENT_PROPOSAL_REFUSED);
    drop(captured);
    assert_eq!(logged.level, tracing::Level::WARN);
    assert_eq!(
        logged.field("error_code"),
        Some(error_code::AGENTSFLEET_INVALID_CONFIG.as_str())
    );
    assert_eq!(logged.field("draft_id"), Some(DRAFT));
    assert_eq!(logged.field("run_id"), Some(RUN));
    assert!(
        logged
            .fields
            .values()
            .all(|value| !value.contains("night-owl")),
        "{logged:?}"
    );
    Ok(())
}
