//! A count for an earlier body never authorizes a changed physical request.

use super::{Physical, complete, outbound, reply};
use afd_core::error_code;
use afd_wire::inference::InferenceApi;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn changed_body_after_measurement_is_refused_before_the_physical_send() {
    let api = InferenceApi::Responses;
    let mut request = outbound(api);
    request.measurement = Some(
        afd_inference::Configured::new(&request.inference, request.context_tokens)
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
