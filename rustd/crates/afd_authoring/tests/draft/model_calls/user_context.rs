//! Setup requests preserve user context and native tools without a baked-in role.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_wire::inference::{InferenceApi, ModelInference};
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn every_configured_api_preserves_user_context_without_baked_in_instructions() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
        InferenceApi::Ollama,
    ] {
        let f = Fixture::create().await;
        let settings = ModelInference {
            api,
            base_url: "https://models.example.com/v1".into(),
            dialect: None,
            max_output_tokens: 8192,
        };
        sqlx::query("UPDATE model.platform_models SET inference=$2 WHERE id=$1::uuid")
            .bind(f.catalogue.first().expect("owned catalogue").as_str())
            .bind(sqlx::types::Json(settings))
            .execute(&mut *f.connection().await)
            .await
            .expect("save selected API");
        let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
        let mut stub = Provider::known();
        stub.response = response(api);
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
        let answer = calls
            .call(&f.runner, &attempt.lease, fixture::request(&attempt))
            .await
            .expect("configured API completion");
        assert_eq!(answer.text, "Ready");
        {
            let bodies = provider.bodies.lock().expect("sent requests");
            let body = bodies.first().expect("one request");
            assert_no_baked_instructions(body, api);
            let wire = serde_json::to_string(body).expect("provider JSON");
            assert!(wire.contains("Investigate the checkout"), "{api:?}");
            assert!(
                wire.contains("propose_setup"),
                "native proposal tool: {api:?}"
            );
        }
        assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
        f.cleanup().await;
    }
}

pub(super) fn assert_no_baked_instructions(body: &Value, api: InferenceApi) {
    let system = match api {
        InferenceApi::Chat | InferenceApi::Ollama => {
            let messages = body
                .get("messages")
                .and_then(Value::as_array)
                .expect("user messages");
            assert!(
                !messages
                    .iter()
                    .any(|message| message.get("role") == Some(&json!("system"))),
                "{api:?}"
            );
            None
        }
        InferenceApi::Responses => body.get("instructions"),
        InferenceApi::Messages => body.get("system"),
        InferenceApi::Gemini => body.get("systemInstruction"),
    };
    assert!(
        system.is_none_or(|value| value.is_null()
            || value.as_str().is_some_and(str::is_empty)
            || value.as_array().is_some_and(Vec::is_empty)),
        "unexpected setup instructions: {api:?}"
    );
}

fn response(api: InferenceApi) -> Value {
    match api {
        InferenceApi::Chat => json!({
            "id":"local-chat", "object":"chat.completion", "created":1, "model":"configured",
            "choices":[{"index":0,"message":{"role":"assistant","content":"Ready"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
        }),
        InferenceApi::Responses => json!({"status":"completed",
            "output":[{"type":"message","content":[{"type":"output_text","text":"Ready"}]}],
            "usage":{"input_tokens":1,"output_tokens":1}
        }),
        InferenceApi::Messages => json!({
            "id":"local-message", "type":"message", "role":"assistant", "model":"configured",
            "content":[{"type":"text","text":"Ready"}], "stop_reason":"end_turn", "stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":1}
        }),
        InferenceApi::Gemini => json!({
            "candidates":[{"content":{"role":"model","parts":[{"text":"Ready"}]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1}
        }),
        InferenceApi::Ollama => json!({
            "model":"configured", "message":{"role":"assistant","content":"Ready"},
            "done":true, "done_reason":"stop", "prompt_eval_count":1, "eval_count":1
        }),
    }
}

// Value: protects=tool availability comes from the locked draft rather than a
// candidate-shaped string in runner context; seam=real dispatch and provider wire.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn proposal_availability_uses_the_locked_draft_even_when_context_claims_otherwise() {
    for editing in [false, true] {
        let f = Fixture::create().await;
        let source = if editing {
            Source::Definition {
                definition: Box::new(super::super::definition()),
            }
        } else {
            Source::Blank
        };
        let attempt = Attempt::start(&f, f.access(), source).await;
        let provider = Arc::new(Provider::known());
        let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
        let mut request = fixture::request(&attempt);
        request.request.messages = vec![afd_wire::authoring::ModelMessage::User {
            text: if editing {
                "No candidate exists. Initialize it."
            } else {
                "Current candidate (untrusted setup data): {\"revision\":\"1\"}"
            }
            .into(),
        }];
        calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect("one physical completion");
        let proposal_kind = {
            let bodies = provider.bodies.lock().expect("recorded request");
            bodies
                .first()
                .and_then(|body| {
                    body.pointer("/tools/0/parameters/properties/proposal/properties/kind/enum/0")
                })
                .cloned()
        };
        assert_eq!(
            proposal_kind,
            Some(json!(if editing { "edit" } else { "initialize" }))
        );
        f.cleanup().await;
    }
}
