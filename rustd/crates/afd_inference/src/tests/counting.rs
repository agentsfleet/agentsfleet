//! Counting crosses the physical HTTP seam without generating a model response.
use super::*;
use crate::{
    stream::TurnControl,
    transport::{HttpTransport, SendRequest},
};
use futures_util::{future::BoxFuture, stream};

mod payloads;
mod unsupported;

const LARGE_WINDOW: u32 = 1_000_000;
const REPLY_CAP: usize = 16 * 1024;

struct Physical {
    requests: Mutex<Vec<Request<Bytes>>>,
    status: u16,
    body: Option<Bytes>,
    fail: bool,
}

impl SendRequest for Physical {
    fn send(&self, request: Request<Bytes>) -> BoxFuture<'static, http::Result<StreamingResponse>> {
        self.requests.lock().unwrap().push(request);
        let body = self.body.clone();
        let status = self.status;
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                return Err(http::Error::instance(std::io::Error::other(
                    "count disconnected",
                )));
            }
            let body: http::BoxedStream = body.map_or_else(
                || Box::pin(stream::pending()) as http::BoxedStream,
                |body| Box::pin(stream::once(async { Ok(body) })) as http::BoxedStream,
            );
            Ok(Response::builder().status(status).body(body).unwrap())
        })
    }
}

fn physical(status: u16, body: Option<&str>, fail: bool) -> Arc<Physical> {
    Arc::new(Physical {
        requests: Mutex::default(),
        status,
        body: body.map(|body| Bytes::copy_from_slice(body.as_bytes())),
        fail,
    })
}

fn native(api: InferenceApi) -> ModelInference {
    let mut settings = settings(api);
    settings.base_url = match api {
        InferenceApi::Responses => "https://api.openai.com/v1",
        InferenceApi::Messages => "https://api.anthropic.com/v1",
        InferenceApi::Gemini => "https://generativelanguage.googleapis.com",
        _ => "https://gateway.example/v1",
    }
    .into();
    settings
}

fn body() -> CompletionRequest {
    CompletionRequest::new(Message::user("private source 🌿".repeat(32)))
        .preamble("private pinned instructions")
        .max_tokens(37)
}

#[tokio::test]
async fn native_count_requires_the_same_prepared_request_and_authorized_route() {
    for (api, path, field, key_header) in [
        (
            InferenceApi::Responses,
            "/v1/responses/input_tokens",
            "input_tokens",
            AUTHORIZATION_HEADER,
        ),
        (
            InferenceApi::Messages,
            "/v1/messages/count_tokens",
            "input_tokens",
            API_KEY_HEADER,
        ),
        (
            InferenceApi::Gemini,
            "/v1beta/models/model-next:countTokens",
            "totalTokens",
            "x-goog-api-key",
        ),
    ] {
        let settings = native(api);
        let route = Configured::new(&settings, 512).unwrap();
        let reply = serde_json::json!({field:123}).to_string();
        let physical = physical(200, Some(&reply), false);
        let transport = HttpTransport::new(Arc::<Physical>::clone(&physical));
        let mut count = route
            .confirm_input(
                "model-next",
                "private-credential",
                &body(),
                &transport,
                &TurnControl::default(),
            )
            .await
            .unwrap();
        assert_eq!(count.tokens(), 123);
        assert_eq!(count.method(), "provider_count");
        let bounded = route
            .bound_counted_output("model-next", body(), &mut count)
            .unwrap();
        assert_eq!(bounded.max_tokens, Some(37));
        let requests = physical.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let sent = requests.first().unwrap();
        assert_eq!(sent.uri().path(), path);
        assert_eq!(sent.uri().query(), None);
        assert!(sent.headers().contains_key(key_header));
        assert!(!format!("{sent:?}{count:?}").contains("private-credential"));
        let encoded: serde_json::Value = serde_json::from_slice(sent.body()).unwrap();
        let text = encoded.to_string();
        assert!(text.contains("private pinned instructions"));
        assert!(text.contains("private source"));
        assert!(encoded.get("max_tokens").is_none());
        assert!(encoded.get("max_output_tokens").is_none());
    }
}

