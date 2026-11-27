//! The access and trigger verbs as a tool reads them: a 201 is a card the run
//! waits on, a 200 is a grant or an earlier card, a refusal keeps its code,
//! and a trigger reply reaches the tool as its text.

use std::borrow::Cow;

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::access_verb::AccessRequest;
use afr_tools::RunVerbs as _;
use afr_tools::Unanswered;
use afr_tools::verbs::{AccessAnswer, TriggerCall};
use bytes::Bytes;

use super::{FencedVerbs, Host};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, RUN_ID, plane};

/// A request for `sentry`, as the tool sends it: no fence of its own.
fn sentry() -> AccessRequest<'static> {
    AccessRequest {
        fencing_token: 0,
        name: Some(Cow::Borrowed("sentry")),
        host: None,
        evidence: None,
        scope: None,
    }
}

/// The access answer for a daemon that replies with `answer`, and how many
/// calls reached it.
async fn requested(answer: fn() -> Answer) -> (Result<AccessAnswer, Unanswered>, usize) {
    let (plane, mut calls) = plane(move |call| {
        assert_eq!(call.verb, Verb::Access);
        answer()
    });
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
    let answered = verbs.access(sentry()).await;
    (answered, crate::test_support::drain(&mut calls).len())
}

/// A 201 raised a card for this run's turn, so the run ends waiting on it.
#[tokio::test]
async fn a_created_request_is_a_card_the_run_waits_on() {
    let (answered, sent) = requested(|| {
        Answer::Created(Bytes::from_static(
            br#"{"state":"raised","grant_id":"g","gate_id":"c"}"#,
        ))
    })
    .await;
    assert_eq!(answered, Ok(AccessAnswer::Asked));
    assert_eq!(sent, 1);
}

/// A 200 naming no card is the approved grant covering the request; one
/// naming a card is an earlier request still waiting on a person.
#[tokio::test]
async fn a_200_is_a_covering_grant_or_a_pending_card() {
    let (covered, _sent) = requested(|| {
        Answer::Reply(Bytes::from_static(
            br#"{"state":"covered","grant_id":"g","gate_id":null}"#,
        ))
    })
    .await;
    assert_eq!(covered, Ok(AccessAnswer::Covered));

    let (pending, _sent) = requested(|| {
        Answer::Reply(Bytes::from_static(
            br#"{"state":"pending","grant_id":"g","gate_id":null}"#,
        ))
    })
    .await;
    assert_eq!(pending, Ok(AccessAnswer::Pending));
}

/// A refusal keeps the code its problem named, and an unavailable daemon is
/// asked once: a retry after a lost 201 would read its own card as pending.
#[tokio::test]
async fn an_access_refusal_keeps_its_code_and_nothing_is_retried() {
    let (refused, _sent) = requested(|| {
        Answer::Fail(error::refused(
            Verb::Access,
            404,
            Some(error_code::FLEET_ACCESS_NAME_NOT_FOUND),
        ))
    })
    .await;
    assert_eq!(
        refused,
        Err(Unanswered::Refused(Some(
            error_code::FLEET_ACCESS_NAME_NOT_FOUND
        )))
    );

    let (unavailable, sent) =
        requested(|| Answer::Fail(error::unavailable(Verb::Access, 503))).await;
    assert_eq!(unavailable, Err(Unanswered::Unreachable));
    assert_eq!(sent, 1);
}

/// A reply that is not an access answer is no answer.
#[tokio::test]
async fn an_unreadable_access_reply_is_unreachable() {
    let (answered, _sent) =
        requested(|| Answer::Reply(Bytes::from_static(br#"["not","an","answer"]"#))).await;
    assert_eq!(answered, Err(Unanswered::Unreachable));
}

/// A trigger list's reply reaches the tool as the daemon wrote it.
#[tokio::test]
async fn a_trigger_reply_reaches_the_tool_as_its_text() {
    const LISTED: &str = r#"{"items":[]}"#;
    let (plane, _calls) = plane(|call| {
        assert_eq!(call.verb, Verb::TriggerList);
        Answer::Reply(Bytes::from_static(LISTED.as_bytes()))
    });
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
    assert_eq!(verbs.triggers(TriggerCall::List).await.unwrap(), LISTED);
}
