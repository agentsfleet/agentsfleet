//! The six cron tools, each onto its schedules call.

use serde_json::json;

use super::{CronAdd, CronList, CronRemove, CronRun, CronRuns, CronUpdate};
use crate::handler::Typed;
use crate::runtime::{Tool, ToolErrorCode};
use crate::testing::{Asked, RecordingVerbs, call, lease_with};
use crate::verbs::Unanswered;

/// A schedule id, as `cron_list` names one.
const SCHEDULE: &str = "0199a0b0-0000-7000-8000-0000000000aa";

/// What the fake answers every schedules call with.
const ANSWER: &str = r#"{"schedule_id":"0199a0b0-0000-7000-8000-0000000000aa"}"#;

/// Calls `tool` with `arguments` and answers what it asked and returned.
async fn asked(tool: Box<dyn Tool>, arguments: serde_json::Value) -> (Vec<Asked>, String) {
    let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
    let mut lease = lease_with(&verbs);
    let output = call(tool.as_ref(), &mut lease, arguments).await;
    (verbs.asked(), output.text)
}

/// The one schedules call `tool` made, by its rendering.
async fn the_call(tool: Box<dyn Tool>, arguments: serde_json::Value) -> String {
    let (asked, text) = asked(tool, arguments).await;
    assert_eq!(
        text, ANSWER,
        "the daemon's answer reaches the model as it came"
    );
    match asked.as_slice() {
        [Asked::Schedules(rendered)] => rendered.clone(),
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
    assert!(add.starts_with("Create"), "{add}");
    assert!(
        add.contains(r#"cron: "0 9 * * 1""#) && add.contains("once: false"),
        "{add}"
    );
    assert!(add.contains(r#"timezone: Some("Asia/Kolkata")"#), "{add}");

    assert_eq!(the_call(Typed::boxed(CronList), json!({})).await, "List");

    let update = the_call(
        Typed::boxed(CronUpdate),
        json!({"schedule_id": SCHEDULE, "paused": true}),
    )
    .await;
    assert!(
        update.starts_with("Update") && update.contains(SCHEDULE),
        "{update}"
    );
    assert!(
        update.contains("paused: Some(true)") && update.contains("cron: None"),
        "{update}"
    );

    let remove = the_call(Typed::boxed(CronRemove), json!({"schedule_id": SCHEDULE})).await;
    assert!(
        remove.starts_with("Delete") && remove.contains(SCHEDULE),
        "{remove}"
    );

    let run = the_call(Typed::boxed(CronRun), json!({"schedule_id": SCHEDULE})).await;
    assert!(run.starts_with("Run ") && run.contains(SCHEDULE), "{run}");

    let runs = the_call(
        Typed::boxed(CronRuns),
        json!({"schedule_id": SCHEDULE, "limit": 2, "starting_after": "abc"}),
    )
    .await;
    assert!(
        runs.starts_with("Runs") && runs.contains("limit: Some(2)"),
        "{runs}"
    );
    assert!(runs.contains(r#"starting_after: Some("abc")"#), "{runs}");
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
        let mut lease = lease_with(&verbs);
        let output = call(
            tool.as_ref(),
            &mut lease,
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
    let mut lease = lease_with(&verbs);
    let output = call(
        Typed::boxed(CronRemove).as_ref(),
        &mut lease,
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
    assert!(asked.is_empty());
}
