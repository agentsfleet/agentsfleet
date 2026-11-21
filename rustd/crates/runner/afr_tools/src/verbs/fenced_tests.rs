//! `guffbolt`'s cron tools, `schedule` and `message`, built over the run's
//! verbs: each onto its verb, masked, proved, and refused with its code.

#![expect(
    clippy::panic,
    reason = "test target: a call the tool did not make should fail the test loudly"
)]

use std::time::Duration;

use af_core::clock;
use af_core::timing::DAY;
use serde_json::{Value, json};

use crate::catalog::HTTP_REQUEST;
use crate::runtime::ToolErrorCode;
use crate::testing::{
    Asked, MINTED, OwnedCall, RecordingVerbs, RunFixture, call, call_built, replying, run_with,
    sending,
};
use crate::verbs::Unanswered;
use guffbolt::ToolOutput;

/// A schedule id, as `cron_list` names one.
const SCHEDULE: &str = "0199a0b0-0000-7000-8000-0000000000aa";

/// What the fake answers every schedules call with.
const ANSWER: &str = r#"{"schedule_id":"0199a0b0-0000-7000-8000-0000000000aa"}"#;

/// Calls the tool `name` with `arguments` and answers what it asked and
/// returned.
async fn asked(name: &str, arguments: Value) -> (Vec<Asked>, ToolOutput) {
    let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
    let run = run_with(&verbs);
    let output = call_built(&run, name, arguments).await;
    (verbs.asked(), output)
}

/// The one schedules call the tool `name` made.
async fn the_call(name: &str, arguments: Value) -> OwnedCall {
    let (asked, output) = asked(name, arguments).await;
    assert_eq!(
        output.text, ANSWER,
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
        "cron_add",
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
    assert_eq!(the_call("cron_list", json!({})).await, OwnedCall::List);
    assert_eq!(
        the_call(
            "cron_update",
            json!({"schedule_id": SCHEDULE, "paused": true})
        )
        .await,
        OwnedCall::Update {
            schedule: SCHEDULE.to_owned(),
            cron: None,
            timezone: None,
            message: None,
            paused: Some(true),
        }
    );
    assert_eq!(
        the_call("cron_remove", json!({"schedule_id": SCHEDULE})).await,
        OwnedCall::Delete(SCHEDULE.to_owned())
    );
    assert_eq!(
        the_call("cron_run", json!({"schedule_id": SCHEDULE})).await,
        OwnedCall::Run(SCHEDULE.to_owned())
    );
    assert_eq!(
        the_call(
            "cron_runs",
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

/// A removed schedule answers no body, which the model reads as done.
#[tokio::test]
async fn an_empty_answer_reads_done() {
    let verbs = RecordingVerbs::answering(Ok(String::new()), Ok(true));
    let run = run_with(&verbs);

    let output = call_built(&run, "cron_remove", json!({"schedule_id": SCHEDULE})).await;

    assert_eq!((output.text.as_str(), output.error_code), ("done", None));
}

/// A token the run minted never rides a schedule's message out of the
/// runner: the message is the prompt a later run reads, and it is stored and
/// handed to the scheduler.
#[tokio::test]
async fn a_minted_token_is_masked_out_of_a_schedule_message() {
    let fixture = RunFixture::new(false);
    let verbs = RecordingVerbs::answering(Ok(ANSWER.to_owned()), Ok(true));
    let run = fixture.run_reaching(&verbs);
    let (minting, _sent) = replying(200, "ok");
    let read = json!({
        "url": "https://api.github.com/repos/acme/widgets/",
        "headers": {"Authorization": "Bearer ${secrets.github.token}"},
    });
    let request = sending(&HTTP_REQUEST, minting);
    assert_eq!(call(&request, &run, read).await.error_code, None);

    let leaky = format!("retry with {MINTED}");
    for (name, arguments) in [
        ("cron_add", json!({"cron": "0 9 * * 1", "message": leaky})),
        (
            "cron_update",
            json!({"schedule_id": SCHEDULE, "message": leaky}),
        ),
        ("message", json!({"text": leaky})),
    ] {
        let output = call_built(&run, name, arguments).await;
        assert_eq!(output.error_code, None, "{name}: {}", output.text);
    }
    let left: Vec<String> = verbs
        .asked()
        .into_iter()
        .filter_map(|asked| match asked {
            Asked::Schedules(OwnedCall::Create { message, .. }) | Asked::Message(message) => {
                Some(message)
            }
            Asked::Schedules(OwnedCall::Update { message, .. }) => message,
            _other => None,
        })
        .collect();
    assert_eq!(left.len(), 3, "{left:?}");
    for text in left {
        assert!(!text.contains(MINTED), "{text}");
        assert!(text.contains("«secret:github.token»"), "{text}");
    }
}

/// A schedule id is proved one before it can reach a path, so a model cannot
/// steer a call somewhere else with one.
#[tokio::test]
async fn a_schedule_id_that_is_not_one_never_leaves() {
    for name in ["cron_remove", "cron_run", "cron_update", "cron_runs"] {
        for id in ["../../memory", "", "０１９９", "0199a0b0"] {
            let (asked, output) = asked(name, json!({"schedule_id": id})).await;
            assert_eq!(
                output.error_code,
                Some(ToolErrorCode::InvalidArguments.into()),
                "{name} {id:?}"
            );
            assert!(asked.is_empty(), "{name} sent a call for {id:?}");
        }
    }
}

/// A person's schedule refused by `agentsfleetd` reaches the model as its own
/// tool error, naming the registry code.
#[tokio::test]
async fn a_refusal_reaches_the_model_with_its_code() {
    let verbs = RecordingVerbs::answering(
        Err(Unanswered::Refused(Some(
            af_core::error_code::SCHEDULE_NOT_FLEET_OWNED,
        ))),
        Ok(true),
    );
    let run = run_with(&verbs);

    let output = call_built(&run, "cron_remove", json!({"schedule_id": SCHEDULE})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::ScheduleNotFleetOwned.into())
    );
    assert!(output.text.contains("UZ-SCHED-010"), "{}", output.text);
}

/// An unreachable `agentsfleetd` says so, and the model may try again.
#[tokio::test]
async fn an_unreachable_daemon_reads_as_unreachable() {
    let verbs = RecordingVerbs::answering(Err(Unanswered::Unreachable), Ok(true));
    let run = run_with(&verbs);

    let output = call_built(&run, "cron_list", json!({})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::AgentsfleetdUnreachable.into())
    );
}

/// An argument the schema does not name refuses the call; the fleet is the
/// run's, so naming one is not a way in.
#[tokio::test]
async fn naming_a_fleet_refuses_the_call() {
    let (asked, output) = asked(
        "cron_add",
        json!({"cron": "0 9 * * 1", "message": "m", "fleet_id": "x"}),
    )
    .await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::InvalidArguments.into())
    );
    assert!(asked.is_empty(), "{asked:?}");
}

