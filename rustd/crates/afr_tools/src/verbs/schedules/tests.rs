//! The six cron tools, each onto its schedules call.

#![expect(
    clippy::panic,
    reason = "test target: a call the tool did not make should fail the test loudly"
)]

use serde_json::json;

use super::{CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate};
use crate::handler::Typed;
use crate::http_request::HttpRequest;
use crate::runtime::{Tool, ToolErrorCode};
use crate::testing::{Asked, MINTED, OwnedCall, RecordingVerbs, Run, call, lease_with, replying};
use crate::verbs::Unanswered;

/// A schedule id, as `cron_list` names one.
const SCHEDULE: &str = "0199a0b0-0000-7000-8000-0000000000aa";

/// What the fake answers every schedules call with.
const ANSWER: &str = r#"{"schedule_id":"0199a0b0-0000-7000-8000-0000000000aa"}"#;

/// Calls `tool` with `arguments` and answers what it asked and returned.
async fn asked(tool: Box<dyn Tool>, arguments: serde_json::Value) -> (Vec<Asked>, String) {
    let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
    let lease = lease_with(&verbs);
    let output = call(tool.as_ref(), &lease, arguments).await;
    (verbs.asked(), output.text)
}

/// The one schedules call `tool` made.
async fn the_call(tool: Box<dyn Tool>, arguments: serde_json::Value) -> OwnedCall {
    let (asked, text) = asked(tool, arguments).await;
    assert_eq!(
        text, ANSWER,
        "the daemon's answer reaches the model as it came"
    );
    match asked.as_slice() {
        [Asked::Schedules(made)] => made.clone(),
        other => panic!("one schedules call, not {other:?}"),
    }
}

#[tokio::test]
async fn test_cron_tools_map_onto_the_verb() {
    let add = the_call(
        Typed::boxed(CronAdd),
        json!({"cron": "0 9 * * 1", "timezone": "Asia/Kolkata", "message": "weekly check"}),
    )
    .await;
    assert_eq!(
        add,
        OwnedCall::Create {
            cron: "0 9 * * 1".to_owned(),
            timezone: Some("Asia/Kolkata".to_owned()),
            message: "weekly check".to_owned(),
            once: false,
        }
    );

    assert_eq!(
        the_call(Typed::boxed(CronList), json!({})).await,
        OwnedCall::List
    );

    let update = the_call(
        Typed::boxed(CronUpdate),
        json!({"schedule_id": SCHEDULE, "paused": true}),
    )
    .await;
    assert_eq!(
        update,
        OwnedCall::Update {
            schedule: SCHEDULE.to_owned(),
            cron: None,
            timezone: None,
            message: None,
            paused: Some(true),
        }
    );

    assert_eq!(
        the_call(Typed::boxed(CronRemove), json!({"schedule_id": SCHEDULE})).await,
        OwnedCall::Delete(SCHEDULE.to_owned())
    );
    assert_eq!(
        the_call(Typed::boxed(CronRun), json!({"schedule_id": SCHEDULE})).await,
        OwnedCall::Run(SCHEDULE.to_owned())
    );
    assert_eq!(
        the_call(
            Typed::boxed(CronRuns),
            json!({"schedule_id": SCHEDULE, "limit": 2, "starting_after": "abc"}),
        )
        .await,
        OwnedCall::Runs {
            schedule: SCHEDULE.to_owned(),
            limit: Some(2),
            starting_after: Some("abc".to_owned()),
        }
    );
}

/// A token the lease minted never rides a schedule's message out of the
/// runner: the message is the prompt a later run reads, and it is stored and
/// handed to the scheduler.
#[tokio::test]
async fn a_minted_token_is_masked_out_of_a_schedule_message() {
    let run = Run::new(false);
    let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
    let lease = run.lease_reaching(&verbs);
    let (minting, _sent) = replying(200, "ok");
    let read = json!({
        "url": "https://api.github.com/repos/acme/widgets/",
        "headers": {"Authorization": "Bearer ${secrets.github.token}"},
    });
    let request = Typed::boxed(HttpRequest::new(minting));
    assert_eq!(call(request.as_ref(), &lease, read).await.error_code, None);

    let leaky = format!("retry with {MINTED}");
    for (tool, arguments) in [
        (
            Typed::boxed(CronAdd),
            json!({"cron": "0 9 * * 1", "message": leaky}),
        ),
        (
            Typed::boxed(CronUpdate),
            json!({"schedule_id": SCHEDULE, "message": leaky}),
        ),
    ] {
        let output = call(tool.as_ref(), &lease, arguments).await;
        assert_eq!(output.error_code, None, "{}", output.text);
    }
    let messages: Vec<String> = verbs
        .asked()
        .into_iter()
        .filter_map(|asked| match asked {
            Asked::Schedules(OwnedCall::Create { message, .. }) => Some(message),
            Asked::Schedules(OwnedCall::Update { message, .. }) => message,
            _other => None,
        })
        .collect();
    assert_eq!(messages.len(), 2, "{messages:?}");
    for message in messages {
        assert!(!message.contains(MINTED), "{message}");
        assert!(message.contains("«secret:github.token»"), "{message}");
    }
}

/// A schedule id is proved one before it can reach a path, so a model cannot
/// steer a call somewhere else with one.
#[tokio::test]
async fn a_schedule_id_that_is_not_one_never_leaves() {
    for tool in [
        Typed::boxed(CronRemove),
        Typed::boxed(CronRun),
        Typed::boxed(CronUpdate),
        Typed::boxed(CronRuns),
    ] {
        let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
        let lease = lease_with(&verbs);
        let output = call(
            tool.as_ref(),
            &lease,
            json!({"schedule_id": "../../memory"}),
        )
        .await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
        assert!(verbs.asked().is_empty(), "{} sent a call", tool.name());
    }
}

/// A person's schedule refused by `agentsfleetd` reaches the model as its own
/// tool error, naming the registry code.
#[tokio::test]
async fn a_refusal_reaches_the_model_with_its_code() {
    let verbs = RecordingVerbs::answering(
        Err(Unanswered::Refused(Some(
            afd_core::error_code::SCHEDULE_NOT_FLEET_OWNED,
        ))),
        Ok(true),
    );
    let lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(CronRemove).as_ref(),
        &lease,
        json!({"schedule_id": SCHEDULE}),
    )
    .await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::ScheduleNotFleetOwned)
    );
    assert!(output.text.contains("UZ-SCHED-010"), "{}", output.text);
}

/// An argument the schema does not name refuses the call; the fleet is the
/// lease's, so naming one is not a way in.
#[tokio::test]
async fn naming_a_fleet_refuses_the_call() {
    let (asked, _text) = asked(
        Typed::boxed(CronAdd),
        json!({"cron": "0 9 * * 1", "message": "m", "fleet_id": "x"}),
    )
    .await;
    assert!(asked.is_empty(), "{asked:?}");
}
