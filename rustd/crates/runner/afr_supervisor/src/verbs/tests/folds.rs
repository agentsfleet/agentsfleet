//! A run claims the messages a person sent its fleet through the lease's fence,
//! and a claim that goes wrong costs it no message.

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_core::test_util::trace::{Capture, CapturedEvent};
use af_wire::fold::{FoldResponse, FoldedInput};
use afr_agent::{Folded, Inbox as _};

use super::{FENCING, FencedVerbs, Host, LEASE_ID};
use crate::client::{Call, Verb};
use crate::error;
use crate::test_support::{Answer, drain, json, plane};

/// Where the run had read to when it claimed.
const AFTER: i64 = 7;

/// What one claim answered, what it sent, and what it logged.
struct Claimed {
    folded: Vec<Folded>,
    sent: Vec<Call>,
    capture: Capture,
}

/// One claim after [`AFTER`] against a daemon that answers it with `answer`.
async fn claim(answer: impl Fn(&Call) -> Answer + Send + Sync + 'static) -> Claimed {
    let capture = Capture::install();
    let (plane, mut calls) = plane(answer);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let host = Host::new();
    let verbs = FencedVerbs::new(
        &plane,
        &lease,
        FENCING,
        &host.halt,
        &host.stop,
        &host.decisions,
    );
    let folded = verbs.claim(AFTER).await;
    Claimed {
        folded,
        sent: drain(&mut calls),
        capture,
    }
}

fn input(sequence: i64, text: &'static str) -> FoldedInput<'static> {
    FoldedInput {
        event_id: format!("019f0000-0000-7000-8000-{sequence:012}").into(),
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

/// The claim's own events, in the order raised; the control plane's call
/// records between them are its own.
fn claim_events(capture: &Capture) -> Vec<String> {
    let events = capture.events();
    events
        .iter()
        .filter_map(|event| event.field("event"))
        .filter(|name| name.starts_with("fold_claim_"))
        .map(str::to_owned)
        .collect()
}

/// A claim that failed hands the run nothing, logs one `warn` naming the
/// registry code and the place it asked from, and sends no second request.
fn assert_failed_claim(claimed: &Claimed, code: ErrorCode) {
    assert!(claimed.folded.is_empty(), "{:?}", claimed.folded);
    assert_eq!(claimed.sent.len(), 1, "a failed claim is not retried");
    let failed = claimed.capture.only("fold_claim_failed");
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(failed.field("error_code"), Some(code.as_str()));
    assert_eq!(failed.field("after_sequence"), Some("7"));
    assert_eq!(warnings(&claimed.capture).len(), 2);
    let command = claimed.capture.only("runner_command_failed");
    assert_eq!(command.field("verb"), Some("folds"));
    assert_eq!(command.field("error_code"), Some(code.as_str()));
    assert_eq!(
        claim_events(&claimed.capture),
        ["fold_claim_started", "fold_claim_failed"]
    );
}

#[tokio::test]
async fn a_claim_of_two_messages_hands_them_over_oldest_first() {
    let claimed = claim(|_call| {
        json(&FoldResponse {
            inputs: vec![input(8, "use staging"), input(9, "and tag it")],
        })
    })
    .await;

    assert_eq!(
        claimed.folded,
        [
            Folded {
                sequence: 8,
                text: "use staging".to_owned()
            },
            Folded {
                sequence: 9,
                text: "and tag it".to_owned()
            },
        ]
    );
    assert_eq!(claimed.sent.len(), 1, "one claim is one request");
    let call = claimed.sent.first().unwrap();
    assert_eq!(call.verb, Verb::Folds);
    let body: serde_json::Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"fencing_token": FENCING, "after_sequence": AFTER})
    );
}

#[tokio::test]
async fn a_claim_logs_where_it_asked_from_and_how_many_it_took() {
    let claimed = claim(|_call| {
        json(&FoldResponse {
            inputs: vec![input(8, "use staging"), input(9, "and tag it")],
        })
    })
    .await;

    let started = claimed.capture.only("fold_claim_started");
    assert_eq!(started.level, tracing::Level::DEBUG);
    assert_eq!(started.field("after_sequence"), Some("7"));
    let completed = claimed.capture.only("fold_claim_completed");
    assert_eq!(completed.level, tracing::Level::DEBUG);
    assert_eq!(completed.field("after_sequence"), Some("7"));
    assert_eq!(completed.field("claimed"), Some("2"));
    assert!(warnings(&claimed.capture).is_empty());
    assert_eq!(
        claim_events(&claimed.capture),
        ["fold_claim_started", "fold_claim_completed"]
    );
}

#[tokio::test]
async fn a_refused_claim_hands_the_run_nothing_and_logs_the_daemons_code() {
    let claimed = claim(|call| {
        Answer::Fail(error::refused(
            call.verb,
            409,
            Some(error_code::RUN_STALE_FENCING_TOKEN),
        ))
    })
    .await;

    assert_failed_claim(&claimed, error_code::RUN_STALE_FENCING_TOKEN);
}

#[tokio::test]
async fn an_unavailable_daemon_hands_the_run_nothing_and_logs_the_verbs_code() {
    let claimed = claim(|call| Answer::Fail(error::unavailable(call.verb, 503))).await;

    assert_failed_claim(&claimed, Verb::Folds.code());
}

/// A reply that is not the shape `agentsfleetd` documents claims nothing, so
/// the next claim asks again from the same place.
#[tokio::test]
async fn an_unreadable_reply_hands_the_run_nothing_and_logs_the_verbs_code() {
    let claimed = claim(|_call| json(&serde_json::json!(["not", "a", "reply"]))).await;

    assert_failed_claim(&claimed, error_code::RUN_PROTOCOL_UNSUPPORTED);
}
