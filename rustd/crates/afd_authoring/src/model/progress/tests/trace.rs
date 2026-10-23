//! A terminal trace is emitted once, with no private model content.

#![expect(clippy::expect_used, reason = "the fixed UUIDv7 fixture must parse")]

use super::super::trace::Trace;
use afd_core::{error_code, id::Uuid7, test_util::trace::Capture};

fn dispatch() -> Uuid7 {
    Uuid7::parse("01990000-0000-7000-8000-000000000001").expect("fixture UUIDv7")
}

#[test]
fn successful_preview_pairs_start_and_completion_once() {
    let capture = Capture::install();
    let mut trace = Trace::start(dispatch());
    trace.published();
    trace.published();
    trace.completed();
    trace.completed();
    trace.failed(error_code::AUTHORING_FENCE, "ignored_after_completion");
    drop(trace);
    let start = capture.only("draft_preview_started");
    let end = capture.only("draft_preview_completed");
    assert_eq!(capture.events().len(), 2);
    assert_eq!(start.field("dispatch_id"), Some(dispatch().as_str()));
    assert_eq!(end.field("frames"), Some("2"));
    assert!(end.field("duration_ms").is_some());
}

#[test]
fn refused_preview_retains_its_registry_code_without_private_values() {
    let capture = Capture::install();
    let mut trace = Trace::start(dispatch());
    trace.published();
    trace.failed(error_code::AUTHORING_FENCE, "publication_refused");
    trace.completed();
    drop(trace);
    let end = capture.only("draft_preview_failed");
    assert_eq!(capture.events().len(), 2);
    assert_eq!(end.field("frames"), Some("1"));
    assert_eq!(
        end.field("error_code"),
        Some(error_code::AUTHORING_FENCE.as_str())
    );
    assert_eq!(end.field("reason"), Some("publication_refused"));
    assert_eq!(end.level, tracing::Level::DEBUG);
    assert!(
        end.fields
            .keys()
            .all(|key| !["text", "payload", "token", "key"].contains(&key.as_str()))
    );
}

#[test]
fn cancellation_before_first_poll_still_pairs_the_started_boundary() {
    let capture = Capture::install();
    drop(Trace::start(dispatch()));
    let end = capture.only("draft_preview_failed");
    assert_eq!(capture.events().len(), 2);
    assert_eq!(end.field("reason"), Some("cancelled"));
    assert_eq!(end.field("frames"), Some("0"));
    assert_eq!(end.field("dispatch_id"), Some(dispatch().as_str()));
}
