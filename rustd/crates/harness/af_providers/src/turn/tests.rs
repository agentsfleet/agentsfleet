//! The runtime host's turns: the wire's model under the run's observer, its
//! failures reported under the run's codes. The wire's own suite is
//! guffbolt-ai's (`guffbolt_ai::wire::connected`).

#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use super::*;
use crate::turn::observer::RunObserver;
use af_core::test_util::trace::Capture;
use bytes::Bytes;
use futures_util::stream;
use guffbolt_ai::wire::transport::{Body, Send, Sent};
use guffbolt_ai::wire::{Api, Route};
use std::sync::{Arc, Mutex};

const CONTEXT_TOKENS: u32 = 1024;
/// Repetitions that put a short phrase past the native-count threshold: about
/// two `o200k_base` tokens each, plus the proxy's quarter, is well over 80
/// percent of [`CONTEXT_TOKENS`]. Native counting fires only near capacity.
const NEAR_CAPACITY: usize = 512;
const KEY: &str = "private-runtime-key";
const MODEL: &str = "model-next";
const PROVIDER: &str = "openai";
const TYPE: &str = "type";
const ID: &str = "id";
const INPUT: &str = "input_tokens";

/// A Responses provider and its count endpoint, recording each path sent;
/// a silent one sends its generation's headers and then nothing.
struct Physical {
    paths: Mutex<Vec<String>>,
    silent: bool,
}

impl Send for Physical {
    fn send(&self, request: http::Request<Bytes>) -> Sent {
        let path = request.uri().path().to_owned();
        self.paths.lock().unwrap().push(path.clone());
        let silent = self.silent;
        Box::pin(async move {
            let counting = path.ends_with(INPUT);
            let body = if counting {
                r#"{"input_tokens":123}"#.to_owned()
            } else {
                let response = serde_json::json!({TYPE:"response.completed","response":{
                    ID:"r","status":"completed","output":[{TYPE:"message","role":"assistant",
                    ID:"m","content":[{TYPE:"output_text","text":"answer","annotations":[]}]}],
                    "usage":{INPUT:123,"output_tokens":2,"total_tokens":125}}});
                format!("event: response.completed\ndata: {response}\n\n")
            };
            let streamed: Body = if silent && !counting {
                Box::pin(stream::pending())
            } else {
                Box::pin(stream::once(async move { Ok(Bytes::from(body)) }))
            };
            let mut response = http::Response::new(streamed);
            response.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(if counting {
                    "application/json"
                } else {
                    "text/event-stream"
                }),
            );
            Ok(response)
        })
    }
}

fn turns(transport: impl Send + 'static) -> Turns {
    let route = Route::new(
        Api::Responses,
        "https://api.openai.com/v1",
        Some(PROVIDER),
        37,
        CONTEXT_TOKENS,
    )
    .unwrap();
    let connected = Connected::new(route, MODEL, KEY, transport)
        .with_observer(Arc::new(RunObserver::new("run-count", PROVIDER)));
    Turns::new(connected, "run-count", PROVIDER)
}

fn physical(silent: bool) -> Arc<Physical> {
    Arc::new(Physical {
        paths: Mutex::default(),
        silent,
    })
}

fn request(messages: &[guffbolt::Message]) -> Request<'_> {
    Request {
        model: MODEL,
        instructions: "private pinned state",
        messages,
        tools: &[],
        hosted: &[],
        cache_key: "",
        max_output_tokens: Some(37),
        single_attempt: true,
    }
}

#[tokio::test]
async fn runtime_confirms_each_current_request_and_calibrates_authoritative_usage()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let captured = Capture::install_filtered(af_inference::log_filter(
        tracing::level_filters::LevelFilter::TRACE,
    ));
    let physical = physical(false);
    let turns = turns(Arc::clone(&physical));
    for text in ["private first source ", "private second source "] {
        let messages = [guffbolt::Message::User(text.repeat(NEAR_CAPACITY))];
        let chunks: Vec<_> = turns.stream(request(&messages)).collect().await;
        assert!(chunks.iter().all(Result::is_ok));
        assert!(
            chunks
                .iter()
                .any(|chunk| matches!(chunk, Ok(Chunk::Usage(_))))
        );
    }
    let paths = physical
        .paths
        .lock()
        .map_err(|_poisoned| "recorder poisoned")?;
    assert_eq!(
        *paths,
        [
            "/v1/responses/input_tokens",
            "/v1/responses",
            "/v1/responses/input_tokens",
            "/v1/responses"
        ]
    );
    let calibration: Vec<_> = captured
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some("model_input_calibration"))
        .collect();
    assert_eq!(calibration.len(), 2);
    assert!(
        calibration
            .iter()
            .all(|event| event.field("method") == Some("provider_count")
                && event.field("actual_input_tokens") == Some("123")
                && event.field("correlation_id") == Some("run-count"))
    );
    let rendered = format!("{:?}{turns:?}", captured.events());
    assert!(!rendered.contains("private "));
    assert!(!rendered.contains(KEY));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_silent_runtime_call_records_deadline_and_cancellation_once()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    use crate::observe::labels::CallOutcome;
    use crate::testing::{Observed, Tally, scoped};
    let (recorder, received) = Tally::new();
    scoped(recorder, async {
        let turns = turns(physical(true));
        let messages = [guffbolt::Message::User("probe".into())];
        let expired: Vec<_> = turns.stream(request(&messages)).collect().await;
        assert!(expired.iter().any(Result::is_err));
        let cancelled = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            turns.stream(request(&messages)).collect::<Vec<_>>(),
        )
        .await;
        assert!(
            cancelled.is_err(),
            "caller cancellation must stop the silent stream"
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await?;
    let outcomes: Vec<_> = received
        .try_iter()
        .filter_map(|record| match record {
            Observed::ModelCall(_, _, outcome, _, _) => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes, [CallOutcome::Deadline, CallOutcome::Cancelled]);
    Ok(())
}

#[test]
fn a_conversation_that_cannot_be_sent_keeps_the_wires_words() {
    let turns = turns(physical(false));
    let messages = [guffbolt::Message::ToolResult {
        call_id: "call-nobody-made".into(),
        output: "4".into(),
        image: None,
    }];

    let refused = turns.measure(request(&messages)).unwrap_err();

    assert_eq!(
        refused.detail(),
        "the conversation cannot be sent: the result for call call-nobody-made answers no call"
    );
    assert_eq!(
        refused.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert_eq!(refused.failure_class(), None);
}

#[tokio::test]
async fn an_input_that_leaves_no_output_is_the_fleets_configuration() {
    let route = Route::new(Api::Chat, "https://gateway.example/v1", None, 37, 64).unwrap();
    let physical = physical(false);
    let connected = Connected::new(route, MODEL, KEY, Arc::clone(&physical));
    let turns = Turns::new(connected, "run-window", PROVIDER);
    let messages = [guffbolt::Message::User("private source ".repeat(64))];

    let chunks: Vec<_> = turns.stream(request(&messages)).collect().await;

    let failure = chunks.into_iter().find_map(Result::err).unwrap();
    assert_eq!(
        failure.code(),
        af_core::error_code::AGENTSFLEET_INVALID_CONFIG
    );
    assert_eq!(failure.failure_class(), None);
    assert!(
        physical.paths.lock().unwrap().is_empty(),
        "nothing was sent"
    );
}
