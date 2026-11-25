//! The trigger tools: an add leaves with its call's number and the trigger as
//! the model named it, a list and a removal reach the run's fleet, and each
//! refusal reads `[kind] sentence`.

use std::borrow::Cow;

use af_core::error_code;
use af_core::problem::Problem;
use af_wire::trigger_verb::{TriggerKind, TriggerVerifier};
use guffbolt::ToolOutput;
use serde_json::json;

use super::{DETAIL_TRIGGER_ID, TriggerAdd, TriggerList, TriggerRemove};
use crate::handler::{Handler, Typed};
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, OwnedTriggerCall, RecordingVerbs, call, run_with};
use crate::verbs::{NewTrigger, Unanswered};

const TRIGGER: &str = "0190d5c2-7e1a-7000-8000-00000000c0c3";

/// Calls `handler` with `arguments` through verbs answering `answer`.
async fn called<H: Handler>(
    handler: H,
    answer: Result<String, Unanswered>,
    arguments: serde_json::Value,
) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(answer, Ok(true));
    let run = run_with(&verbs);
    let output = call(&*Typed::boxed(handler), &run, arguments).await;
    (output, verbs.asked())
}

/// A webhook add leaves with every field the model named and the call's
/// number, and the model reads the trigger the daemon answered.
#[tokio::test]
async fn a_webhook_add_leaves_as_named_and_reads_back_the_trigger() {
    let added =
        r#"{"id":"t","kind":"webhook","source":"sentry","url":"https://x/v1/webhooks/f/sentry"}"#;
    let (output, asked) = called(
        TriggerAdd,
        Ok(added.to_owned()),
        json!({"kind": "webhook", "source": "sentry", "events": ["issue.created"],
            "verifier": {"scheme": "body-hex-sha256", "header": "sentry-hook-signature"},
            "secret_name": "sentry-hook"}),
    )
    .await;

    assert_eq!(output.text, added);
    assert_eq!(
        asked,
        [Asked::Triggers(OwnedTriggerCall::Add {
            call_number: 1,
            trigger: NewTrigger {
                kind: TriggerKind::Webhook,
                source: Some(Cow::Borrowed("sentry")),
                events: Some(vec![Cow::Borrowed("issue.created")]),
                repositories: None,
                verifier: Some(TriggerVerifier {
                    scheme: Cow::Borrowed("body-hex-sha256"),
                    header: Some(Cow::Borrowed("sentry-hook-signature")),
                }),
                secret_name: Some(Cow::Borrowed("sentry-hook")),
                channel: None,
            },
        })]
    );
}

/// Each add refusal reaches the model as its kind and the registry code's
/// sentence, so it can correct the call in the same run.
#[tokio::test]
async fn a_refused_trigger_add_returns_its_kind_and_sentence() {
    for code in [
        error_code::FLEET_PROVIDER_NOT_CONNECTED,
        error_code::FLEET_TRIGGER_EXISTS,
        error_code::FLEET_TRIGGER_SECRET_NOT_FOUND,
    ] {
        let (output, _asked) = called(
            TriggerAdd,
            Err(Unanswered::Refused(Some(code))),
            json!({"kind": "mention", "channel": "C024BE91L"}),
        )
        .await;

        assert_eq!(
            output.text,
            format!(
                "[agentsfleetd_refused] {}: {}",
                code.as_str(),
                Problem::of(code).hint()
            )
        );
    }
}

/// A kind outside the three, or an argument the tool does not take, never
/// leaves the runner.
#[tokio::test]
async fn an_add_the_schema_refuses_sends_nothing() {
    for arguments in [
        json!({"kind": "cron"}),
        json!({"kind": "webhook", "url": "https://x"}),
        json!({"source": "sentry"}),
    ] {
        let (output, asked) = called(TriggerAdd, Ok(String::new()), arguments.clone()).await;

        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{arguments}"
        );
        assert_eq!(asked, [], "{arguments}");
    }
}

/// The list reads the fleet's triggers as the daemon answered them.
#[tokio::test]
async fn the_list_reads_the_fleets_triggers() {
    let (output, asked) = called(TriggerList, Ok(r#"{"items":[]}"#.to_owned()), json!({})).await;

    assert_eq!(output.text, r#"{"items":[]}"#);
    assert_eq!(asked, [Asked::Triggers(OwnedTriggerCall::List)]);
}

/// A removal names the trigger and reads `done` for the empty answer; an id
/// that is not one never leaves the runner.
#[tokio::test]
async fn a_removal_names_its_trigger_and_an_id_that_is_not_one_is_refused() {
    let (removed, asked) = called(
        TriggerRemove,
        Ok(String::new()),
        json!({"trigger_id": TRIGGER}),
    )
    .await;
    assert_eq!(removed.text, "done");
    assert_eq!(
        asked,
        [Asked::Triggers(OwnedTriggerCall::Remove(
            TRIGGER.to_owned()
        ))]
    );

    let (refused, asked) = called(
        TriggerRemove,
        Ok(String::new()),
        json!({"trigger_id": "sentry"}),
    )
    .await;
    assert_eq!(
        refused.text,
        format!("[invalid_arguments] {DETAIL_TRIGGER_ID}")
    );
    assert_eq!(asked, []);
}

/// A GitHub App subscription leaves as an App event with its repositories and
/// no source, which the daemon fills in as GitHub.
#[tokio::test]
async fn an_app_event_add_leaves_as_a_github_subscription() {
    let (_output, asked) = called(
        TriggerAdd,
        Ok(r#"{"id":"t","kind":"app_event","source":"github"}"#.to_owned()),
        json!({"kind": "app_event", "events": ["pull_request"], "repositories": ["org/app"]}),
    )
    .await;

    assert_eq!(
        asked,
        [Asked::Triggers(OwnedTriggerCall::Add {
            call_number: 1,
            trigger: NewTrigger {
                kind: TriggerKind::AppEvent,
                source: None,
                events: Some(vec![Cow::Borrowed("pull_request")]),
                repositories: Some(vec![Cow::Borrowed("org/app")]),
                verifier: None,
                secret_name: None,
                channel: None,
            },
        })]
    );
}
