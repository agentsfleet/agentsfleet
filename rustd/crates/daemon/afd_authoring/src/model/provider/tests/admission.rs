//! A count for an earlier body never authorizes a changed physical request.

use super::{Physical, complete, outbound, reply};
use af_core::error_code;
use af_wire::inference::InferenceApi;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn changed_body_after_measurement_is_refused_before_the_physical_send() {
    let api = InferenceApi::Responses;
    let mut request = outbound(api);
    request.measurement = Some(
        af_inference::Configured::new(&request.inference, request.context_tokens)
            .expect("saved inference route")
            .input_count(&request.model, &request.body)
            .expect("the original complete request is measured"),
    );
    request.body = request.body.preamble("changed after measurement");
    let physical = Arc::new(Physical {
        requests: Mutex::new(Vec::new()),
        status: 200,
        body: serde_json::to_vec(&reply(api)).expect("provider response"),
        fail: false,
    });
    let error = complete(request, Arc::<Physical>::clone(&physical))
        .await
        .expect_err("a stale measurement cannot authorize a physical effect");
    assert_eq!(error.code(), error_code::AUTHORING_UNKNOWN);
    assert!(
        std::error::Error::source(&error)
            .is_some_and(<dyn std::error::Error>::is::<rig_core::ProviderError>)
    );
    assert!(physical.requests.lock().expect("physical sends").is_empty());
}

#[derive(Debug)]
struct Silent;
#[async_trait::async_trait]
impl super::Transport for Silent {
    async fn send(
        &self,
        _request: super::ProviderRequest,
    ) -> super::Result<super::ProviderResponse> {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        Ok(super::ProviderResponse {
            status: 200,
            headers,
            body: Box::pin(futures_util::stream::pending()),
        })
    }
}

#[tokio::test(start_paused = true)]
async fn silent_provider_respects_the_deadline_and_caller_cancellation() {
    let mut bounded = outbound(InferenceApi::Responses);
    bounded.timeout = std::time::Duration::from_millis(10);
    let (status, _partial, cause) = complete(bounded, Arc::new(Silent))
        .await
        .expect("partial failure is retained");
    assert_eq!(status, 502);
    assert_eq!(
        cause.map(crate::types::FailureCause::as_str),
        Some("deadline")
    );
    let cancelled = tokio::time::timeout(
        std::time::Duration::from_millis(5),
        complete(outbound(InferenceApi::Responses), Arc::new(Silent)),
    )
    .await;
    cancelled.expect_err("caller cancellation must stop the physical response");
}
