#![expect(clippy::unwrap_used, reason = "fixtures fail loudly on invalid setup")]

use super::*;
mod admission;
mod codecs;
mod counting;
use crate::wire::Api as InferenceApi;
use crate::wire::input_tokens;
use crate::wire::json::json_request;
use bytes::Bytes;
use rig_core::http_client::{
    self as http, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use rig_core::message::Message;
use rig_core::wasm_compat::WasmCompatSend;
use std::sync::{Arc, Mutex};

const UNUSED_OPERATION: &str = "unused transport operation";

/// What one route is built from, as a caller would hold it.
#[derive(Debug, Clone)]
struct ModelInference {
    api: InferenceApi,
    base_url: String,
    dialect: Option<String>,
    max_output_tokens: u32,
}

fn settings(api: InferenceApi) -> ModelInference {
    ModelInference {
        api,
        base_url: "https://inference.example/custom/v1".into(),
        dialect: None,
        max_output_tokens: 37,
    }
}

/// `value`'s route under a `context_tokens` window.
fn routed(value: &ModelInference, context_tokens: u32) -> Result<Route> {
    Route::new(
        value.api,
        &value.base_url,
        value.dialect.as_deref(),
        value.max_output_tokens,
        context_tokens,
    )
}

#[test]
fn rejects_unknown_or_wrong_api_dialects() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
    ] {
        let mut value = settings(api);
        for dialect in ["not-a-dialect", ""] {
            value.dialect = Some(dialect.into());
            assert!(routed(&value, 100).unwrap_err().is_invalid_request());
        }
    }
}

#[test]
fn a_native_api_refuses_any_saved_dialect() {
    for api in [InferenceApi::Gemini, InferenceApi::Ollama] {
        let mut value = settings(api);
        assert!(routed(&value, 100).is_ok(), "{api:?} without one");
        value.dialect = Some("openai".into());
        assert!(
            routed(&value, 100).unwrap_err().is_invalid_request(),
            "{api:?} has no dialect table to look one up in"
        );
    }
}

#[test]
fn tool_support_follows_the_selected_sdk_api_and_dialect() {
    for (api, dialect, expected) in [
        (InferenceApi::Chat, None, true),
        (InferenceApi::Chat, Some("perplexity"), false),
        (InferenceApi::Chat, Some("hyperbolic"), false),
        (InferenceApi::Chat, Some("mira"), false),
        (InferenceApi::Responses, Some("perplexity"), true),
        (InferenceApi::Messages, None, true),
    ] {
        let mut value = settings(api);
        value.dialect = dialect.map(str::to_owned);
        let route = routed(&value, 100).unwrap();
        assert_eq!(route.supports_tools("owner/model-next"), expected);
    }
}

/// Chat completions take text alone; every other API delivers a tool's images,
/// inside the result or in the user message rig's history adapter adds.
#[test]
fn only_chat_completions_drop_a_tools_images() {
    for (api, carries) in [
        (InferenceApi::Chat, false),
        (InferenceApi::Responses, true),
        (InferenceApi::Messages, true),
        (InferenceApi::Gemini, true),
        (InferenceApi::Ollama, true),
    ] {
        assert_eq!(api.carries_images(), carries, "{api:?}");
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<Request<Bytes>>>>);

impl HttpClientExt for Capture {
    fn send(
        &self,
        req: Request<Bytes>,
    ) -> impl Future<Output = http::Result<Response<LazyBody>>> + WasmCompatSend + 'static {
        self.0.lock().unwrap().push(req);
        async {
            Response::builder()
                .status(400)
                .body(Box::pin(async { Ok(Bytes::new()) }) as LazyBody)
                .map_err(http::Error::Protocol)
        }
    }

    fn send_multipart(
        &self,
        _: Request<MultipartForm>,
    ) -> impl Future<Output = http::Result<Response<LazyBody>>> + WasmCompatSend + 'static {
        std::future::ready(Err(http::Error::transient(std::io::Error::other(
            UNUSED_OPERATION,
        ))))
    }

    fn send_streaming(
        &self,
        req: Request<Bytes>,
    ) -> impl Future<Output = http::Result<StreamingResponse>> + WasmCompatSend {
        self.0.lock().unwrap().push(req);
        std::future::ready(Err(http::Error::transient(std::io::Error::other(
            UNUSED_OPERATION,
        ))))
    }
}

