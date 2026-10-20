//! Authentication evidence cannot swallow usage or turn a network loss into zero.

#![expect(clippy::expect_used, reason = "fixture failures must fail the test")]

use super::{Outbound, ProviderRequest, ProviderResponse, Transport, complete};
use crate::{Result, error::ErrorKind};
use afd_core::error_code;
use afd_crypto::secret::SecretString;
use afd_wire::inference::{InferenceApi, ModelInference};
use rig_core::{completion::CompletionRequest, message::Message};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
struct Physical {
    status: u16,
    body: Vec<u8>,
    count: AtomicUsize,
    fail: bool,
}

#[async_trait::async_trait]
impl Transport for Physical {
    async fn send(&self, _request: ProviderRequest) -> Result<ProviderResponse> {
        self.count.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            return Err(ErrorKind::Unknown.into());
        }
        Ok(ProviderResponse {
            status: self.status,
            body: self.body.clone(),
        })
    }
}

fn outbound(api: InferenceApi) -> Outbound {
    let key = SecretString::new("fixture".into());
    Outbound {
        key,
        body: CompletionRequest::new(Message::user("private prompt")).max_tokens(16),
        inference: ModelInference {
            api,
            base_url: "https://models.example.com/v1".into(),
            dialect: None,
            max_output_tokens: 16,
        },
        model: "provider/model".into(),
        context_tokens: 128_000,
        timeout: Duration::from_secs(1),
    }
}

fn physical(status: u16, document: &Value) -> Arc<Physical> {
    Arc::new(Physical {
        status,
        body: serde_json::to_vec(document).expect("JSON"),
        count: AtomicUsize::new(0),
        fail: false,
    })
}

// Value: protects=all supported APIs retain an explicit authentication refusal;
// fails_when=an SDK decoding error erases HTTP rejection; seam=physical transport.
#[tokio::test]
async fn authentication_rejection_is_preserved_without_retry_or_private_diagnostics() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
        InferenceApi::Ollama,
    ] {
        let boundary = physical(
            401,
            &json!({"error":{"code":401,"message":"private rejection"}}),
        );
        let fault = complete(outbound(api), Arc::clone(&boundary) as Arc<dyn Transport>)
            .await
            .expect_err("provider rejected credentials");
        assert_eq!(fault.code(), error_code::AUTHORING_MODEL, "{api:?}");
        assert_eq!(boundary.count.load(Ordering::Relaxed), 1);
        assert!(!fault.to_string().contains("private"));
    }
}

// Value: protects=ambiguous errors never claim an authentication refusal;
// fails_when=arbitrary failed or successful HTTP replies invent known zero usage.
#[tokio::test]
async fn ambiguous_errors_and_http_success_are_not_authentication_rejections() {
    for (status, document) in [
        (400, json!({"error":{}})),
        (403, json!({"error":{}})),
        (429, json!({"error":{}})),
        (503, json!({"error":{}})),
        (200, json!({"error":{"code":401}})),
        (401, json!({})),
        (401, json!({"error":"unstructured"})),
        (401, json!({"error":{},"usage":null})),
        (401, json!({"error":{},"choices":[]})),
    ] {
        let boundary = physical(status, &document);
        let result = complete(
            outbound(InferenceApi::Responses),
            Arc::clone(&boundary) as Arc<dyn Transport>,
        )
        .await;
        assert!(
            !result.is_err_and(|error| error.code() == error_code::AUTHORING_MODEL),
            "{status}: {document}"
        );
        assert_eq!(boundary.count.load(Ordering::Relaxed), 1);
    }
}

#[tokio::test]
async fn authentication_status_does_not_erase_reported_usage() {
    let boundary = physical(
        401,
        &json!({"status":"failed","error":{"code":"invalid_key","message":"rejected"},
        "output":[], "usage":{"input_tokens":3,"output_tokens":2}}),
    );
    let (status, response) = complete(
        outbound(InferenceApi::Responses),
        Arc::clone(&boundary) as Arc<dyn Transport>,
    )
    .await
    .expect("usage remains authoritative");
    assert_eq!(status, 401);
    assert_eq!(response.usage.input_tokens, Some(3));
    assert_eq!(response.usage.output_tokens, Some(2));
    assert_eq!(boundary.count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn a_lost_or_malformed_authentication_reply_remains_uncertain() {
    for fail in [false, true] {
        let boundary = Arc::new(Physical {
            status: 401,
            body: b"truncated reply".to_vec(),
            count: AtomicUsize::new(0),
            fail,
        });
        let fault = complete(
            outbound(InferenceApi::Chat),
            Arc::clone(&boundary) as Arc<dyn Transport>,
        )
        .await
        .expect_err("no authoritative rejection document");
        assert_ne!(fault.code(), error_code::AUTHORING_MODEL);
        assert_eq!(boundary.count.load(Ordering::Relaxed), 1);
    }
}
