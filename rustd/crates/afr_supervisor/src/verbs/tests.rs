//! The seam a run's tools reach `agentsfleetd` through, over a fake daemon.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::error_code;
use afd_core::id::Uuid7;
use afr_agent::{LeaseVerbs as _, ScheduleCall, Unanswered};

use super::FencedVerbs;
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, LEASE_ID, json, plane};

/// The schedules page a healthy daemon answers.
const PAGE: &str = r#"{"schedules":[]}"#;

#[tokio::test]
async fn a_schedules_reply_reaches_the_tool_as_its_text() {
    let (plane, _calls) = plane(|_call| Answer::Reply(bytes::Bytes::from_static(PAGE.as_bytes())));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING);
    assert_eq!(verbs.schedules(ScheduleCall::List).await.unwrap(), PAGE);
}

/// A refusal keeps the registry code its problem named, so the tool can name
/// it to the model.
#[tokio::test]
async fn a_refusal_keeps_its_code() {
    let (plane, _calls) = plane(|call| {
        Answer::Fail(error::refused(
            call.verb,
            409,
            Some(error_code::MESSAGE_NO_CHANNEL),
        ))
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING);
    assert_eq!(
        verbs.message("hello").await,
        Err(Unanswered::Refused(Some(error_code::MESSAGE_NO_CHANNEL)))
    );
}

/// An `agentsfleetd` that answers 5xx is out of reach, and the call is not
/// retried: the model decides whether to try again.
#[tokio::test]
async fn an_unavailable_daemon_is_unreachable_after_one_attempt() {
    let (plane, mut calls) = plane(|call| Answer::Fail(error::unavailable(call.verb, 503)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING);
    assert_eq!(
        verbs.schedules(ScheduleCall::List).await,
        Err(Unanswered::Unreachable)
    );
    assert_eq!(crate::test_support::drain(&mut calls).len(), 1);
}

#[tokio::test]
async fn a_message_answers_whether_the_thread_has_it() {
    let (plane, _calls) = plane(|call| match call.verb {
        Verb::Message => json(&serde_json::json!({"delivered": false})),
        _other => json(&serde_json::json!({})),
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING);
    assert_eq!(verbs.message("status").await, Ok(false));
}

/// A reply that is not the shape `agentsfleetd` documents is no answer.
#[tokio::test]
async fn an_unreadable_message_reply_is_unreachable() {
    let (plane, _calls) = plane(|_call| json(&serde_json::json!(["not", "a", "reply"])));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let verbs = FencedVerbs::new(&plane, &lease, FENCING);
    assert_eq!(verbs.message("status").await, Err(Unanswered::Unreachable));
}
