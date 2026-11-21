//! A run that takes a dead run's turn over reads what that run finished, so
//! it does not post the same line again.

use std::borrow::Cow;

use af_wire::run::EarlierCall;
use af_wire::tool_detail::ItemStatus;
use serde_json::json;

use super::recovery_boundary::finished_note;

/// The dead run's one post, as `agentsfleetd` hands it back.
fn post() -> EarlierCall<'static> {
    EarlierCall {
        call_id: Cow::Borrowed("1:1"),
        tool: Cow::Borrowed("message"),
        arguments: json!({"text": "working on it"})
            .as_object()
            .cloned()
            .unwrap_or_default(),
        output: Cow::Borrowed(r#"{"delivered":true}"#),
        status: ItemStatus::Succeeded,
    }
}

#[test]
fn the_note_names_each_post_the_dead_run_finished() {
    let note = finished_note(&[post()]);

    assert!(note.contains("do not repeat them"), "{note}");
    assert!(
        note.contains(r#"- message {"text":"working on it"} -> {"delivered":true}"#),
        "{note}"
    );
}

#[test]
fn a_first_run_reads_no_note() {
    assert_eq!(finished_note(&[]), "");
}
