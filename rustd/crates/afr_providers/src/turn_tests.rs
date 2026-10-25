//! The runtime host counts the immutable request before opening its real SDK stream.
use super::*;
use afd_core::test_util::trace::Capture;
use afd_inference::transport::SendRequest;
use bytes::Bytes;
use futures_util::future::BoxFuture;
use rig_core::http_client::{self as rig_http, StreamingResponse};
use std::sync::{Arc, Mutex};

const CONTEXT_TOKENS: u32 = 1024;
const KEY: &str = "private-runtime-key";
const MODEL: &str = "model-next";
const PROVIDER: &str = "openai";
const TYPE: &str = "type";
const ID: &str = "id";
const INPUT: &str = "input_tokens";

struct Physical {
    paths: Mutex<Vec<String>>,
    status: u16,
}

impl SendRequest for Physical {
    fn send(
        &self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'static, rig_http::Result<StreamingResponse>> {
        let path = request.uri().path().to_owned();
        let recorded = self
            .paths
            .lock()
            .map(|mut paths| paths.push(path.clone()))
            .map_err(|_poisoned| std::io::Error::other("recorder poisoned"));
        let status = self.status;
        Box::pin(async move {
            recorded.map_err(rig_http::Error::instance)?;
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
            let mut response =
                http::Response::new(Box::pin(stream::once(async move { Ok(Bytes::from(body)) }))
                    as rig_http::BoxedStream);
            *response.status_mut() =
                http::StatusCode::from_u16(if counting { status } else { 200 })
                    .map_err(rig_http::Error::instance)?;
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

fn turns(status: u16) -> Result<(Turns, Arc<Physical>)> {
    let settings = ModelInference {
        api: InferenceApi::Responses,
        base_url: "https://api.openai.com/v1".into(),
        dialect: Some(PROVIDER.into()),
        max_output_tokens: 37,
    };
    let physical = Arc::new(Physical {
        paths: Mutex::default(),
        status,
    });
    let transport = HttpTransport::new(Arc::<Physical>::clone(&physical));
    let route =
        afd_inference::Configured::new(&settings, CONTEXT_TOKENS).map_err(raise::configuration)?;
    let model = route.model(MODEL, KEY, transport.clone());
    Ok((
        Turns {
            model,
            settings,
            model_context_tokens: CONTEXT_TOKENS,
            lease_id: "lease-count".into(),
            provider: PROVIDER.into(),
            counting_key: SecretString::new(KEY.into()),
            counting_transport: transport,
        },
        physical,
    ))
}

fn request(messages: &[crate::provider::Message]) -> Request<'_> {
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
-> Result<(), Box<dyn std::error::Error>> {
    let captured = Capture::install_filtered(afd_inference::log_filter(
        tracing::level_filters::LevelFilter::TRACE,
    ));
    let (turns, physical) = turns(200)?;
    for text in ["private first source ", "private second source "] {
        let messages = [crate::provider::Message::User(text.repeat(64))];
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
                && event.field("actual_input_tokens") == Some("123"))
    );
    let rendered = format!("{:?}{turns:?}", captured.events());
    assert!(!rendered.contains("private "));
    assert!(!rendered.contains(KEY));
    Ok(())
}

#[tokio::test]
async fn refused_native_counts_never_open_a_paid_runtime_attempt()
-> Result<(), Box<dyn std::error::Error>> {
    for status in [400, 401, 429, 500] {
        let (turns, physical) = turns(status)?;
        let messages = [crate::provider::Message::User("private source ".repeat(96))];
        let chunks: Vec<_> = turns.stream(request(&messages)).collect().await;
        assert_eq!(chunks.len(), 1);
        assert!(chunks.first().is_some_and(Result::is_err));
        assert_eq!(
            *physical
                .paths
                .lock()
                .map_err(|_poisoned| "recorder poisoned")?,
            ["/v1/responses/input_tokens"]
        );
    }
    Ok(())
}
