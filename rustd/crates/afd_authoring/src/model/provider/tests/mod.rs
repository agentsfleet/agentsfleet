//! Exercise the real SDK codecs across the single physical-send boundary.

#![expect(
    clippy::expect_used,
    reason = "fixture construction and a refused request must fail the test if their outcome changes"
)]

mod rejections;
#[path = "../../../../tests/provider_wire.rs"]
mod wire;

use super::*;
use afd_crypto::secret::SecretString;
use afd_wire::inference::{InferenceApi, ModelInference};
use rig_core::{completion::CompletionRequest, message::Message};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const ROLE: &str = "role";
const OUTPUT_TOKENS: &str = "output_tokens";
const ASSISTANT: &str = "assistant";
const CONTENT: &str = "content";
const ANSWER: &str = "answer";
const MESSAGE: &str = "message";
const INPUT_TOKENS: &str = "input_tokens";
const USAGE: &str = "usage";
const TEXT: &str = "text";
const TYPE: &str = "type";
const MODEL: &str = "workspace/model-next";
const MODEL_KEY: &str = "model";
const STOP: &str = "stop";

#[derive(Debug)]
struct Physical {
    requests: Mutex<Vec<ProviderRequest>>,
    status: u16,
    body: Vec<u8>,
    fail: bool,
}

#[async_trait::async_trait]
impl Transport for Physical {
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse> {
        self.requests.lock().expect("record request").push(request);
        if self.fail {
            return Err(crate::error::ErrorKind::Unknown.into());
        }
        let (headers, body) = wire::body(self.status, self.body.clone());
        Ok(ProviderResponse {
            status: self.status,
            headers,
            body,
        })
    }
}

fn outbound(api: InferenceApi) -> Outbound {
    Outbound {
        dispatch_id: "fixture-dispatch".into(),
        progress: None,
        measurement: None,
        key: SecretString::new("private-key".into()),
        body: CompletionRequest::new(Message::user("private-prompt")).max_tokens(37),
        inference: ModelInference {
            api,
            base_url: "https://models.example.com/v1".into(),
            dialect: None,
            max_output_tokens: 37,
        },
        model: MODEL.into(),
        context_tokens: 128_000,
        timeout: Duration::from_millis(1234),
    }
}