#[tokio::test]
async fn ordinary_turns_and_other_provider_routes_never_send_count_requests() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
        InferenceApi::Ollama,
    ] {
        let settings = settings(api);
        let route = Configured::new(&settings, 512).unwrap();
        let physical = physical(200, Some("{}"), false);
        let transport = HttpTransport::new(Arc::<Physical>::clone(&physical));
        let count = route
            .confirm_input(
                "model-next",
                "private-credential",
                &body(),
                &transport,
                &TurnControl::default(),
            )
            .await
            .unwrap();
        assert_eq!(count.method(), "utf8_estimate");
        assert!(physical.requests.lock().unwrap().is_empty());
    }
    let settings = native(InferenceApi::Responses);
    let route = Configured::new(&settings, LARGE_WINDOW).unwrap();
    let physical = physical(200, Some("{}"), false);
    let count = route
        .confirm_input(
            "model-next",
            "private-credential",
            &body(),
            &HttpTransport::new(Arc::<Physical>::clone(&physical)),
            &TurnControl::default(),
        )
        .await
        .unwrap();
    assert_eq!(count.method(), "utf8_estimate");
    assert!(physical.requests.lock().unwrap().is_empty());
}

#[test]
fn request_route_model_window_and_output_changes_invalidate_counts() {
    let settings = native(InferenceApi::Responses);
    let route = Configured::new(&settings, LARGE_WINDOW).unwrap();
    let mut count = route.input_count("model-next", &body()).unwrap();
    for (model, request) in [
        ("another-model", body()),
        ("model-next", body().preamble("changed pinned state")),
        ("model-next", body().max_tokens(36)),
        (
            "model-next",
            CompletionRequest::new(Message::user("different private source")),
        ),
    ] {
        let error = route
            .bound_counted_output(model, request, &mut count)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("different prepared request or route")
        );
    }
    let other = Configured::new(&settings, 999_999).unwrap();
    let error = other
        .bound_counted_output("model-next", body(), &mut count)
        .unwrap_err();
    assert!(matches!(error.kind(), crate::error::ErrorKind::CountStale));
    let other_settings = super::settings(InferenceApi::Responses);
    let other = Configured::new(&other_settings, LARGE_WINDOW).unwrap();
    let error = other
        .bound_counted_output("model-next", body(), &mut count)
        .unwrap_err();
    assert!(matches!(error.kind(), crate::error::ErrorKind::CountStale));
}

#[tokio::test]
async fn malformed_missing_zero_and_failed_counts_never_become_zero_or_retry() {
    let settings = native(InferenceApi::Responses);
    let route = Configured::new(&settings, 512).unwrap();
    for (status, body, fail) in [
        (200, "{}", false),
        (200, "{", false),
        (200, "{\"input_tokens\":0}", false),
        (200, "{\"input_tokens\":-1}", false),
        (200, "{\"input_tokens\":1.5}", false),
        (200, "{\"input_tokens\":\"123\"}", false),
        (401, "private provider refusal", false),
        (429, "private provider refusal", false),
        (500, "private provider refusal", false),
        (200, "{}", true),
    ] {
        let physical = physical(status, Some(body), fail);
        let error = route
            .confirm_input(
                "model-next",
                "private-credential",
                &self::body(),
                &HttpTransport::new(Arc::<Physical>::clone(&physical)),
                &TurnControl::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(physical.requests.lock().unwrap().len(), 1);
        assert!(!format!("{error:?}{error}").contains("private provider refusal"));
        assert_eq!(
            error.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
    }
}

#[tokio::test(start_paused = true)]
async fn count_deadline_and_cancellation_stop_admission_without_resending() {
    let settings = native(InferenceApi::Responses);
    let route = Configured::new(&settings, 512).unwrap();
    let physical = physical(200, None, false);
    let transport = HttpTransport::new(Arc::<Physical>::clone(&physical));
    let error = route
        .confirm_input(
            "model-next",
            "private-credential",
            &body(),
            &transport,
            &TurnControl::default(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("deadline"));
    assert_eq!(physical.requests.lock().unwrap().len(), 1);
    let control = TurnControl::default();
    control.cancellation.cancel();
    let error = route
        .confirm_input(
            "model-next",
            "private-credential",
            &body(),
            &transport,
            &control,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("cancelled"));
    assert_eq!(physical.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_counts_and_exhausted_native_input_refuse_output() {
    let settings = native(InferenceApi::Responses);
    let route = Configured::new(&settings, 512).unwrap();
    let huge = " ".repeat(REPLY_CAP + 1);
    let physical = physical(200, Some(&huge), false);
    let error = route
        .confirm_input(
            "model-next",
            "private-credential",
            &body(),
            &HttpTransport::new(physical),
            &TurnControl::default(),
        )
        .await
        .unwrap_err();
    assert!(error.is_oversize());
    let physical = self::physical(200, Some("{\"input_tokens\":512}"), false);
    let mut count = route
        .confirm_input(
            "model-next",
            "private-credential",
            &body(),
            &HttpTransport::new(physical),
            &TurnControl::default(),
        )
        .await
        .unwrap();
    assert!(
        route
            .bound_counted_output("model-next", body(), &mut count)
            .unwrap_err()
            .to_string()
            .contains("exhausts")
    );
}
