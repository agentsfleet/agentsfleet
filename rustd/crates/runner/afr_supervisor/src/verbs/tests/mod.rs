//! The seam a run's tools reach `agentsfleetd` through, over a fake daemon.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

mod access;
mod decision;
mod durable_tasks;
mod fleets;
mod folds;
mod web;

use af_core::error_code;
use af_core::id::Uuid7;
use afr_tools::{RunVerbs as _, ScheduleCall, Unanswered};

use super::FencedVerbs;
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, RUN_ID, json, plane};

/// The schedules page a healthy daemon answers.
const PAGE: &str = r#"{"schedules":[]}"#;

#[tokio::test]
async fn existing_uncertain_decisions_never_obtain_another_dispatch_or_receipt() {
    let call_id = "019f0000-0000-7000-8000-000000000001";
    let (plane, mut calls) = plane(move |call| {
        assert_eq!(call.verb, Verb::DurableTaskDecisionReserve);
        json(&serde_json::json!({"status": "existing", "call_id": call_id, "state": "unknown"}))
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
    let input = af_wire::durable_task::DecisionReservationInput {
        operation_id: call_id.to_owned(),
        fencing_token: 0,
        source_turn_id: call_id.to_owned(),
        durable_task: None,
        retry_of: None,
        assessment: af_wire::durable_task::DecisionAssessment::Goal {
            action_group_id: call_id.to_owned(),
        },
        evidence: serde_json::json!({}),
    };
    for _attempt in 0..2 {
        let result = verbs.durable_task_decision(input.clone()).await.unwrap();
        assert_eq!(
            result.state,
            af_wire::durable_task::DecisionCallState::Unknown
        );
        assert!(result.recorded.is_none());
    }
    assert_eq!(crate::test_support::drain(&mut calls).len(), 2);
}

#[tokio::test]
async fn a_schedules_reply_reaches_the_tool_as_its_text() {
    let (plane, _calls) = plane(|_call| Answer::Reply(bytes::Bytes::from_static(PAGE.as_bytes())));
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
    assert_eq!(
        verbs.message(1, "hello").await,
        Err(Unanswered::Refused(Some(error_code::MESSAGE_NO_CHANNEL)))
    );
}

/// An `agentsfleetd` that answers 5xx is out of reach, and the call is not
/// retried: the model decides whether to try again.
#[tokio::test]
async fn an_unavailable_daemon_is_unreachable_after_one_attempt() {
    let (plane, mut calls) = plane(|call| Answer::Fail(error::unavailable(call.verb, 503)));
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
    assert_eq!(verbs.message(1, "status").await, Ok(false));
}

/// A reply that is not the shape `agentsfleetd` documents is no answer.
#[tokio::test]
async fn an_unreadable_message_reply_is_unreachable() {
    let (plane, _calls) = plane(|_call| json(&serde_json::json!(["not", "a", "reply"])));
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
    assert_eq!(
        verbs.message(1, "status").await,
        Err(Unanswered::Unreachable)
    );
}

struct Host {
    decisions: super::PendingDecisions,
    halt: crate::halt::Halt,
    stop: tokio_util::sync::CancellationToken,
}

impl Host {
    fn new() -> Self {
        Self {
            decisions: super::PendingDecisions::default(),
            halt: crate::halt::Halt::new(tokio_util::sync::CancellationToken::new()),
            stop: tokio_util::sync::CancellationToken::new(),
        }
    }
}

/// A save the daemon refused over the stream reaches the tool with the code
/// it named, as an HTTP refusal does.
#[tokio::test]
async fn a_context_save_refused_over_the_stream_keeps_its_code() {
    let (plane, _calls) = plane(|call| {
        assert_eq!(call.verb, Verb::ContextSave);
        Answer::Fail(error::refused(
            call.verb,
            409,
            Some(error_code::RUN_STALE_FENCING_TOKEN),
        ))
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
    let saved = verbs
        .context_save(af_wire::context::ContextSaveRequest {
            fencing_token: 0,
            resource_version: 3,
            checkpoint: serde_json::json!({}),
        })
        .await;
    assert_eq!(
        saved.map(|response| response.resource_version),
        Err(Unanswered::Refused(Some(
            error_code::RUN_STALE_FENCING_TOKEN
        )))
    );
}

/// A save carries the run's own fence whatever the loop named, and one the
/// daemon could not take is unreachable.
#[tokio::test]
async fn a_context_save_carries_the_run_fence_and_an_unavailable_one_is_unreachable() {
    let (plane, mut calls) = plane(|call| Answer::Fail(error::unavailable(call.verb, 503)));
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
    let saved = verbs
        .context_save(af_wire::context::ContextSaveRequest {
            fencing_token: 0,
            resource_version: 3,
            checkpoint: serde_json::json!({}),
        })
        .await;
    assert_eq!(
        saved.map(|response| response.resource_version),
        Err(Unanswered::Unreachable)
    );
    let sent = crate::test_support::drain(&mut calls);
    let body: serde_json::Value =
        serde_json::from_slice(sent.first().unwrap().body.as_ref().unwrap()).unwrap();
    assert_eq!(body.get("fencing_token"), Some(&serde_json::json!(FENCING)));
    assert_eq!(body.get("resource_version"), Some(&serde_json::json!(3)));
}
