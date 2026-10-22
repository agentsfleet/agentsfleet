//! Both production purposes use this reader; fixtures cross the real SDK HTTP boundary.
#![expect(clippy::unwrap_used, reason = "fixture preconditions must fail loudly")]

use super::*;
use crate::transport::{HttpTransport, SendRequest};
use afd_wire::inference::{InferenceApi, ModelInference};
use bytes::Bytes;
use futures_util::{future::BoxFuture, stream};
use rig_core::{
    http_client::{self as rig_http, StreamingResponse},
    message::Message,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::mpsc;

mod timing;
mod tracing;

const TEXT: &str = "data: {\"id\":\"r\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi 🌿\"},\"finish_reason\":null}]}\n\n";
const USAGE: &str = "data: {\"id\":\"r\",\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2}}\n\n";
const STOP: &str = "data: {\"id\":\"r\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

struct Physical {
    body: Mutex<Option<rig_http::BoxedStream>>,
    sends: AtomicUsize,
}

impl std::fmt::Debug for Physical {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Physical").finish_non_exhaustive()
    }
}

impl SendRequest for Physical {
    fn send(
        &self,
        _: http::Request<Bytes>,
    ) -> BoxFuture<'static, rig_http::Result<StreamingResponse>> {
        self.sends.fetch_add(1, Ordering::Relaxed);
        let body = self.body.lock().unwrap().take().unwrap();
        Box::pin(async move {
            Ok(http::Response::builder()
                .header(http::header::CONTENT_TYPE, "text/event-stream")
                .body(body)
                .unwrap())
        })
    }
}

fn open(body: rig_http::BoxedStream, control: TurnControl) -> (StreamTurn, Arc<Physical>) {
    let physical = Arc::new(Physical {
        body: Mutex::new(Some(body)),
        sends: AtomicUsize::new(0),
    });
    let transport = HttpTransport::new(Arc::<Physical>::clone(&physical));
    let settings = ModelInference {
        api: InferenceApi::Chat,
        base_url: "https://model.example/v1".into(),
        dialect: None,
        max_output_tokens: 37,
    };
    let route = Configured::new(&settings, 128_000).unwrap();
    let model = route.model("configured-model", "private-credential", transport);
    let turn = StreamTurn::open(
        &route,
        &model,
        CompletionRequest::new(Message::user("private prompt")),
        control,
    )
    .unwrap();
    (turn, physical)
}

fn channel() -> (mpsc::Sender<rig_http::Result<Bytes>>, rig_http::BoxedStream) {
    let (tx, rx) = mpsc::channel(8);
    let body = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|item| (item, rx))
    });
    (tx, Box::pin(body))
}

// Value: protects=first text arrives while the provider body is still open;
// fails_when=any shared HTTP or SDK layer collects the response before yielding.
#[tokio::test]
async fn first_text_arrives_before_the_provider_closes_its_response() {
    let (tx, body) = channel();
    let (mut turn, physical) = open(body, TurnControl::default());
    tx.send(Ok(Bytes::from_static(TEXT.as_bytes())))
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let StreamEvent::Text { text, .. } = turn.next().await.unwrap().unwrap() {
                break text;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(first, "Hi 🌿");
    assert!(!tx.is_closed(), "the provider has not finished sending");
    tx.send(Ok(Bytes::from_static(USAGE.as_bytes())))
        .await
        .unwrap();
    tx.send(Ok(Bytes::from_static(STOP.as_bytes())))
        .await
        .unwrap();
    drop(tx);
    let result = turn.finish().await;
    assert!(result.failure.is_none());
    assert_eq!(
        result.response.usage.input_tokens,
        Some(3),
        "{:?}",
        result.response.raw
    );
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn fragmented_unicode_and_framing_are_decoded_by_the_sdk() {
    let wire = format!("{TEXT}{USAGE}{STOP}");
    let chunks: Vec<_> = wire
        .bytes()
        .map(|byte| Bytes::from(vec![byte]))
        .map(Ok)
        .collect();
    let (mut turn, physical) = open(Box::pin(stream::iter(chunks)), TurnControl::default());
    let mut answer = String::new();
    while let Some(item) = turn.next().await {
        if let StreamEvent::Text { text, .. } = item.unwrap() {
            answer.push_str(&text);
        }
    }
    let result = turn.finish().await;
    assert_eq!(answer, "Hi 🌿");
    assert!(result.failure.is_none());
    assert_eq!(result.response.usage.output_tokens, Some(2));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn a_cut_response_retains_usage_and_never_becomes_a_completed_reply() {
    let pieces = [
        Ok(Bytes::from_static(TEXT.as_bytes())),
        Ok(Bytes::from_static(USAGE.as_bytes())),
        Err(rig_http::Error::instance(std::io::Error::other(
            "fixture disconnect",
        ))),
    ];
    let (turn, physical) = open(Box::pin(stream::iter(pieces)), TurnControl::default());
    let result = turn.finish().await;
    assert_eq!(result.cause, Some(FailureCause::Provider));
    assert_eq!(
        result.response.usage.input_tokens,
        Some(3),
        "{:?}",
        result.response.raw
    );
    assert_eq!(result.response.usage.output_tokens, Some(2));
    assert!(
        result.into_response().is_err(),
        "partial output cannot authorize execution"
    );
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test(start_paused = true)]
async fn a_hard_deadline_ends_a_silent_provider_without_resending() {
    let (_tx, body) = channel();
    let control = TurnControl {
        deadline: Some(Instant::now() + Duration::from_secs(2)),
        ..Default::default()
    };
    let (mut turn, physical) = open(body, control);
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Deadline))
    ));
    assert!(turn.next().await.is_none());
    let result = turn.finish().await;
    assert_eq!(result.cause, Some(FailureCause::Deadline));
    assert_eq!(result.response.usage.input_tokens, None);
    assert!(result.failure.unwrap().to_string().contains("deadline"));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test(start_paused = true)]
async fn the_shared_inactivity_bound_ends_an_unbounded_silent_turn() {
    let (_tx, body) = channel();
    let (mut turn, physical) = open(body, TurnControl::default());
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Deadline))
    ));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn cancellation_before_polling_never_sends_a_provider_request() {
    let control = TurnControl::default();
    control.cancellation.cancel();
    let (mut turn, physical) = open(Box::pin(stream::pending()), control);
    assert!(matches!(
        turn.next().await,
        Some(Err(FailureCause::Cancelled))
    ));
    assert_eq!(turn.finish().await.cause, Some(FailureCause::Cancelled));
    assert_eq!(physical.sends.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn diagnostics_never_include_private_text_or_credentials() {
    let (turn, _) = open(Box::pin(stream::empty()), TurnControl::default());
    assert!(!format!("{turn:?}").contains("private"));
    let result = turn.finish().await;
    assert!(!format!("{result:?}").contains("private"));
    assert!(
        result.failure.is_some(),
        "empty response cannot prove completion"
    );
}