const AUTHORIZATION_HEADER: &str = "authorization";
/// The header Google authenticates a Gemini API request by.
const GEMINI_KEY_HEADER: &str = "x-goog-api-key";
const API_KEY_HEADER: &str = "x-api-key";
const ROUTES: [(InferenceApi, Option<&str>, &str, &str); 6] = [
    (
        InferenceApi::Chat,
        None,
        "/custom/v1/chat/completions",
        AUTHORIZATION_HEADER,
    ),
    (
        InferenceApi::Chat,
        Some("groq"),
        "/custom/v1/chat/completions",
        AUTHORIZATION_HEADER,
    ),
    (
        InferenceApi::Responses,
        None,
        "/custom/v1/responses",
        AUTHORIZATION_HEADER,
    ),
    (
        InferenceApi::Responses,
        Some("openai"),
        "/custom/v1/responses",
        AUTHORIZATION_HEADER,
    ),
    (
        InferenceApi::Messages,
        None,
        "/custom/v1/messages",
        API_KEY_HEADER,
    ),
    (
        InferenceApi::Messages,
        Some("anthropic"),
        "/custom/v1/messages",
        API_KEY_HEADER,
    ),
];

#[tokio::test]
async fn uses_persisted_destinations_and_accepts_arbitrary_configured_model_identifiers() {
    for (api, dialect, suffix, header) in ROUTES {
        let mut value = settings(api);
        value.dialect = dialect.map(str::to_owned);
        let route = routed(&value, 100).unwrap();
        let capture = Capture::default();
        let model = route.model("owner/model-next", "hidden-credential", capture.clone());
        assert_eq!(model.id(), Some("owner/model-next"));
        assert!(!format!("{model:?}").contains("hidden-credential"));
        let error = model
            .call(CompletionRequest::new(Message::user("hello")))
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("hidden-credential"));
        let seen = capture.0.lock().unwrap();
        let request = seen.first().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(request.uri().host(), Some("inference.example"));
        assert_eq!(request.uri().path(), suffix);
        assert!(request.headers().contains_key(header));
        assert_eq!(request.uri().query(), None, "a non-Gemini URL is untouched");
        assert!(!request.headers().contains_key(GEMINI_KEY_HEADER));
        let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
        assert_eq!(
            body.get("model"),
            Some(&serde_json::json!("owner/model-next"))
        );
        if api == InferenceApi::Messages {
            assert_eq!(body.get("max_tokens"), Some(&serde_json::json!(37)));
        }
    }
}

/// The key a Gemini route is given; no captured URL or error may contain it.
const GEMINI_KEY: &str = "gemini-hidden-credential";

/// rig's Gemini wire puts the key in the URL; the route's transport moves it
/// to `x-goog-api-key` before any transport sees the request, unary and
/// streamed alike.
#[tokio::test]
async fn a_gemini_request_carries_its_key_in_a_header_never_in_its_url() {
    use futures_util::StreamExt as _;

    let value = settings(InferenceApi::Gemini);
    let route = routed(&value, 100).unwrap();
    let capture = Capture::default();
    let model = route.model("gemini-next", GEMINI_KEY, capture.clone());
    let unary = model
        .call(CompletionRequest::new(Message::user("hello")))
        .await
        .unwrap_err();
    let mut streamed = model
        .stream(CompletionRequest::new(Message::user("hello")))
        .unwrap();
    let frame = streamed.next().await;
    let seen = capture.0.lock().unwrap();
    assert_eq!(seen.len(), 2, "one unary and one streamed send");
    for request in seen.iter() {
        assert_eq!(request.uri().host(), Some("inference.example"));
        assert!(!request.uri().to_string().contains(GEMINI_KEY));
        assert_eq!(
            request.headers().get(GEMINI_KEY_HEADER).unwrap(),
            GEMINI_KEY
        );
        assert!(!format!("{request:?}").contains(GEMINI_KEY));
    }
    for rendered in [
        unary.to_string(),
        format!("{unary:?}"),
        format!("{frame:?}"),
    ] {
        assert!(!rendered.contains(GEMINI_KEY));
    }
}

/// A Gemini key no header may carry is refused while the request is built:
/// it never reaches the transport, and the refusal does not repeat the key.
#[tokio::test]
async fn a_gemini_key_that_cannot_be_a_header_is_never_sent() {
    let key = format!("{GEMINI_KEY}\n");
    let value = settings(InferenceApi::Gemini);
    let route = routed(&value, 100).unwrap();
    let capture = Capture::default();
    let model = route.model("gemini-next", &key, capture.clone());

    let error = model
        .call(CompletionRequest::new(Message::user("hello")))
        .await
        .unwrap_err();

    assert!(capture.0.lock().unwrap().is_empty(), "nothing was sent");
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains(GEMINI_KEY));
    }
}
