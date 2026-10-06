//! The three provider wires against a fake provider on a real socket, driven
//! by the real loop: a tool turn each, bounded retry, the key kept to its one
//! header, `web_search` as a hosted spec, and a stream cut before its turn
//! ended, opened again within the same bound. How a turn ends is `ends`; which
//! host it reaches is `hosts`; one turn read below the loop is `turns`; an
//! image a tool read riding the next turn is `images`.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test target: a failed precondition should fail the test loudly"
)]

mod support;

#[path = "providers/ends.rs"]
mod ends;
#[path = "providers/hosts.rs"]
mod hosts;
#[path = "providers/images.rs"]
mod images;
#[path = "providers/retries.rs"]
mod retries;
#[path = "providers/turns.rs"]
mod turns;

use std::time::{Duration, Instant};

use afd_core::test_util::trace::Capture;
use afd_wire::report::{FailureClass, ResultOutcome};
use afd_wire::tool_trace::ToolCallStatus;
use afr_tools::catalog::{UPDATE_PLAN, WEB_SEARCH};
use serde_json::json;
use tracing::level_filters::LevelFilter;

use self::support::wires::{CACHED_TOKENS, PROMPT_TOKENS, Wire};
use self::support::{Fake, KEY, LEASE_ID, Reply, TOKEN, engine, lease, run};

/// The answer every scripted run ends on.
const ANSWER: &str = "It is 4.";
/// The call id every scripted call carries.
const CALL_ID: &str = "call-1";
/// The wait the fake's 429 asks for, as the header spells it.
const RETRY_AFTER: &str = "1";
/// What a refused hosted call reads back, as the router spells it.
const HOSTED_REFUSAL: &str = "[hosted_tool_unavailable]";
/// Sends per turn, the first included: the transport's retry and a cut
/// turn's reopening share the bound.
const ATTEMPTS: usize = 3;

#[tokio::test]
async fn test_each_provider_drives_a_tool_turn() {
    for wire in Wire::ALL {
        let arguments = json!({"expression": "2+2"});
        let mut fake = Fake::serve(vec![
            wire.call(CALL_ID, UPDATE_PLAN.name(), &arguments),
            wire.answer(ANSWER),
        ])
        .await;
        let provider = wire.provider();
        let leased = lease(&provider, &[UPDATE_PLAN.name()], "what is 2+2?");

        let (output, _frames) = run(&engine(&fake), &leased).await;

        assert!(
            matches!(output.result.outcome, ResultOutcome::Completed(_)),
            "{wire:?}: {:?}",
            output.result.outcome
        );
        assert_eq!(output.result.content, ANSWER, "{wire:?}");
        let trace = output.trace.unwrap();
        assert_eq!(trace.calls.len(), 1, "{wire:?}");
        assert_eq!(trace.calls[0].status, ToolCallStatus::Succeeded, "{wire:?}");
        let seen = fake.seen();
        assert_eq!(seen.len(), 2, "{wire:?}: one request per turn");
        assert!(
            seen.iter().all(|request| request.path == wire.path()),
            "{wire:?}"
        );
        assert_eq!(
            wire.results(&seen[1].body),
            [UPDATE_PLAN.name()],
            "{wire:?}: the call's result goes back in the wire's own shape"
        );
        assert_eq!(
            (
                output.result.input_tokens,
                output.result.cached_input_tokens
            ),
            (2 * (PROMPT_TOKENS - CACHED_TOKENS), 2 * CACHED_TOKENS),
            "{wire:?}: two turns, the cache reads counted once and apart"
        );
    }
}

