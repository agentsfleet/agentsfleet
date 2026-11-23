//! A decision request travels whole: no field is cut to fit a byte limit.

use af_providers::decision::{DecisionClient, DecisionEvaluator as _};
use af_wire::decision::{DecisionApi, DecisionOutcome};
use garde::Unvalidated;
use serde_json::Value;

use super::fixture::{self, Reply, Server};

pub(super) async fn escaped_state_is_sent_whole() {
    let mut server = Server::start(vec![Reply::Json(fixture::native_reply())]).await;
    // Every newline doubles when escaped; the provider still receives all of them.
    let state = "\n".repeat(8_191);
    let mut request = fixture::request().into_inner();
    request.state = Value::String(state.clone());
    let request = Unvalidated::new(request).validate().expect("escaped state");
    let result = DecisionClient::with_client(
        server.client.clone(),
        fixture::profile(&server, DecisionApi::Typesafe),
        fixture::credential(),
    )
    .evaluate(&request)
    .await
    .expect("complete bounded state is sent");
    assert!(matches!(result.result, DecisionOutcome::Answered { .. }));
    let seen = server.next().await;
    assert_eq!(
        seen.body["state"].as_str(),
        Some(state.as_str()),
        "no truncation"
    );

    assert_eq!(server.unconsumed(), 0, "one request, one send");
    server.shutdown().await;
}
