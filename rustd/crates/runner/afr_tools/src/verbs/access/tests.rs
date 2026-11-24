//! `request_access`: a card ends the run waiting, an approved or pending grant
//! lets it go on, and every refusal reads `[kind] sentence`.

use af_core::error_code::{self, ErrorCode};
use af_core::problem::Problem;
use af_wire::access_verb::AccessRequest;
use guffbolt::ToolOutput;
use serde_json::json;

use super::RequestAccess;
use crate::handler::Typed;
use crate::runtime::ToolErrorCode;
use crate::testing::{Asked, RecordingVerbs, call, run_with};
use crate::verbs::{AccessAnswer, Unanswered};

/// Calls `request_access` with `arguments` through verbs answering `answer`.
async fn requested(
    answer: Result<AccessAnswer, Unanswered>,
    arguments: serde_json::Value,
) -> (ToolOutput, Vec<Asked>) {
    let verbs = RecordingVerbs::answering(Ok(String::new()), Ok(true)).granting(answer);
    let run = run_with(&verbs);
    let output = call(&*Typed::boxed(RequestAccess), &run, arguments).await;
    (output, verbs.asked())
}

/// What the model reads for a refusal naming `code`.
fn refusal(code: ErrorCode) -> String {
    format!(
        "[agentsfleetd_refused] {}: {}",
        code.as_str(),
        Problem::of(code).hint()
    )
}

/// Each refusal the daemon answers reaches the model as its kind and a
/// sentence naming the registry code, and the call asks no one, so the run
/// goes on and the model may correct it.
#[tokio::test]
async fn a_refused_access_request_returns_its_kind_and_sentence() {
    for code in [
        error_code::FLEET_ACCESS_NAME_NOT_FOUND,
        error_code::FLEET_PROVIDER_NOT_CONNECTED,
        error_code::AGENTSFLEET_INVALID_CONFIG,
    ] {
        let (output, _asked) = requested(
            Err(Unanswered::Refused(Some(code))),
            json!({"name": "sentry"}),
        )
        .await;

        assert_eq!(output.text, refusal(code), "{code:?}");
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::AgentsfleetdRefused.into())
        );
        assert_eq!(output.asks, None, "a refusal asks no one: {code:?}");
    }

    let (unnamed, _asked) = requested(Err(Unanswered::Refused(None)), json!({"name": "x"})).await;
    assert_eq!(
        unnamed.text,
        "[agentsfleetd_refused] agentsfleetd refused the request"
    );
    let (unreachable, _asked) = requested(Err(Unanswered::Unreachable), json!({"name": "x"})).await;
    assert_eq!(
        unreachable.text,
        "[agentsfleetd_unreachable] agentsfleetd did not answer; try again shortly"
    );
    assert_eq!(unreachable.asks, None);
}

/// A card raised for this turn ends the run waiting on the person, and the
/// request leaves with the name and scope as the model gave them.
#[tokio::test]
async fn a_raised_card_ends_the_run_waiting_on_the_person() {
    let scope = serde_json::Map::from_iter([
        ("repositories".to_owned(), json!(["org/app"])),
        ("access".to_owned(), json!("read")),
    ]);
    let (output, asked) = requested(
        Ok(AccessAnswer::Asked),
        json!({"name": "github", "scope": scope}),
    )
    .await;

    assert_eq!(
        output.asks.as_deref(),
        Some("Waiting for a person to approve access to github.")
    );
    assert_eq!(output.error_code, None);
    assert_eq!(
        asked,
        [Asked::Access(AccessRequest {
            fencing_token: 0,
            name: "github".into(),
            scope: Some(scope),
        })]
    );
}

/// An approved grant, or a card an earlier request raised, lets the run go
/// on: neither asks the person again.
#[tokio::test]
async fn a_covered_or_pending_request_lets_the_run_go_on() {
    let (covered, _asked) = requested(Ok(AccessAnswer::Covered), json!({"name": "sentry"})).await;
    assert_eq!(covered.asks, None);
    assert_eq!(
        covered.text,
        "sentry is already approved for this fleet, and this run holds it."
    );

    let (pending, _asked) = requested(Ok(AccessAnswer::Pending), json!({"name": "sentry"})).await;
    assert_eq!(pending.asks, None);
    assert_eq!(
        pending.text,
        "An earlier request for sentry still waits on a person's approval; this run does not \
         hold it."
    );
}

/// A name the wire's bounds refuse, an argument the tool does not take, or a
/// scope that is not an object never leaves the runner.
#[tokio::test]
async fn a_request_no_daemon_would_read_is_refused_here() {
    for arguments in [
        json!({"name": ""}),
        json!({"name": "sen\u{0}try"}),
        json!({"name": "sentry", "host": "sentry.io"}),
        json!({"name": "github", "scope": ["org/app"]}),
        json!({}),
    ] {
        let (output, asked) = requested(Ok(AccessAnswer::Asked), arguments.clone()).await;

        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{arguments}"
        );
        assert_eq!(output.asks, None);
        assert_eq!(asked, [], "{arguments}");
    }
}