#[tokio::test]
async fn test_provider_retry_honours_retry_after() {
    let capture = Capture::install();
    let wire = Wire::Messages;
    let mut fake = Fake::serve(vec![
        Reply::Status {
            status: 429,
            retry_after: Some(RETRY_AFTER),
        },
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[], "hello");
    let started = Instant::now();

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let asked = Duration::from_secs(RETRY_AFTER.parse().unwrap());
    assert!(started.elapsed() >= asked, "the wait was honoured");
    assert_eq!(output.result.content, ANSWER);
    assert_eq!(fake.seen().len(), 2);
    let retried = capture.only("provider_retry");
    assert_eq!(retried.level, tracing::Level::WARN);
    assert_eq!(retried.field("status"), Some("429"));
    assert_eq!(retried.field("attempt"), Some("1"));
    let asked_ms = asked.as_millis().to_string();
    assert_eq!(retried.field("wait_ms"), Some(asked_ms.as_str()));
    assert_eq!(retried.field("lease_id"), Some(LEASE_ID));
    assert_eq!(retried.field("provider"), Some("anthropic"));
}

#[tokio::test]
async fn a_refusal_ends_the_run_on_its_first_answer_naming_the_status() {
    let mut fake = Fake::serve(vec![Reply::Status {
        status: 401,
        retry_after: None,
    }])
    .await;
    let leased = lease(&Wire::Responses.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a refused turn fails the run");
    };
    assert_eq!(failure.class, None, "the fleet's error carries no reason");
    assert!(failure.detail.contains("401"), "{}", failure.detail);
    assert_eq!(fake.seen().len(), 1, "a 4xx is never retried");
}

#[tokio::test]
async fn a_fault_is_retried_three_sends_and_then_ends_naming_its_status() {
    let fault = Reply::Status {
        status: 503,
        retry_after: Some("0"),
    };
    let mut fake = Fake::serve(vec![fault.clone(), fault.clone(), fault]).await;
    let leased = lease(&Wire::Chat.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("an unanswered turn fails the run");
    };
    assert!(failure.detail.contains("503"), "{}", failure.detail);
    assert_eq!(fake.seen().len(), 3, "the ceiling is three sends");
}

#[tokio::test]
async fn test_api_key_never_leaves_the_supervisor() {
    for wire in Wire::ALL {
        // What the journal holds at the loudest level an operator can name.
        let capture = Capture::install_filtered(afr_providers::log_filter(LevelFilter::TRACE));
        let echoed = json!({"expression": KEY, "token": TOKEN});
        let mut fake = Fake::serve(vec![
            wire.call(CALL_ID, UPDATE_PLAN.name(), &echoed),
            wire.answer(&format!("the key was {KEY}")),
        ])
        .await;
        let leased = lease(
            &wire.provider(),
            &[UPDATE_PLAN.name()],
            &format!("use {TOKEN}"),
        );

        let (output, frames) = run(&engine(&fake), &leased).await;

        let seen = fake.seen();
        assert!(
            seen.iter()
                .all(|request| wire.carries_key(&request.headers, KEY)),
            "{wire:?}"
        );
        for request in &seen {
            let mut elsewhere = request
                .headers
                .iter()
                .filter(|(name, _)| !matches!(name.as_str(), "x-api-key" | "authorization"));
            assert!(
                elsewhere.all(|(_, value)| !value.to_str().unwrap().contains(KEY)),
                "{wire:?}: the key rides one header"
            );
            let prompt = request.body.to_string();
            assert!(
                !prompt.contains(KEY) && !prompt.contains(TOKEN),
                "{wire:?}: {prompt}"
            );
        }
        let rendered = format!("{frames:?}{output:?}");
        assert!(
            !rendered.contains(KEY) && !rendered.contains(TOKEN),
            "{wire:?}: {rendered}"
        );
        let logged = format!("{:?}", capture.events());
        assert!(
            !logged.contains(KEY) && !logged.contains(TOKEN),
            "{wire:?}: {logged}"
        );
        assert!(
            output.result.content.contains("«secret:llm.api_key»"),
            "{wire:?}"
        );
    }
}

#[tokio::test]
async fn test_web_search_is_a_hosted_spec() {
    for wire in [Wire::Messages, Wire::Responses] {
        let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
        let leased = lease(&wire.provider(), &[WEB_SEARCH.name()], "search");

        let (output, _frames) = run(&engine(&fake), &leased).await;

        assert_eq!(output.result.content, ANSWER, "{wire:?}");
        let offered = wire.offered(&fake.seen()[0].body);
        let spec = offered
            .iter()
            .find(|tool| tool.starts_with(WEB_SEARCH.name()));
        assert!(spec.is_some(), "{wire:?}: {offered:?}");
    }
    let wire = Wire::Chat;
    let mut fake = Fake::serve(vec![
        wire.call(CALL_ID, WEB_SEARCH.name(), &json!({"query": "x"})),
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[WEB_SEARCH.name()], "search");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let seen = fake.seen();
    assert!(
        wire.offered(&seen[0].body).is_empty(),
        "chat has no hosted spec"
    );
    let refused = wire.results(&seen[1].body);
    assert!(refused[0].starts_with(HOSTED_REFUSAL), "{refused:?}");
    assert_eq!(
        output.trace.unwrap().calls[0].status,
        ToolCallStatus::Failed
    );
}

#[tokio::test]
async fn a_stream_cut_before_its_turn_ended_is_opened_again_then_a_lost_connection() {
    for wire in Wire::ALL {
        let Reply::Stream(mut events) = wire.answer(ANSWER) else {
            panic!("an answer streams");
        };
        // Messages and Responses open on a frame that shows nothing; a chat
        // stream's first chunk is already its text.
        let silent = match wire {
            Wire::Messages | Wire::Responses => 1,
            Wire::Chat => 0,
        };
        events.truncate(silent);
        let cut = Reply::Stream(events);
        let mut fake = Fake::serve(vec![cut; ATTEMPTS]).await;
        let leased = lease(&wire.provider(), &[], "hello");

        let (output, _frames) = run(&engine(&fake), &leased).await;

        let ResultOutcome::Failed(failure) = output.result.outcome else {
            panic!("{wire:?}: a cut turn is no answer");
        };
        assert_eq!(failure.class, Some(FailureClass::TransportLoss), "{wire:?}");
        assert_eq!(
            fake.seen().len(),
            ATTEMPTS,
            "{wire:?}: nothing showed, so it reopened"
        );
    }
}

// Its text already went out live; a second pass would show it twice.
#[tokio::test]
async fn a_stream_cut_after_its_text_showed_is_lost_without_opening_again() {
    let wire = Wire::Chat;
    let Reply::Stream(mut events) = wire.answer(ANSWER) else {
        panic!("an answer streams");
    };
    events.truncate(1);
    let mut fake = Fake::serve(vec![Reply::Stream(events), wire.answer(ANSWER)]).await;
    let leased = lease(&wire.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a cut turn is no answer");
    };
    assert_eq!(failure.class, Some(FailureClass::TransportLoss));
    assert_eq!(fake.seen().len(), 1);
}

#[tokio::test]
async fn a_cut_turn_that_ends_whole_on_its_second_opening_answers() {
    let wire = Wire::Messages;
    let Reply::Stream(mut cut) = wire.answer(ANSWER) else {
        panic!("an answer streams");
    };
    cut.truncate(1);
    let mut fake = Fake::serve(vec![Reply::Stream(cut), wire.answer(ANSWER)]).await;
    let leased = lease(&wire.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    assert_eq!(output.result.content, ANSWER);
    assert_eq!(fake.seen().len(), 2);
}

#[tokio::test]
async fn a_provider_error_mid_stream_ends_the_turn_as_a_transport_loss() {
    let error = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\n";
    let fake = Fake::serve(vec![Reply::Stream(vec![error.to_owned()])]).await;
    let leased = lease(&Wire::Messages.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("an ended turn is no answer");
    };
    assert_eq!(failure.class, Some(FailureClass::TransportLoss));
    assert!(
        failure.detail.ends_with("overloaded_error"),
        "{}",
        failure.detail
    );
}
