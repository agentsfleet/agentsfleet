//! A connected model counts the immutable request before opening its real
//! stream, opens a turn again only while nothing was shown or spent, and
//! holds the bounds an embedder set, and no others.

#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::{StreamExt as _, stream};

use super::Connected;
use crate::wire::observe::FailureCause;
use crate::wire::recorder::{Recorder, Seen};
use crate::wire::transport::{Body, Send, SendError, Sent};
use crate::wire::{Api, Retry, Route};
use crate::{Chunk, Message, Model as _, Request};

#[cfg(feature = "reqwest")]
mod bounds;
mod model_id;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Generation {
    Complete,
    Silent,
    Truncated,
}

/// A Responses provider and its count endpoint, recording each path sent.
struct Physical {
    paths: Mutex<Vec<String>>,
    /// The `model` each request's body named.
    models: Mutex<Vec<String>>,
    status: u16,
    generation: Generation,
}

impl Send for Physical {
    fn send(&self, request: http::Request<Bytes>) -> Sent {
        let path = request.uri().path().to_owned();
        self.paths.lock().unwrap().push(path.clone());
        let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
        self.models.lock().unwrap().push(
            body.get("model")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
        let status = self.status;
        let silent = self.generation == Generation::Silent;
        let truncated = self.generation == Generation::Truncated;
        Box::pin(async move {
            let counting = path.ends_with(INPUT);
            let body = if counting {
                r#"{"input_tokens":123}"#.to_owned()
            } else if truncated {
                String::new()
            } else {
                let response = serde_json::json!({TYPE:"response.completed","response":{
                    ID:"r","status":"completed","output":[{TYPE:"message","role":"assistant",
                    ID:"m","content":[{TYPE:"output_text","text":"answer","annotations":[]}]}],
                    "usage":{INPUT:123,"output_tokens":2,"total_tokens":125}}});
                format!("event: response.completed\ndata: {response}\n\n")
            };
            let streamed: Body = if silent && !counting {
                stream::pending().boxed()
            } else {
                stream::once(async move { Ok(Bytes::from(body)) }).boxed()
            };
            let mut response = http::Response::new(streamed);
            *response.status_mut() =
                http::StatusCode::from_u16(if counting { status } else { 200 }).unwrap();
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

fn turns(status: u16) -> (Connected, Arc<Physical>, Arc<Recorder>) {
    turns_with_stream(status, Generation::Complete)
}

fn turns_with_stream(
    status: u16,
    generation: Generation,
) -> (Connected, Arc<Physical>, Arc<Recorder>) {
    let route = Route::new(
        Api::Responses,
        "https://api.openai.com/v1",
        Some(PROVIDER),
        37,
        CONTEXT_TOKENS,
    )
    .unwrap();
    let physical = Arc::new(Physical {
        paths: Mutex::default(),
        models: Mutex::default(),
        status,
        generation,
    });
    let recorder = Recorder::new();
    let connected =
        Connected::new(route, MODEL, KEY, Arc::clone(&physical))
            .with_observer(Arc::<Recorder>::clone(&recorder));
    (connected, physical, recorder)
}

fn request(messages: &[Message]) -> Request<'_> {
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
async fn configured_opening_budget_stops_truncated_unpaid_generations() {
    for openings in [1, 2] {
        let (turns, physical, recorder) = turns_with_stream(200, Generation::Truncated);
        let turns = turns.with_retry(Retry::DEFAULT.with_openings(openings));
        let messages = [Message::User("probe".into())];
        let mut request = request(&messages);
        request.single_attempt = false;
        let chunks: Vec<_> = turns.stream(request).collect().await;
        assert!(chunks.iter().any(Result::is_err));
        let paths = physical.paths.lock().unwrap();
        assert_eq!(paths.len(), openings);
        assert!(paths.iter().all(|path| path == "/v1/responses"));
        let reopened = recorder
            .seen()
            .into_iter()
            .filter(|seen| matches!(seen, Seen::Reopened(..)))
            .count();
        assert_eq!(reopened, openings - 1);
    }
}

#[tokio::test]
async fn runtime_confirms_each_current_request_and_calibrates_authoritative_usage() {
    let (turns, physical, recorder) = turns(200);
    for text in ["private first source ", "private second source "] {
        let messages = [Message::User(text.repeat(NEAR_CAPACITY))];
        let chunks: Vec<_> = turns.stream(request(&messages)).collect().await;
        assert!(chunks.iter().all(Result::is_ok));
        assert!(
            chunks
                .iter()
                .any(|chunk| matches!(chunk, Ok(Chunk::Usage(_))))
        );
    }
    assert_eq!(
        *physical.paths.lock().unwrap(),
        [
            "/v1/responses/input_tokens",
            "/v1/responses",
            "/v1/responses/input_tokens",
            "/v1/responses"
        ]
    );
    let calibration = recorder.measured();
    assert_eq!(calibration.len(), 2);
    assert!(
        calibration
            .iter()
            .all(|measured| measured.method == "provider_count"
                && measured.actual_input_tokens == 123)
    );
    let rendered = format!("{:?}{turns:?}", recorder.seen());
    assert!(!rendered.contains("private "));
    assert!(!rendered.contains(KEY));
}

#[tokio::test]
async fn refused_native_counts_never_open_a_paid_runtime_attempt() {
    for status in [400, 401, 429, 500] {
        let (turns, physical, _recorder) = turns(status);
        let messages = [Message::User("private source ".repeat(NEAR_CAPACITY))];
        let chunks: Vec<_> = turns.stream(request(&messages)).collect().await;
        assert_eq!(chunks.len(), 1);
        assert!(chunks.first().is_some_and(Result::is_err));
        assert_eq!(
            *physical.paths.lock().unwrap(),
            ["/v1/responses/input_tokens"]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_silent_runtime_call_records_deadline_and_cancellation_once() {
    let (turns, _physical, recorder) = turns_with_stream(200, Generation::Silent);
    let messages = [Message::User("probe".into())];
    let expired: Vec<_> = turns.stream(request(&messages)).collect().await;
    assert!(expired.iter().any(Result::is_err));
    let cancelled = tokio::time::timeout(
        Duration::from_secs(1),
        turns.stream(request(&messages)).collect::<Vec<_>>(),
    )
    .await;
    assert!(
        cancelled.is_err(),
        "caller cancellation must stop the silent stream"
    );
    let outcomes: Vec<_> = recorder.ended().iter().map(|turn| turn.cause).collect();
    assert_eq!(
        outcomes,
        [Some(FailureCause::Deadline), Some(FailureCause::Cancelled)]
    );
}

#[test]
fn a_connected_model_prints_its_api_and_model_and_never_its_key() {
    let (turns, _physical, _recorder) = turns(200);

    let rendered = format!("{turns:?}");

    assert!(rendered.contains("Responses"), "{rendered}");
    assert!(rendered.contains(MODEL), "{rendered}");
    assert!(!rendered.contains(KEY), "{rendered}");
    assert!(turns.accepts_images());
    assert_eq!(turns.route().host(), "api.openai.com");
}

#[test]
fn a_measure_counts_the_routes_own_wire() {
    let (turns, physical, _recorder) = turns(200);
    let messages = [Message::User("hello".into())];

    let measured = turns.measure(request(&messages)).unwrap();

    assert!(measured > 0);
    assert!(
        physical.paths.lock().unwrap().is_empty(),
        "nothing was sent"
    );
}

/// The answer text a turn streamed; only the suites over the default transport
/// read it.
#[cfg(feature = "reqwest")]
fn answered(chunks: &[crate::Result<Chunk>]) -> String {
    chunks
        .iter()
        .filter_map(|chunk| match chunk {
            Ok(Chunk::Text {
                kind: crate::TextKind::Answer,
                text,
            }) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// The one failure `chunks` ended on.
fn failure(chunks: Vec<crate::Result<Chunk>>) -> crate::Error {
    let mut failures: Vec<_> = chunks.into_iter().filter_map(Result::err).collect();
    assert_eq!(failures.len(), 1, "{failures:?}");
    failures.pop().unwrap()
}

/// A transport that refuses everything it is asked to send, as an
/// embedder's guard refuses a destination it never dials.
#[derive(Default)]
struct Guarded {
    sends: std::sync::atomic::AtomicUsize,
}

impl Send for Guarded {
    fn send(&self, _: http::Request<Bytes>) -> Sent {
        self.sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async {
            Err(SendError::Refused(Box::new(std::io::Error::other(
                "the destination resolves to a private address",
            ))))
        })
    }
}

// Value: a destination the embedder's guard refused is refused the same way
// on every send, so neither the retry nor a reopening repeats it.
#[tokio::test]
async fn a_send_the_guard_refused_is_not_retried() {
    let guard = Arc::new(Guarded::default());
    let route = Route::new(Api::Chat, "https://private.example/v1", None, 37, 128_000).unwrap();
    let recorder = Recorder::new();
    let connected =
        Connected::new(route, MODEL, KEY, Arc::clone(&guard))
            .with_observer(Arc::<Recorder>::clone(&recorder));
    let messages = [Message::User("hello".into())];
    let mut asked = request(&messages);
    asked.single_attempt = false;

    let chunks: Vec<_> = connected.stream(asked).collect().await;

    let refused = failure(chunks);
    assert!(refused.is_refused(), "{refused:?}");
    assert_eq!(guard.sends.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(
        !recorder
            .seen()
            .iter()
            .any(|seen| matches!(seen, Seen::Retrying(_) | Seen::Reopened(..))),
        "{:?}",
        recorder.seen()
    );
}
