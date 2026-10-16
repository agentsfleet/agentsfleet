//! Exercise the real SDK codecs across the single physical-send boundary.

#![expect(
    clippy::expect_used,
    reason = "fixture construction and a refused request must fail the test if their outcome changes"
)]

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
        Ok(ProviderResponse {
            status: self.status,
            body: self.body.clone(),
        })
    }
}

fn outbound(api: InferenceApi) -> Outbound {
    Outbound {
        key: SecretString::new("private-key".into()),
        body: CompletionRequest::new(Message::user("private-prompt")).max_tokens(37),
        inference: ModelInference {
            api,
            base_url: "https://models.example.com/v1".into(),
            dialect: None,
            max_output_tokens: 37,
        },
        model: "workspace/model-next".into(),
        context_tokens: 128_000,
        timeout_ms: 1234,
    }
}

fn reply(api: InferenceApi) -> Value {
    match api {
        InferenceApi::Chat => {
            json!({"choices":[{(MESSAGE):{(ROLE):ASSISTANT,(CONTENT):ANSWER},"finish_reason":"stop"}],(USAGE):{"prompt_tokens":3,"completion_tokens":2}})
        }
        InferenceApi::Responses => {
            json!({"status":"completed","output":[{(TYPE):MESSAGE,(CONTENT):[{(TYPE):"output_text",(TEXT):ANSWER}]}],(USAGE):{(INPUT_TOKENS):3,(OUTPUT_TOKENS):2}})
        }
        InferenceApi::Messages => {
            json!({(TYPE):MESSAGE,(ROLE):ASSISTANT,(CONTENT):[{(TYPE):TEXT,(TEXT):ANSWER}],"stop_reason":"end_turn",(USAGE):{(INPUT_TOKENS):3,(OUTPUT_TOKENS):2}})
        }
    }
}

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
        let (status, response) = complete(outbound(api), Arc::<Physical>::clone(&physical))
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
        assert_eq!(sent.timeout_ms, 1234);
        assert!(sent.request.headers().contains_key(header));
        assert!(!format!("{sent:?}").contains("private-"));
        let body: Value = serde_json::from_slice(sent.body()).expect("SDK request");
        assert_eq!(body.get("model"), Some(&json!("workspace/model-next")));
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
        let error = complete(
            outbound(InferenceApi::Chat),
            Arc::<Physical>::clone(&physical),
        )
        .await
        .expect_err("failed response");
        assert!(!error.to_string().contains("private-"));
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
    let (status, response) = complete(outbound(api), Arc::<Physical>::clone(&physical))
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