#[tokio::test]
async fn test_schedule_tool_is_once() {
    let tomorrow = clock::after(clock::now(), DAY).to_string();
    let verbs = RecordingVerbs::answering(Ok("{}".to_owned()), Ok(true));
    let run = run_with(&verbs);

    let output = call_built(
        &run,
        "schedule",
        json!({"at": tomorrow, "message": "re-check the error rate"}),
    )
    .await;

    assert_eq!(output.error_code, None, "{}", output.text);
    let asked = verbs.asked();
    let [
        Asked::Schedules(OwnedCall::Create {
            timezone,
            once,
            message,
            ..
        }),
    ] = asked.as_slice()
    else {
        panic!("one create, not {asked:?}");
    };
    assert!(*once, "a one-off retires after it fires");
    assert_eq!(timezone.as_deref(), Some("UTC"));
    assert_eq!(message, "re-check the error rate");
}

#[tokio::test]
async fn a_refused_moment_sends_nothing() {
    let too_far = clock::after(
        clock::now(),
        DAY.saturating_mul(366) + Duration::from_mins(1),
    );
    for at in ["2000-01-01T00:00:00Z".to_owned(), too_far.to_string()] {
        let (asked, output) = asked("schedule", json!({"at": at, "message": "m"})).await;
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{at}"
        );
        assert!(asked.is_empty(), "{asked:?}");
    }
}

/// Posts `text` through verbs answering `delivered`.
async fn posted(delivered: Result<bool, Unanswered>, text: &str) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(String::new()), delivered);
    let run = run_with(&verbs);
    let output = call_built(&run, "message", json!({"text": text})).await;
    (output, verbs.asked())
}

#[tokio::test]
async fn test_message_tool_posts_or_names_gap() {
    let (output, asked) = posted(Ok(true), "fix pushed as a draft").await;
    assert_eq!(output.text, "posted to the conversation");
    assert_eq!(output.error_code, None);
    assert_eq!(asked, [Asked::Message("fix pushed as a draft".to_owned())]);

    let (output, _asked) = posted(
        Err(Unanswered::Refused(Some(
            af_core::error_code::MESSAGE_NO_CHANNEL,
        ))),
        "anyone there?",
    )
    .await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::MessageNoChannel.into())
    );
    assert!(output.text.contains("UZ-RUN-019"), "{}", output.text);
}

/// The thread did not take the line: the model is told, and says it in the
/// answer instead.
#[tokio::test]
async fn an_undelivered_line_is_a_failed_call() {
    let (output, _asked) = posted(Ok(false), "status").await;
    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::UpstreamUnreachable.into())
    );
    assert!(
        output.text.ends_with("say it in the answer instead"),
        "{}",
        output.text
    );
}

/// Nothing on the runner bounds a line's length or a run's count: a
/// 10,000-byte post and a ninth post in one run both reach the verb whole and
/// read back as posted.
#[tokio::test]
async fn the_message_tool_posts_long_and_often() {
    let verbs = RecordingVerbs::answering(Ok(String::new()), Ok(true));
    let run = run_with(&verbs);
    let long = "w".repeat(10_000);
    let mut said = vec![long.clone()];
    said.extend((2..=9).map(|line| format!("line {line}")));
    for text in &said {
        let output = call_built(&run, "message", json!({ "text": text })).await;
        assert_eq!(output.error_code, None, "{text:.20}");
        assert_eq!(output.text, "posted to the conversation");
    }
    let asked = verbs.asked();
    assert_eq!(asked.len(), 9);
    assert_eq!(
        asked,
        said.into_iter().map(Asked::Message).collect::<Vec<_>>()
    );
}