fn reply(api: InferenceApi) -> Value {
    match api {
        InferenceApi::Chat => {
            json!({"choices":[{(MESSAGE):{(ROLE):ASSISTANT,(CONTENT):ANSWER},"finish_reason":STOP}],(USAGE):{"prompt_tokens":3,"completion_tokens":2}})
        }
        InferenceApi::Responses => {
            json!({"status":"completed","output":[{(TYPE):MESSAGE,(CONTENT):[{(TYPE):"output_text",(TEXT):ANSWER}]}],(USAGE):{(INPUT_TOKENS):3,(OUTPUT_TOKENS):2}})
        }
        InferenceApi::Messages => {
            json!({(TYPE):MESSAGE,(ROLE):ASSISTANT,(CONTENT):[{(TYPE):TEXT,(TEXT):ANSWER}],"stop_reason":"end_turn",(USAGE):{(INPUT_TOKENS):3,(OUTPUT_TOKENS):2}})
        }
        InferenceApi::Gemini => {
            json!({"candidates":[{(CONTENT):{(ROLE):MODEL_KEY,"parts":[{(TEXT):ANSWER}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2}})
        }
        InferenceApi::Ollama => {
            json!({(MODEL_KEY):MODEL,(MESSAGE):{(ROLE):ASSISTANT,(CONTENT):ANSWER},"done":true,"done_reason":STOP,"prompt_eval_count":3,"eval_count":2})
        }
    }
}

// Value: protects=the native Gemini and Ollama APIs dispatch through the same
// saved-settings route as the other three, one physical send each, with the
// credential where each provider's SDK wire places it and never in diagnostics;
// fails_when=a native API falls back to another wire, retries, drops usage or
// leaks the key into Debug; why_new=§8 adds both APIs to InferenceApi;
// seam=none; no_claim=live providers.
#[tokio::test]
async fn the_native_apis_send_once_and_keep_the_key_out_of_diagnostics() {
    // rig's GenerateContent wire puts the key in a `key` query parameter;
    // afd_inference moves it to `x-goog-api-key` before any transport sees the
    // request, so the daemon's dispatch path sends it as a header too. Ollama's
    // native chat sends a bearer header.
    for (api, path, header) in [
        (
            InferenceApi::Gemini,
            "/v1/v1beta/models/workspace/model-next:streamGenerateContent",
            "x-goog-api-key",
        ),
        (InferenceApi::Ollama, "/v1/api/chat", "authorization"),
    ] {
        let physical = Arc::new(Physical {
            requests: Mutex::new(Vec::new()),
            status: 200,
            body: serde_json::to_vec(&reply(api)).expect("response JSON"),
            fail: false,
        });
        let (status, response, _) = complete(outbound(api), Arc::<Physical>::clone(&physical))
            .await
            .expect("SDK response");
        assert_eq!(status, 200);
        assert_eq!(response.usage.input_tokens, Some(3), "{api:?}");
        assert_eq!(response.usage.output_tokens, Some(2), "{api:?}");
        let requests = physical.requests.lock().expect("recorded requests");
        assert_eq!(requests.len(), 1, "{api:?}");
        let sent = requests.first().expect("one physical request");
        assert_eq!(sent.request.uri().path(), path, "{api:?}");
        assert!(sent.request.headers().contains_key(header), "{api:?}");
        assert!(
            !sent.request.uri().to_string().contains("private-"),
            "{api:?}: the key never rides the URL"
        );
        assert!(!format!("{sent:?}").contains("private-"), "{api:?}");
        let body: Value = serde_json::from_slice(sent.body()).expect("SDK request");
        assert!(body.to_string().contains("private-prompt"), "{api:?}");
    }
}

// Value: protects=provider diagnostics hide private fields while the physical request retains its prompt;
// fails_when=Debug leaks request fields, hides the timeout or redaction removes the provider prompt;
// why_new=the existing boundary test checks secret absence but not prompt delivery or useful diagnostics;
// seam=none; no_claim=live provider acceptance (existing physical-send fixture).
#[tokio::test]
async fn every_api_uses_the_configured_model_credential_limit_and_one_physical_send() {
    for (api, path, header) in [
        (InferenceApi::Chat, "/v1/chat/completions", "authorization"),
        (InferenceApi::Responses, "/v1/responses", "authorization"),
        (InferenceApi::Messages, "/v1/messages", "x-api-key"),
    ] {
        let physical = Arc::new(Physical {
            requests: Mutex::new(Vec::new()),
            status: 200,
            body: serde_json::to_vec(&reply(api)).expect("response JSON"),
            fail: false,
        });
        let (status, response, _) = complete(outbound(api), Arc::<Physical>::clone(&physical))
            .await
            .expect("SDK response");
        assert_eq!(status, 200);
        assert_eq!(response.usage.input_tokens, Some(3));
        assert_eq!(response.usage.output_tokens, Some(2));
        assert!(
            matches!(response.choice.first(), Some(rig_core::message::AssistantContent::Text(part)) if part.text == ANSWER)
        );
        let requests = physical.requests.lock().expect("recorded requests");
        assert_eq!(requests.len(), 1);
        let sent = requests.first().expect("one physical request");
        assert_eq!(sent.request.uri().path(), path);
        assert_eq!(sent.timeout, Duration::from_millis(1234));
        assert!(sent.request.headers().contains_key(header));
        let diagnostic = format!("{sent:?}");
        assert!(!diagnostic.contains("private-") && !diagnostic.contains("models.example.com"));
        assert!(diagnostic.contains("1.234s"), "{diagnostic}");
        let body: Value = serde_json::from_slice(sent.body()).expect("SDK request");
        assert!(body.to_string().contains("private-prompt"));
        assert_eq!(body.get(MODEL_KEY), Some(&json!(MODEL)));
        assert_eq!(
            body.get("max_tokens")
                .or_else(|| body.get("max_output_tokens")),
            Some(&json!(37))
        );
    }
}

#[tokio::test]
async fn physical_failure_invalid_json_status_and_oversize_never_retry() {
    for (status, body, fail) in [
        (200, b"not JSON".to_vec(), false),
        (99, b"{}".to_vec(), false),
        (
            200,
            vec![b' '; afd_wire::authoring::ENVELOPE_BYTES_MAX + 1],
            false,
        ),
        (200, b"{}".to_vec(), true),
    ] {
        let physical = Arc::new(Physical {
            requests: Mutex::new(Vec::new()),
            status,
            body,
            fail,
        });
        let (_, response, cause) = complete(
            outbound(InferenceApi::Chat),
            Arc::<Physical>::clone(&physical),
        )
        .await
        .expect("uncertainty retains partial facts");
        assert!(cause.is_some());
        assert!(response.usage.input_tokens.is_none());
        assert_eq!(
            physical.requests.lock().expect("recorded requests").len(),
            1
        );
    }
}

#[tokio::test]
async fn failed_http_status_keeps_reported_usage_for_accounting() {
    let api = InferenceApi::Responses;
    let physical = Arc::new(Physical {
        requests: Mutex::new(Vec::new()),
        status: 503,
        body: serde_json::to_vec(&reply(api)).expect("JSON"),
        fail: false,
    });
    let (status, response, _) = complete(outbound(api), Arc::<Physical>::clone(&physical))
        .await
        .expect("known reply");
    assert_eq!(status, 503);
    assert_eq!(response.usage.input_tokens, Some(3));
    assert_eq!(response.usage.output_tokens, Some(2));
    assert_eq!(
        physical.requests.lock().expect("recorded requests").len(),
        1
    );
}

// Value: protects=successful and failed provider bodies cannot leak through boundary response diagnostics;
// fails_when=ProviderResponse Debug includes body bytes or safe formatting changes the physical response;
// why_new=existing request privacy tests never format a ProviderResponse;
// seam=none; no_claim=SDK response decoding or live provider acceptance (boundary value only).
#[test]
fn provider_response_diagnostics_keep_status_without_private_reply_bytes() {
    for status in [200, 401, 503] {
        let body = serde_json::to_vec(&json!({
            "text": "private-provider-answer",
            "reasoning": "private-provider-reasoning",
            "credential": "private-provider-credential"
        }))
        .expect("private response JSON");
        let (headers, stream) = wire::body(status, body.clone());
        let response = ProviderResponse {
            status,
            headers,
            body: stream,
        };
        let exposed_bytes = format!("{body:?}").replace(' ', "");
        for rendered in [format!("{response:?}"), format!("{response:#?}")] {
            assert!(
                !rendered.contains("private-"),
                "private response leaked: {rendered}"
            );
            assert!(
                !rendered.contains("credential"),
                "private field leaked: {rendered}"
            );
            let compact: String = rendered.chars().filter(|ch| !ch.is_whitespace()).collect();
            assert!(
                !compact.contains(exposed_bytes.trim_end_matches(']')),
                "private response bytes leaked: {rendered}"
            );
            assert!(
                rendered.contains(&status.to_string()),
                "status remains readable: {rendered}"
            );
        }
        assert_eq!(response.status, status);
    }
}
