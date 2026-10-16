#![expect(clippy::unwrap_used, reason = "fixtures fail loudly on invalid setup")]

use super::*;
use bytes::Bytes;
use rig_core::http_client::{
    self as http, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use rig_core::message::Message;
use rig_core::wasm_compat::WasmCompatSend;
use std::{
    error::Error as _,
    sync::{Arc, Mutex},
};

const UNUSED_OPERATION: &str = "unused transport operation";

fn settings(api: InferenceApi) -> ModelInference {
    ModelInference {
        api,
        base_url: "https://inference.example/custom/v1".into(),
        dialect: None,
        max_output_tokens: 37,
    }
}

#[test]
fn rejects_unsafe_endpoints_without_exposing_them() {
    for endpoint in [
        "not a url",
        "http://example.com",
        "file:///etc/passwd",
        "https://user:secret@example.com",
        "https://example.com?secret=value",
        "https://example.com#secret",
        "https://127.0.0.1",
        "https://169.254.169.254",
        "https://[::1]",
        "https://[::ffff:10.0.0.1]",
    ] {
        let mut value = settings(InferenceApi::Chat);
        value.base_url = endpoint.into();
        let error = Configured::new(&value, 100).unwrap_err();
        assert_eq!(error.code(), afd_core::error_code::INVALID_REQUEST);
        assert!(!error.to_string().contains("secret"));
    }
    for endpoint in ["https://8.8.8.8", "https://[2606:4700:4700::1111]"] {
        let mut value = settings(InferenceApi::Chat);
        value.base_url = endpoint.into();
        let route = Configured::new(&value, 100).unwrap();
        assert_eq!(
            route.into_host().as_ref(),
            endpoint.trim_start_matches("https://")
        );
    }
}

#[test]
fn enforces_declared_bounds_and_model_specific_output_limit() {
    let mut value = settings(InferenceApi::Chat);
    Configured::new(&value, 37).unwrap();
    assert_eq!(
        Configured::new(&value, 36).unwrap_err().code(),
        afd_core::error_code::INVALID_REQUEST
    );
    value.max_output_tokens = 0;
    let error = Configured::new(&value, 100).unwrap_err();
    assert!(error.source().is_some());
    value.max_output_tokens = 37;
    value.base_url = "x".repeat(2049);
    assert_eq!(
        Configured::new(&value, 100).unwrap_err().code(),
        afd_core::error_code::INVALID_REQUEST
    );
    value.base_url = String::new();
    assert_eq!(
        Configured::new(&value, 100).unwrap_err().code(),
        afd_core::error_code::INVALID_REQUEST
    );
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
            assert_eq!(
                Configured::new(&value, 100).unwrap_err().code(),
                afd_core::error_code::INVALID_REQUEST
            );
        }
    }
}

#[test]
fn a_credential_endpoint_restriction_cannot_be_replaced_by_catalogue_routing() {
    let value = settings(InferenceApi::Chat);
    let route = Configured::new(&value, 100).unwrap();
    assert!(route.accepts_endpoint(None));
    assert!(route.accepts_endpoint(Some(&value.base_url)));
    assert!(route.accepts_endpoint(Some("https://inference.example/custom/v1/")));
    assert!(!route.accepts_endpoint(Some("https://another.example/v1")));
    assert!(!route.accepts_endpoint(Some("")));
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<Request<Bytes>>>>);

impl HttpClientExt for Capture {
    fn send<T, U>(
        &self,
        req: Request<T>,
    ) -> impl Future<Output = http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        T: Into<Bytes> + WasmCompatSend,
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        self.0.lock().unwrap().push(req.map(Into::into));
        async {
            Response::builder()
                .status(400)
                .body(Box::pin(async { Ok(U::from(Bytes::new())) }) as LazyBody<U>)
                .map_err(http::Error::Protocol)
        }
    }

    fn send_multipart<U>(
        &self,
        _: Request<MultipartForm>,
    ) -> impl Future<Output = http::Result<Response<LazyBody<U>>>> + WasmCompatSend + 'static
    where
        U: From<Bytes> + WasmCompatSend + 'static,
    {
        std::future::ready(Err(http::Error::instance(std::io::Error::other(
            UNUSED_OPERATION,
        ))))
    }

    fn send_streaming<T>(
        &self,
        _: Request<T>,
    ) -> impl Future<Output = http::Result<StreamingResponse>> + WasmCompatSend
    where
        T: Into<Bytes> + WasmCompatSend,
    {
        std::future::ready(Err(http::Error::instance(std::io::Error::other(
            UNUSED_OPERATION,
        ))))
    }
}

const AUTHORIZATION_HEADER: &str = "authorization";
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
        let route = Configured::new(&value, 100).unwrap();
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
