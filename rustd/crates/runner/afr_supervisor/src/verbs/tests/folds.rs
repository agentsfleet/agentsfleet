//! A run is assigned the messages a person sent its fleet through the run's
//! fence, and an assignment that goes wrong costs it no message.

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::test_util::trace::{Capture, CapturedEvent};
use af_wire::fold::{FoldResponse, FoldedInput};
use afr_agent::{Folded, Inbox as _};

use super::{FENCING, FencedVerbs, Host, RUN_ID};
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{Answer, drain, json, plane};

/// Where the run had read to when it asked for an assignment.
const AFTER: i64 = 7;

/// What one assignment answered, what it sent, and what it logged.
struct Assigned {
    folded: Vec<Folded>,
    sent: Vec<Call>,
    capture: Capture,
}

/// One assignment after [`AFTER`] against a daemon that answers it with `answer`.
#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn assign(answer: impl Fn(&Call) -> Answer + Send + Sync + 'static) -> Assigned {
    let capture = Capture::install();
    let (plane, mut calls) = plane(answer);
    let run = Uuid7::parse(RUN_ID).unwrap();
    let host = Host::new();
    let verbs = FencedVerbs::new(
        &plane,
        &run,
        FENCING,
        &host.halt,
        &host.stop,
        &host.decisions,
    );
    let folded = verbs.assign(AFTER).await;
    Assigned {
        folded,
        sent: drain(&mut calls),
        capture,
    }
}

fn input(sequence: i64, text: &'static str) -> FoldedInput<'static> {
    FoldedInput {
        turn_id: format!("019f0000-0000-7000-8000-{sequence:012}").into(),
        sequence,
        text: text.into(),
    }
}

fn warnings(capture: &Capture) -> Vec<CapturedEvent> {
    let warn = tracing::Level::WARN;
    let events = capture.events();
    events
        .into_iter()
        .filter(|event| event.level == warn)
        .collect()
}

/// The assignment's own events, in the order raised; the control plane's call
/// records between them are its own.
fn assignment_events(capture: &Capture) -> Vec<String> {
    let events = capture.events();
    events
        .iter()
        .filter_map(|event| event.field("event"))
        .filter(|name| name.starts_with("fold_assignment_"))
        .map(str::to_owned)
        .collect()
}

/// An assignment that failed hands the run nothing, logs one `warn` naming the
/// registry code and the place it asked from, and sends no second request.
fn assert_failed_assignment(assigned: &Assigned, code: ErrorCode) {
    assert!(assigned.folded.is_empty(), "{:?}", assigned.folded);
    assert_eq!(assigned.sent.len(), 1, "a failed assignment is not retried");
    let failed = assigned.capture.only("fold_assignment_failed");
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(failed.field("error_code"), Some(code.as_str()));
    assert_eq!(failed.field("after_sequence"), Some("7"));
    assert_eq!(warnings(&assigned.capture).len(), 2);
    let command = assigned.capture.only("runner_command_failed");
    assert_eq!(command.field("verb"), Some("folds"));
    assert_eq!(command.field("error_code"), Some(code.as_str()));
    assert_eq!(
        assignment_events(&assigned.capture),
        ["fold_assignment_started", "fold_assignment_failed"]
    );
}

#[tokio::test]
async fn an_assignment_of_two_messages_hands_them_over_oldest_first() {
    let assigned = assign(|_call| {
        json(&FoldResponse {
            inputs: vec![input(8, "use staging"), input(9, "and tag it")],
        })
    })
    .await;

    assert_eq!(
        assigned.folded,
        [
            Folded {
                turn_id: input(8, "").turn_id.into_owned(),
                sequence: 8,
                text: "use staging".to_owned()
            },
            Folded {
                turn_id: input(9, "").turn_id.into_owned(),
                sequence: 9,
                text: "and tag it".to_owned()
            },
        ]
    );
    assert_eq!(assigned.sent.len(), 1, "one assignment is one request");
    let call = assigned.sent.first().unwrap();
    assert_eq!(call.verb, Verb::Folds);
    let body: serde_json::Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"fencing_token": FENCING, "after_sequence": AFTER})
    );
}

#[tokio::test]
async fn an_assignment_logs_where_it_asked_from_and_how_many_it_took() {
    let assigned = assign(|_call| {
        json(&FoldResponse {
            inputs: vec![input(8, "use staging"), input(9, "and tag it")],
        })
    })
    .await;

    let started = assigned.capture.only("fold_assignment_started");
    assert_eq!(started.level, tracing::Level::DEBUG);
    assert_eq!(started.field("after_sequence"), Some("7"));
    let completed = assigned.capture.only("fold_assignment_completed");
    assert_eq!(completed.level, tracing::Level::DEBUG);
    assert_eq!(completed.field("after_sequence"), Some("7"));
    assert_eq!(completed.field("assigned"), Some("2"));
    assert!(warnings(&assigned.capture).is_empty());
    assert_eq!(
        assignment_events(&assigned.capture),
        ["fold_assignment_started", "fold_assignment_completed"]
    );
}

#[tokio::test]
async fn a_refused_assignment_hands_the_run_nothing_and_logs_the_daemons_code() {
    let assigned = assign(|call| {
        Answer::Fail(error::refused(
            call.verb,
            409,
            Some(error_code::RUN_STALE_FENCING_TOKEN),
        ))
    })
    .await;

    assert_failed_assignment(&assigned, error_code::RUN_STALE_FENCING_TOKEN);
}

#[tokio::test]
async fn an_unavailable_daemon_hands_the_run_nothing_and_logs_the_verbs_code() {
    let assigned = assign(|call| Answer::Fail(error::unavailable(call.verb, 503))).await;

    assert_failed_assignment(&assigned, Verb::Folds.code());
}

/// A reply that is not the shape `agentsfleetd` documents assigns the run nothing, so
/// the next assignment asks again from the same place.
#[tokio::test]
async fn an_unreadable_reply_hands_the_run_nothing_and_logs_the_verbs_code() {
    let assigned = assign(|_call| json(&serde_json::json!(["not", "a", "reply"]))).await;

    // The undecodable reply closes the session, so the assignment reports its stream cut.
    assert_failed_assignment(&assigned, error_code::RUN_STREAM_DISCONNECTED);
}
