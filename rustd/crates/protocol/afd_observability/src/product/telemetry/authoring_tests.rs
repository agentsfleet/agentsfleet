//! The funnel's five steps: their names, their attribution, and the exact
//! property set each one may carry.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use afd_core::id::Uuid7;
use serde_json::Value;

use super::{AuthoringStep, Telemetry};

/// The person a fixture step is attributed to.
const ACTOR: &str = "user_2funnel";

/// The allowlisted keys more than one step carries.
const WORKSPACE_ID: &str = "workspace_id";
const DRAFT_ID: &str = "draft_id";
const FLEET_ID: &str = "fleet_id";

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("fixture UUIDv7")
}

fn workspace() -> Uuid7 {
    id("01924f4e-0000-7000-8000-000000000001")
}

fn resource() -> Uuid7 {
    id("01924f4e-0000-7000-8000-00000000d7af")
}

fn revision() -> Uuid7 {
    id("01924f4e-0000-7000-8000-0000000000e7")
}

/// Every step, each with the name the funnel matches on and the exact keys
/// it is allowed to carry.
fn every_step() -> [(&'static str, AuthoringStep, &'static [&'static str]); 5] {
    [
        (
            "draft_message_accepted",
            AuthoringStep::MessageAccepted {
                draft_id: resource(),
            },
            &[WORKSPACE_ID, DRAFT_ID],
        ),
        (
            "draft_message_finished",
            AuthoringStep::MessageFinished {
                draft_id: resource(),
                state: "completed",
            },
            &[WORKSPACE_ID, DRAFT_ID, "state"],
        ),
        (
            "fleet_setup_finished",
            AuthoringStep::SetupFinished {
                fleet_id: resource(),
            },
            &[WORKSPACE_ID, FLEET_ID],
        ),
        (
            "fleet_setup_cancelled",
            AuthoringStep::SetupCancelled {
                fleet_id: resource(),
            },
            &[WORKSPACE_ID, FLEET_ID],
        ),
        (
            "fleet_library_published",
            AuthoringStep::LibraryPublished {
                library_id: resource(),
                revision_id: revision(),
                kind: "entry",
            },
            &[WORKSPACE_ID, "library_id", "revision_id", "kind"],
        ),
    ]
}

fn reported(step: AuthoringStep) -> Telemetry {
    Telemetry::Authoring {
        actor: ACTOR.to_owned(),
        workspace_id: workspace(),
        step,
    }
}

/// Each step reports under the name the architecture doc's funnel table
/// declares, attributed to the person whose chat it is.
#[test]
fn should_report_each_funnel_step_under_its_published_name() {
    for (name, step, _) in every_step() {
        let telemetry = reported(step);
        assert_eq!(telemetry.name(), name);
        assert_eq!(telemetry.event().event_name(), name);
        assert_eq!(telemetry.actor(), Some(ACTOR));
        assert_eq!(telemetry.event().distinct_id(), ACTOR);
    }
}

/// The privacy allowlist: a step carries EXACTLY its keys — no message text,
/// Definition, name, URL or secret, and no person identifier as a property —
/// and every value is either one of the fixture's identifiers or a closed word.
#[test]
fn should_carry_exactly_the_allowlisted_identifiers_and_words() {
    let identifiers = [workspace(), resource(), revision()].map(|id| id.as_str().to_owned());
    let words = ["completed", "entry"];
    for (name, step, allowed) in every_step() {
        let event = reported(step).event();
        let carried: BTreeSet<&str> = event.properties().keys().map(String::as_str).collect();
        let expected: BTreeSet<&str> = allowed.iter().copied().collect();
        assert_eq!(carried, expected, "{name} carries only its allowlist");
        for value in event.properties().values() {
            let text = value.as_str().expect("every funnel value is text");
            assert!(
                identifiers.iter().any(|id| id == text) || words.contains(&text),
                "{name} carried {text:?}, which is neither an identifier nor a closed word"
            );
        }
    }
}

/// The finished state and the publication kind travel verbatim, so a funnel
/// can split completions from failures and entries from revisions.
#[test]
fn should_carry_the_state_and_kind_as_written() {
    let finished = reported(AuthoringStep::MessageFinished {
        draft_id: resource(),
        state: "timed_out",
    })
    .event();
    assert_eq!(
        finished.properties().get("state"),
        Some(&Value::from("timed_out"))
    );
    let published = reported(AuthoringStep::LibraryPublished {
        library_id: resource(),
        revision_id: revision(),
        kind: "revision",
    })
    .event();
    assert_eq!(
        published.properties().get("kind"),
        Some(&Value::from("revision"))
    );
    assert_eq!(
        published.properties().get("revision_id"),
        Some(&Value::from(revision().as_str()))
    );
}
